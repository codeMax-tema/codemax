import json
from dataclasses import dataclass
from pathlib import Path

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
    ValidationStatus,
    append_log,
    set_todo_status,
    utc_now,
)

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


def plan_node(state: AgentState) -> AgentState:
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return state

    if state.todos:
        return state.model_copy(update={"phase": AgentPhase.PLANNED, "updated_at": utc_now()})

    state = state.model_copy(
        update={
            "phase": AgentPhase.PLANNED,
            "todos": PLAN_TODOS,
            "updated_at": utc_now(),
        }
    )
    return append_log(state, "Plan node generated the task todo list.")


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


def edit_node(state: AgentState) -> AgentState:
    if state.phase in {
        AgentPhase.WAITING_APPROVAL,
        AgentPhase.NEEDS_INTERVENTION,
        AgentPhase.COMPLETED,
        AgentPhase.FAILED,
    }:
        return state

    target_path = edit_target_path(state)
    if any(Path(edit.path) == target_path for edit in state.file_edits):
        return state

    worktree = Path(state.worktree_path).expanduser()
    if not worktree.is_dir():
        state = set_todo_status(
            state,
            "edit",
            TodoStatus.FAILED,
            f"Worktree path does not exist: {state.worktree_path}",
        )
        state = state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()})
        return append_log(
            state, f"Edit node failed; missing worktree: {state.worktree_path}.", "error"
        )

    try:
        target_path.parent.mkdir(parents=True, exist_ok=True)
        target_path.write_text(render_edit_plan(state), encoding="utf-8")
    except OSError as error:
        state = set_todo_status(state, "edit", TodoStatus.FAILED, str(error))
        state = state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()})
        return append_log(state, f"Edit node failed while writing worktree file: {error}", "error")

    edit = AgentFileEdit(
        path=str(target_path),
        operation="write",
        summary=edit_summary(state),
    )
    repair_edits, repair_logs = apply_structured_repair_directives(state)
    state = set_todo_status(state, "edit", TodoStatus.COMPLETED)
    state = state.model_copy(
        update={
            "phase": AgentPhase.EDITING,
            "file_edits": [*state.file_edits, edit, *repair_edits],
            "updated_at": utc_now(),
        }
    )
    state = append_log(state, f"Edit node wrote {target_path}.")
    for message, level in repair_logs:
        state = append_log(state, message, level)
    return state


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


def error_analysis_node(state: AgentState) -> AgentState:
    result = state.validation_result
    if result is None or result.passed:
        return state

    source = result.stderr.strip() or result.stdout.strip() or "Validation failed without output."
    lines = [line.strip() for line in source.splitlines() if line.strip()]
    tail = lines[-8:] or [source[:500]]
    repair_directives = extract_structured_repair_directives(source)
    next_actions = [
        "Inspect the failing validation output.",
        "Apply a targeted repair in the task worktree.",
        "Run the validation command again after the repair.",
    ]
    if repair_directives:
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
    state = set_todo_status(state, "edit", TodoStatus.IN_PROGRESS)
    state = state.model_copy(
        update={
            "phase": AgentPhase.REPAIRING,
            "repair_plan": plan,
            "repair_round": next_round,
            "validation_request": None,
            "validation_result": None,
            "updated_at": utc_now(),
        }
    )
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
    if state.phase in {AgentPhase.COMPLETED, AgentPhase.FAILED, AgentPhase.NEEDS_INTERVENTION}:
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


def apply_structured_repair_directives(
    state: AgentState,
) -> tuple[list[AgentFileEdit], list[tuple[str, str]]]:
    if state.repair_plan is None or state.repair_round <= 0:
        return [], []

    directives = extract_structured_repair_directives("\n".join(state.repair_plan.next_actions))
    if not directives:
        return [], []

    worktree = Path(state.worktree_path).expanduser().resolve()
    edits: list[AgentFileEdit] = []
    logs: list[tuple[str, str]] = []

    for directive in directives:
        target = safe_repair_target(worktree, directive.path)
        if target is None:
            logs.append((f"Skipped repair directive outside worktree: {directive.path}", "warning"))
            continue

        try:
            original = target.read_text(encoding="utf-8")
        except OSError as error:
            logs.append((f"Skipped repair directive for unreadable file {target}: {error}", "warning"))
            continue

        if directive.find not in original:
            logs.append((f"Skipped repair directive because text was not found in {target}.", "warning"))
            continue

        updated = original.replace(directive.find, directive.replace, 1)
        try:
            target.write_text(updated, encoding="utf-8")
        except OSError as error:
            logs.append((f"Skipped repair directive for unwritable file {target}: {error}", "error"))
            continue

        edits.append(
            AgentFileEdit(
                path=str(target),
                operation="replace",
                summary=f"Applied structured repair directive during round {state.repair_round}.",
            )
        )
        logs.append((f"Applied structured repair directive to {target}.", "info"))

    return edits, logs


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
