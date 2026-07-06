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
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return state

    if state.file_edits:
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

    target_dir = worktree / ".codemax"
    target_path = target_dir / "agent-edit-plan.md"
    try:
        target_dir.mkdir(parents=True, exist_ok=True)
        target_path.write_text(render_edit_plan(state), encoding="utf-8")
    except OSError as error:
        state = set_todo_status(state, "edit", TodoStatus.FAILED, str(error))
        state = state.model_copy(update={"phase": AgentPhase.FAILED, "updated_at": utc_now()})
        return append_log(state, f"Edit node failed while writing worktree file: {error}", "error")

    edit = AgentFileEdit(
        path=str(target_path),
        operation="write",
        summary="Generated Codemax task edit plan inside the worktree.",
    )
    state = set_todo_status(state, "edit", TodoStatus.COMPLETED)
    state = state.model_copy(
        update={
            "phase": AgentPhase.EDITING,
            "file_edits": [*state.file_edits, edit],
            "updated_at": utc_now(),
        }
    )
    return append_log(state, f"Edit node wrote {target_path}.")


def validate_node(state: AgentState) -> AgentState:
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return state

    if state.validation_result is not None:
        return state

    if state.validation_request is not None:
        return state.model_copy(update={"phase": AgentPhase.VALIDATING, "updated_at": utc_now()})

    request = ValidationRequest(
        command=state.validation_command,
        cwd=state.worktree_path,
        reason="Run after generated worktree edit.",
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
    if result is None:
        return state

    source = result.stderr.strip() or result.stdout.strip() or "Validation failed without output."
    lines = [line.strip() for line in source.splitlines() if line.strip()]
    tail = lines[-8:] or [source[:500]]
    plan = AgentRepairPlan(
        summary="Validation failed; generated repair plan from command output.",
        suspectedCauses=tail[:3],
        nextActions=[
            "Inspect the failing validation output.",
            "Apply a targeted repair in the task worktree.",
            "Run the validation command again after the repair.",
        ],
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

    state = state.model_copy(
        update={
            "phase": AgentPhase.FAILED,
            "repair_plan": plan,
            "repair_round": state.repair_round + 1,
            "updated_at": utc_now(),
        }
    )
    return append_log(state, "Error analysis node generated a repair plan.", "warning")


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
    state = set_todo_status(state, "error-analysis", TodoStatus.SKIPPED)
    state = state.model_copy(update={"phase": AgentPhase.COMPLETED, "updated_at": utc_now()})
    return append_log(state, "Validation passed; task state completed.")


def route_after_approval(state: AgentState) -> str:
    if state.phase in {AgentPhase.WAITING_APPROVAL, AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return "end"
    return "edit"


def route_after_edit(state: AgentState) -> str:
    if state.phase in {AgentPhase.COMPLETED, AgentPhase.FAILED}:
        return "end"
    return "validate"


def route_after_validate(state: AgentState) -> str:
    if state.phase == AgentPhase.FAILED:
        return "end"
    if state.validation_result is None:
        return "end"
    if state.validation_result.passed:
        return "complete"
    return "error_analysis"


def render_edit_plan(state: AgentState) -> str:
    todo_lines = "\n".join(f"- [{todo.status}] {todo.title}" for todo in state.todos)
    return (
        "# Codemax Agent Edit Plan\n\n"
        f"Task: {state.title}\n\n"
        f"Task ID: {state.task_id}\n\n"
        f"Description: {state.description or 'No description provided.'}\n\n"
        "Generated change: this file records the S5-E03 edit node output.\n\n"
        "Todos:\n"
        f"{todo_lines}\n\n"
        f"Validation command requested from Rust: `{state.validation_command}`\n"
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
