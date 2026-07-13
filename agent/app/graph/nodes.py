import hashlib
import json
import os
import stat
import subprocess
from dataclasses import dataclass
from pathlib import Path

from pydantic import ValidationError

from app.editing.apply import EditSafetyError, apply_edit_plan
from app.editing.models import EditingPlan, TodoPlan
from app.graph.state import (
    AgentApproval,
    AgentFileEdit,
    AgentPhase,
    AgentRepairPlan,
    AgentState,
    AgentTodo,
    ApprovalStatus,
    TodoStatus,
    ValidationRequest,
    ValidationResult,
    ValidationStatus,
    append_log,
    set_all_todo_status,
    set_todo_status,
    utc_now,
)
from app.model_gateway import ModelGatewayError, build_model_gateway
from app.privacy import redact_model_context
from app.providers import ModelMessage

EDITING_SYSTEM_PROMPT = "Generate a minimal structured editing plan. Paths must be workspace-relative. Do not include binary content."

PLAN_TODOS = [
    AgentTodo(
        id="plan",
        title="Plan the task",
        description="Create a concrete todo list from the task context.",
        status=TodoStatus.COMPLETED,
    ),
    AgentTodo(
        id="edit",
        title="Generate a worktree edit",
        description="Create a safe generated edit inside the task worktree.",
    ),
    AgentTodo(
        id="validate",
        title="Request validation",
        description="Ask the Rust side to execute the configured validation command.",
    ),
    AgentTodo(
        id="error-analysis",
        title="Analyze validation errors",
        description="Turn stderr/stdout into a repair plan when validation fails.",
    ),
]


def _bounded_context_items(items: list[str], *, item_limit: int = 2_000) -> list[str]:
    return [item[:item_limit] for item in items[-20:]]


def _task_prompt(state: AgentState) -> str:
    todos_json = json.dumps(
        [todo.model_dump(mode="json") for todo in state.todos],
        ensure_ascii=False,
    )
    user_messages_json = json.dumps(
        _bounded_context_items(state.context.user_messages),
        ensure_ascii=False,
    )
    notes_json = json.dumps(
        _bounded_context_items(state.context.notes),
        ensure_ascii=False,
    )
    return redact_model_context(
        f"Task title: {state.title}\n"
        f"Task description: {state.description}\n"
        "Path policy: all generated paths must be workspace-relative.\n"
        f"User messages: {user_messages_json}\n"
        f"Context notes: {notes_json}\n"
        f"Existing todos: {todos_json}"
    )


def _json_schema_response(name: str, schema: dict[str, object]) -> dict[str, object]:
    return {
        "type": "json_schema",
        "json_schema": {
            "name": name,
            "strict": True,
            "schema": schema,
        },
    }


def _safe_model_failure(stage: str, error: Exception) -> str:
    if isinstance(error, ModelGatewayError):
        return f"{stage} model request failed: {error.code}."
    return f"{stage} model response was invalid ({type(error).__name__})."




def _uses_model_workflow(state: AgentState) -> bool:
    return state.workflow_version >= 2 and state.model_id is not None


def plan_node(state: AgentState) -> AgentState:
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return state

    if state.todos and (not _uses_model_workflow(state) or state.todo_plan is not None):
        return state.model_copy(update={"phase": AgentPhase.PLANNED, "updated_at": utc_now()})

    if not _uses_model_workflow(state):
        state = state.model_copy(
            update={
                "phase": AgentPhase.PLANNED,
                "todos": PLAN_TODOS,
                "updated_at": utc_now(),
            }
        )
        return append_log(state, "Plan node used the legacy offline todo list.")

    try:
        result = build_model_gateway().chat(
            messages=[
                ModelMessage(
                    role="system",
                    content=(
                        "Generate a task-specific todo plan. Return only JSON that matches "
                        "the supplied schema; do not use a fixed template."
                    ),
                ),
                ModelMessage(role="user", content=_task_prompt(state)),
            ],
            temperature=0,
            response_format=_json_schema_response("todo_plan", TodoPlan.model_json_schema()),
        )
        todo_plan = TodoPlan.model_validate_json(result.content)
    except (ModelGatewayError, ValidationError, ValueError) as error:
        failed = state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()})
        return append_log(failed, _safe_model_failure("Plan", error), "error")

    todos = [
        AgentTodo(
            id=todo.id,
            title=todo.title,
            description=todo.description,
            status=TodoStatus.PENDING,
        )
        for todo in todo_plan.todos
    ]
    planned = state.model_copy(
        update={
            "phase": AgentPhase.PLANNED,
            "todos": todos,
            "todo_plan": todo_plan,
            "updated_at": utc_now(),
        }
    )
    return append_log(planned, "Plan node generated a model-driven todo list.")


def approval_interrupt_node(state: AgentState) -> AgentState:
    if not state.requires_approval:
        return state

    if state.approval and state.approval.status == ApprovalStatus.APPROVED:
        return state.model_copy(update={"phase": AgentPhase.PLANNED, "updated_at": utc_now()})

    if state.approval and state.approval.status in {
        ApprovalStatus.REJECTED,
        ApprovalStatus.CANCELLED,
    }:
        return append_log(
            state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()}),
            "Approval was not granted; task is paused as failed.",
            "warning",
        )

    approval = state.approval or AgentApproval(
        id=f"approval-{state.task_id}-{state.checkpoint_index + 1}",
        content="Approve generated worktree edit and validation request.",
        reason="The task was advanced with requireApproval=true before worktree changes.",
    )
    state = state.model_copy(
        update={
            "phase": AgentPhase.WAITING_APPROVAL,
            "approval": approval,
            "updated_at": utc_now(),
        }
    )
    return append_log(state, f"Approval interrupt requested: {approval.id}.")


def _canonical_worktree_identity(worktree_path: str) -> str:
    resolved = Path(worktree_path).expanduser().resolve(strict=False)
    return os.path.normcase(str(resolved))


def _stat_identity(path: Path) -> dict[str, int]:
    return _stat_result_identity(path.stat())


def _stat_result_identity(value: os.stat_result) -> dict[str, int]:
    return {
        "device": value.st_dev,
        "inode": value.st_ino,
        "mode": value.st_mode,
        "size": value.st_size,
        "modifiedNs": value.st_mtime_ns,
        "changedNs": value.st_ctime_ns,
    }


def _delete_target_identity(
    worktree_path: str, edit_path: str, *, attempts: int = 2
) -> dict[str, object]:
    relative_path = Path(edit_path)
    if (
        relative_path.is_absolute()
        or relative_path.anchor
        or not relative_path.parts
        or relative_path == Path(".")
        or any(part == ".." for part in relative_path.parts)
    ):
        raise OSError("Delete target must be a workspace-relative file path.")

    root = Path(worktree_path).expanduser().resolve(strict=True)
    candidate = root / relative_path
    initial_path_stat = candidate.lstat()
    if stat.S_ISLNK(initial_path_stat.st_mode):
        raise OSError("Delete approval targets must not be symbolic links.")
    resolved = candidate.resolve(strict=True)
    try:
        relative = resolved.relative_to(root).as_posix()
    except ValueError as error:
        raise OSError("Delete target must remain inside the workspace.") from error
    if not stat.S_ISREG(initial_path_stat.st_mode) or not resolved.is_file():
        raise OSError("Delete approval targets must be regular workspace files.")

    digest = hashlib.sha256()
    with resolved.open("rb") as stream:
        before = os.fstat(stream.fileno())
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
        after = os.fstat(stream.fileno())
    before_identity = _stat_result_identity(before)
    after_identity = _stat_result_identity(after)
    if before_identity != after_identity:
        if attempts > 1:
            return _delete_target_identity(worktree_path, edit_path, attempts=attempts - 1)
        raise OSError("Delete target changed while approval evidence was collected.")

    final_path_stat = candidate.lstat()
    if stat.S_ISLNK(final_path_stat.st_mode):
        raise OSError("Delete target changed into a symbolic link.")
    final_identity = _stat_result_identity(final_path_stat)
    if (
        final_identity["device"] != after_identity["device"]
        or final_identity["inode"] != after_identity["inode"]
    ):
        if attempts > 1:
            return _delete_target_identity(worktree_path, edit_path, attempts=attempts - 1)
        raise OSError("Delete target identity changed while approval evidence was collected.")
    if candidate.resolve(strict=True) != resolved:
        raise OSError("Delete target identity changed while approval evidence was collected.")

    return {
        "path": relative,
        **final_identity,
        "sha256": digest.hexdigest(),
    }


def _delete_plan_fingerprint(state: AgentState, edit_plan: EditingPlan) -> str:
    root = Path(state.worktree_path).expanduser().resolve(strict=True)
    workspace_identity = _stat_identity(root)
    delete_targets = sorted(
        (
            _delete_target_identity(state.worktree_path, edit.path)
            for edit in edit_plan.edits
            if edit.operation == "delete"
        ),
        key=lambda target: str(target["path"]),
    )
    if _stat_identity(root) != workspace_identity:
        raise OSError("Workspace changed while delete approval evidence was collected.")
    payload = {
        "taskId": state.task_id,
        "worktreePath": _canonical_worktree_identity(state.worktree_path),
        "worktreeIdentity": workspace_identity,
        "repairRound": state.repair_round,
        "editPlan": edit_plan.model_dump(mode="json"),
        "deleteTargets": delete_targets,
    }
    encoded = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def _delete_approval_content(edit_plan: EditingPlan, fingerprint: str) -> str:
    paths = sorted(edit.path for edit in edit_plan.edits if edit.operation == "delete")
    return f"Approve deletion of workspace files: {', '.join(paths)} [plan:{fingerprint}]"


def _approval_matches_delete_plan(
    approval: AgentApproval | None,
    approval_content: str | None,
) -> bool:
    return bool(
        approval is not None
        and approval_content is not None
        and approval.approval_type == "model_delete"
        and approval.content == approval_content
    )


def _safe_edit_failure(error: EditSafetyError | OSError) -> str:
    if isinstance(error, OSError):
        return "Edit transaction failed because the workspace could not be updated."
    lowered = str(error).lower()
    if "binary" in lowered or "utf-8" in lowered:
        return "Edit plan was refused because a target was not safe UTF-8 text."
    return "Edit plan failed workspace safety validation."


def _sanitized_failed_edit_plan(edit_plan: EditingPlan) -> EditingPlan:
    edits: list[dict[str, str]] = []
    for edit in edit_plan.edits:
        safe_edit = {
            "operation": edit.operation,
            "path": "[REDACTED]",
            "summary": "Edit omitted after workspace safety validation.",
        }
        if edit.operation in {"create", "update"}:
            safe_edit["content"] = "[REDACTED]"
        edits.append(safe_edit)
    return EditingPlan.model_validate({"edits": edits})


def _sanitized_applied_edit_plan(edit_plan: EditingPlan) -> EditingPlan:
    edits: list[dict[str, str]] = []
    for edit in edit_plan.edits:
        safe_edit = {
            "operation": edit.operation,
            "path": edit.path,
            "summary": redact_model_context(edit.summary),
        }
        if edit.operation in {"create", "update"}:
            safe_edit["content"] = "[REDACTED]"
        edits.append(safe_edit)
    return EditingPlan.model_validate({"edits": edits})


def edit_node(state: AgentState) -> AgentState:
    if state.workflow_version < 2:
        return legacy_edit_node(state)
    if state.edit_plan_applied:
        return state.model_copy(update={"phase": AgentPhase.EDITING, "updated_at": utc_now()})

    edit_plan = state.edit_plan
    if edit_plan is None:
        try:
            gateway = build_model_gateway()
            result = gateway.chat(
                [
                    ModelMessage(role="system", content=EDITING_SYSTEM_PROMPT),
                    ModelMessage(role="user", content=_task_prompt(state)),
                ],
                temperature=0,
                response_format=_json_schema_response("editing_plan", EditingPlan.model_json_schema()),
            )
            edit_plan = EditingPlan.model_validate_json(result.content)
        except (ModelGatewayError, ValidationError, ValueError) as error:
            failed = state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()})
            failed = set_all_todo_status(failed, TodoStatus.FAILED)
            return append_log(failed, _safe_model_failure("Edit", error), "error")

    delete_approval_content = None
    if any(edit.operation == "delete" for edit in edit_plan.edits):
        delete_approval_content = _delete_approval_content(edit_plan, _delete_plan_fingerprint(state, edit_plan))
        approved = _approval_matches_delete_plan(state.approval, delete_approval_content) and bool(state.approval and state.approval.status == ApprovalStatus.APPROVED)
        if not approved:
            approval = state.approval if _approval_matches_delete_plan(state.approval, delete_approval_content) else AgentApproval(
                id=f"approval-{state.task_id}-{state.checkpoint_index + 1}", approvalType="model_delete",
                content=delete_approval_content, reason="Model-generated delete operations require explicit approval.")
            waiting = state.model_copy(update={"phase": AgentPhase.WAITING_APPROVAL, "requires_approval": True, "approval": approval, "edit_plan": edit_plan, "updated_at": utc_now()})
            return append_log(waiting, "Edit plan requires approval before deleting workspace files.")

    commit_id = state.pending_file_commit_id or f"file-commit-{state.task_id}-{state.checkpoint_index + 1}"
    pending = state.model_copy(update={
        "phase": AgentPhase.AWAITING_FILE_COMMIT,
        "edit_plan": edit_plan,
        "edit_plan_applied": False,
        "pending_file_commit_id": commit_id,
        "requires_approval": False,
        "updated_at": utc_now(),
    })
    return append_log(pending, f"File commit {commit_id} is waiting for the Rust safety service.")


def legacy_edit_node(state: AgentState) -> AgentState:
    failed = state.model_copy(update={"phase": AgentPhase.NEEDS_INTERVENTION, "updated_at": utc_now()})
    failed = set_all_todo_status(failed, TodoStatus.FAILED, "Legacy Python workspace editing is disabled.")
    return append_log(failed, "Legacy workflow cannot edit user files; recreate the task with the Rust safe-file protocol.", "error")


def validate_node(state: AgentState) -> AgentState:
    if state.phase in {
        AgentPhase.WAITING_APPROVAL,
        AgentPhase.NEEDS_INTERVENTION,
        AgentPhase.COMPLETED,
        AgentPhase.FAILED,
    }:
        return state

    if state.validation_result is not None:
        return state

    if state.validation_request is not None:
        return state.model_copy(update={"phase": AgentPhase.VALIDATING, "updated_at": utc_now()})

    request = ValidationRequest(
        command=state.validation_command,
        cwd=state.worktree_path,
        reason=validation_reason(state),
    )
    state = set_todo_status(state, "validate", TodoStatus.IN_PROGRESS)
    state = state.model_copy(
        update={
            "phase": AgentPhase.VALIDATING,
            "validation_request": request,
            "updated_at": utc_now(),
        }
    )
    return append_log(state, f"Validation node requested command: {request.command}.")


def _workspace_change_context(state: AgentState) -> str:
    root = Path(state.worktree_path).expanduser().resolve()
    sections: list[str] = []
    try:
        completed = subprocess.run(
            ["git", "-C", str(root), "diff", "--no-ext-diff", "--unified=3", "--"],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        completed = None
    if completed is not None and completed.returncode == 0 and completed.stdout.strip():
        sections.append("Git diff:\n" + completed.stdout[:24_000])

    snapshots: list[str] = []
    seen: set[Path] = set()
    for file_edit in reversed(state.file_edits[-20:]):
        relative = Path(file_edit.path)
        if relative.is_absolute() or relative.anchor:
            continue
        target = (root / relative).resolve(strict=False)
        if target in seen or not target.is_relative_to(root):
            continue
        seen.add(target)
        if not target.is_file():
            snapshots.append(f"--- {relative.as_posix()} ({file_edit.operation}; unavailable) ---")
            continue
        try:
            content = target.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            snapshots.append(f"--- {relative.as_posix()} ({file_edit.operation}; non-text) ---")
            continue
        snapshots.append(
            f"--- {relative.as_posix()} ({file_edit.operation}) ---\n{content[:8_000]}"
        )
    if snapshots:
        sections.append("Edited file snapshots:\n" + "\n".join(reversed(snapshots)))
    context = "\n\n".join(sections) or "No textual workspace diff was available."
    return redact_model_context(context)


def _bounded_validation_output(text: str, limit: int = 12_000) -> str:
    if len(text) <= limit:
        return text
    head_size = limit // 3
    tail_size = limit - head_size
    return f"{text[:head_size]}\n...[validation output truncated]...\n{text[-tail_size:]}"


def _generate_model_repair_plan(state: AgentState, result: ValidationResult) -> EditingPlan:
    prompt = redact_model_context(
        f"{_task_prompt(state)}\n"
        f"Repair round: {state.repair_round + 1} of {state.max_repair_rounds}\n"
        f"Validation command: {result.command}\n"
        f"Exit code: {result.exit_code}\n\n"
        f"Validation stdout:\n{_bounded_validation_output(result.stdout)}\n\n"
        f"Validation stderr:\n{_bounded_validation_output(result.stderr)}\n\n"
        f"Workspace changes:\n{_workspace_change_context(state)}"
    )
    response = build_model_gateway().chat(
        messages=[
            ModelMessage(
                role="system",
                content=(
                    "Generate a minimal structured repair editing plan from the real validation "
                    "output and workspace changes. Return only JSON matching the supplied schema. "
                    "Do not require special markers in validation output. Paths must be "
                    "workspace-relative."
                ),
            ),
            ModelMessage(role="user", content=prompt),
        ],
        temperature=0,
        response_format=_json_schema_response(
            "repair_editing_plan",
            EditingPlan.model_json_schema(),
        ),
    )
    return EditingPlan.model_validate_json(response.content)


def error_analysis_node(state: AgentState) -> AgentState:
    result = state.validation_result
    if result is None or result.passed:
        return state
    if result.cancelled or result.timed_out:
        status = validation_status_from_result(state)
        safe_message = (
            "Validation was cancelled; automatic repair was not started."
            if result.cancelled
            else "Validation timed out; automatic repair was not started."
        )
        state = set_all_todo_status(state, TodoStatus.FAILED, safe_message)
        update: dict[str, object] = {
            "phase": AgentPhase.NEEDS_INTERVENTION,
            "repair_plan": None,
            "updated_at": utc_now(),
        }
        if state.validation_request is not None:
            update["validation_request"] = state.validation_request.model_copy(
                update={"status": status}
            )
        state = state.model_copy(update=update)
        return append_log(state, safe_message, "warning")

    source = result.stderr.strip() or result.stdout.strip() or "Validation failed without output."
    safe_source = redact_model_context(source)
    lines = [line.strip() for line in safe_source.splitlines() if line.strip()]
    tail = lines[-8:] or [source[:500]]
    next_actions = [
        "Inspect the failing validation output.",
        "Apply a targeted repair in the task worktree.",
        "Run the validation command again after the repair.",
    ]
    if not _uses_model_workflow(state):
        repair_directives = extract_structured_repair_directives(source)
        next_actions.extend(directive.raw for directive in repair_directives)

    plan = AgentRepairPlan(
        summary="Validation failed; generated repair plan from command output.",
        suspectedCauses=tail[:3],
        nextActions=next_actions,
    )
    state = set_todo_status(
        state,
        "validate",
        TodoStatus.FAILED,
        f"Validation exit code: {result.exit_code}",
    )
    state = set_todo_status(state, "error-analysis", TodoStatus.COMPLETED)
    if state.validation_request is not None:
        state = state.model_copy(
            update={
                "validation_request": state.validation_request.model_copy(
                    update={"status": validation_status_from_result(state)}
                )
            }
        )

    if state.repair_round >= state.max_repair_rounds:
        state = set_all_todo_status(
            state,
            TodoStatus.FAILED,
            "Automatic repair limit reached; manual intervention is required.",
        )
        state = state.model_copy(
            update={
                "phase": AgentPhase.NEEDS_INTERVENTION,
                "repair_plan": plan,
                "updated_at": utc_now(),
            }
        )
        return append_log(
            state,
            (
                "Validation still failed after "
                f"{state.repair_round} repair round(s); manual intervention is required."
            ),
            "warning",
        )

    next_round = state.repair_round + 1
    update: dict[str, object] = {
        "phase": AgentPhase.REPAIRING,
        "repair_plan": plan,
        "repair_round": next_round,
        "validation_request": None,
        "validation_result": None,
        "repair_file_edits": [],
        "updated_at": utc_now(),
    }
    if _uses_model_workflow(state):
        attempt = state.model_copy(update=update)
        try:
            repair_edit_plan = _generate_model_repair_plan(state, result)
        except (ModelGatewayError, ValidationError, ValueError) as error:
            failed = attempt.model_copy(
                update={"phase": AgentPhase.FAILED, "updated_at": utc_now()}
            )
            failed = set_all_todo_status(failed, TodoStatus.FAILED)
            return append_log(failed, _safe_model_failure("Repair", error), "error")
        state = set_all_todo_status(state, TodoStatus.IN_PROGRESS)
        update.update(
            {
                "edit_plan": repair_edit_plan,
                "edit_plan_applied": False,
            }
        )
    else:
        state = set_todo_status(state, "edit", TodoStatus.IN_PROGRESS)
    state = state.model_copy(update=update)
    return append_log(
        state,
        f"Error analysis generated repair plan for round {next_round}.",
        "warning",
    )


def complete_node(state: AgentState) -> AgentState:
    if state.validation_request is not None:
        state = state.model_copy(
            update={
                "validation_request": state.validation_request.model_copy(
                    update={"status": ValidationStatus.PASSED}
                )
            }
        )

    if state.todo_plan is not None:
        state = set_all_todo_status(state, TodoStatus.COMPLETED)
    else:
        for todo_id in ["edit", "validate"]:
            state = set_todo_status(state, todo_id, TodoStatus.COMPLETED)
        if state.repair_plan is None:
            state = set_todo_status(state, "error-analysis", TodoStatus.SKIPPED)
    state = state.model_copy(update={"phase": AgentPhase.COMPLETED, "updated_at": utc_now()})
    return append_log(state, "Validation passed; task state completed.")


def route_after_approval(state: AgentState) -> str:
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return "end"
    return "edit"


def route_after_edit(state: AgentState) -> str:
    if state.phase in {
        AgentPhase.WAITING_APPROVAL,
        AgentPhase.AWAITING_FILE_COMMIT,
        AgentPhase.COMPLETED,
        AgentPhase.FAILED,
        AgentPhase.NEEDS_INTERVENTION,
    }:
        return "end"
    return "validate"


def route_after_validate(state: AgentState) -> str:
    if state.phase in {AgentPhase.FAILED, AgentPhase.NEEDS_INTERVENTION}:
        return "end"
    if state.validation_result is None:
        return "end"
    if state.validation_result.passed:
        return "complete"
    return "error_analysis"


def route_after_error_analysis(state: AgentState) -> str:
    if state.phase == AgentPhase.REPAIRING:
        return "edit"
    return "end"


def edit_target_path(state: AgentState) -> Path:
    root = Path(state.worktree_path).expanduser() / ".codemax"
    if state.repair_round > 0:
        return root / f"agent-repair-round-{state.repair_round}.md"
    return root / "agent-edit-plan.md"


def edit_summary(state: AgentState) -> str:
    if state.repair_round > 0:
        return f"Generated Codemax repair plan for validation round {state.repair_round}."
    return "Generated Codemax task edit plan inside the worktree."


def validation_reason(state: AgentState) -> str:
    if state.repair_round > 0:
        return f"Run after generated repair round {state.repair_round}."
    return "Run after generated worktree edit."


def render_edit_plan(state: AgentState) -> str:
    todo_lines = "\n".join(f"- [{todo.status}] {todo.title}" for todo in state.todos)
    validation_candidates = "\n".join(
        f"- {candidate.language}/{candidate.ecosystem}: `{candidate.command}` ({candidate.reason})"
        for candidate in state.validation_candidates
    )
    repair_section = ""
    if state.repair_plan is not None and state.repair_round > 0:
        causes = "\n".join(f"- {cause}" for cause in state.repair_plan.suspected_causes)
        actions = "\n".join(f"- {action}" for action in state.repair_plan.next_actions)
        repair_section = (
            f"\nRepair round: {state.repair_round} of {state.max_repair_rounds}\n\n"
            f"Repair summary: {state.repair_plan.summary}\n\n"
            "Suspected causes:\n"
            f"{causes or '- None captured.'}\n\n"
            "Next actions:\n"
            f"{actions or '- Re-run validation.'}\n"
        )

    return (
        "# Codemax Agent Edit Plan\n\n"
        f"Task: {state.title}\n\n"
        f"Task ID: {state.task_id}\n\n"
        f"Description: {state.description or 'No description provided.'}\n\n"
        "Generated change: this file records the Agent edit or repair output.\n\n"
        "Todos:\n"
        f"{todo_lines}\n\n"
        "Detected validation candidates:\n"
        f"{validation_candidates or '- No project-specific command detected.'}\n\n"
        f"Validation command requested from Rust: `{state.validation_command}`\n"
        f"{repair_section}"
    )


def validation_status_from_result(state: AgentState) -> ValidationStatus:
    result = state.validation_result
    if result is None:
        return ValidationStatus.REQUESTED
    if result.cancelled:
        return ValidationStatus.CANCELLED
    if result.timed_out:
        return ValidationStatus.TIMED_OUT
    if result.passed:
        return ValidationStatus.PASSED
    return ValidationStatus.FAILED


@dataclass(frozen=True, slots=True)
class StructuredRepairDirective:
    path: str
    find: str
    replace: str
    raw: str


def extract_structured_repair_directives(text: str) -> list[StructuredRepairDirective]:
    directives: list[StructuredRepairDirective] = []
    for line in text.splitlines():
        marker_index = line.find("CODEMAX_REPAIR")
        if marker_index < 0:
            continue

        payload = line[marker_index + len("CODEMAX_REPAIR") :].strip()
        payload = payload.removeprefix(":").strip()
        try:
            data = json.loads(payload)
        except json.JSONDecodeError:
            continue

        path = data.get("path")
        find = data.get("find")
        replace = data.get("replace")
        if not all(isinstance(value, str) for value in [path, find, replace]):
            continue
        if not path or not find:
            continue

        directives.append(
            StructuredRepairDirective(
                path=path,
                find=find,
                replace=replace,
                raw=f"CODEMAX_REPAIR {json.dumps(data, ensure_ascii=False)}",
            )
        )
    return directives


def apply_structured_repair_directives(state: AgentState) -> tuple[list[AgentFileEdit], list[tuple[str, str]]]:
    if state.repair_directives:
        return [], [("Legacy structured repair directives were not applied because Python workspace writes are disabled.", "warning")]
    return [], []


def safe_repair_target(worktree: Path, relative_path: str) -> Path | None:
    candidate = Path(relative_path)
    if candidate.is_absolute() or any(part == ".." for part in candidate.parts):
        return None

    target = (worktree / candidate).resolve()
    try:
        target.relative_to(worktree)
    except ValueError:
        return None
    return target if target.is_file() else None
