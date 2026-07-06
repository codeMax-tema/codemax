from enum import StrEnum
from threading import Lock
from typing import Literal

from fastapi import APIRouter, HTTPException, status
from pydantic import BaseModel, ConfigDict, Field

from app.graph import (
    AgentPhase,
    AgentState,
    CheckpointStore,
    checkpoint_id,
    create_initial_state,
    run_agent_graph,
)
from app.graph.state import (
    ApprovalStatus,
    ValidationResult,
    append_log,
    utc_now,
)
from app.memory import MemoryService

router = APIRouter(prefix="/api/v1/tasks", tags=["tasks"])
_store = CheckpointStore()
_memory = MemoryService()
_tasks_lock = Lock()


class AgentTaskStatus(StrEnum):
    ACCEPTED = "accepted"
    CREATED = "created"


class ApprovalDecision(StrEnum):
    APPROVED = "approved"
    REJECTED = "rejected"
    CANCELLED = "cancelled"


class AgentModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class CreateAgentTaskRequest(AgentModel):
    task_id: str = Field(alias="taskId", min_length=1)
    repository_path: str = Field(alias="repositoryPath", min_length=1)
    worktree_path: str = Field(alias="worktreePath", min_length=1)
    title: str = Field(min_length=1)
    description: str = ""
    model_id: str | None = Field(default=None, alias="modelId")
    validation_command: str = Field(default="python --version", alias="validationCommand")


class CreateAgentTaskResponse(AgentModel):
    task_id: str = Field(alias="taskId")
    status: Literal[AgentTaskStatus.CREATED]
    phase: AgentPhase
    checkpoint_id: str = Field(alias="checkpointId")
    message: str
    state: AgentState


class AdvanceAgentTaskRequest(AgentModel):
    reason: str | None = None
    user_message: str | None = Field(default=None, alias="userMessage")
    require_approval: bool = Field(default=False, alias="requireApproval")


class AdvanceAgentTaskResponse(AgentModel):
    task_id: str = Field(alias="taskId")
    status: Literal[AgentTaskStatus.ACCEPTED]
    phase: AgentPhase
    checkpoint_id: str = Field(alias="checkpointId")
    state: AgentState


class ResumeApprovalRequest(AgentModel):
    decision: ApprovalDecision
    comment: str | None = None


class ResumeApprovalResponse(AgentModel):
    task_id: str = Field(alias="taskId")
    approval_id: str = Field(alias="approvalId")
    status: Literal[AgentTaskStatus.ACCEPTED]
    phase: AgentPhase
    checkpoint_id: str = Field(alias="checkpointId")
    state: AgentState


class ValidationResultRequest(AgentModel):
    run_id: str | None = Field(default=None, alias="runId")
    command: str | None = None
    cwd: str | None = None
    stdout: str = ""
    stderr: str = ""
    exit_code: int | None = Field(default=None, alias="exitCode")
    timed_out: bool = Field(default=False, alias="timedOut")
    cancelled: bool = False


@router.post("", response_model=CreateAgentTaskResponse, status_code=status.HTTP_201_CREATED)
def create_task(request: CreateAgentTaskRequest) -> CreateAgentTaskResponse:
    with _tasks_lock:
        if _store.exists(request.task_id):
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail=f"Agent task already exists: {request.task_id}",
            )

        state = create_initial_state(
            task_id=request.task_id,
            repository_path=request.repository_path,
            worktree_path=request.worktree_path,
            title=request.title,
            description=request.description,
            model_id=request.model_id,
            validation_command=request.validation_command,
            context_notes=memory_context_notes(request),
        )
        state = _store.save(state)

    return CreateAgentTaskResponse(
        taskId=state.task_id,
        status=AgentTaskStatus.CREATED,
        phase=state.phase,
        checkpointId=checkpoint_id(state),
        message="Agent task state created and checkpointed.",
        state=state,
    )


@router.get("/{task_id}", response_model=AgentState)
def get_task_state(task_id: str) -> AgentState:
    return load_state_or_404(task_id)


@router.post("/{task_id}/advance", response_model=AdvanceAgentTaskResponse)
def advance_task(task_id: str, request: AdvanceAgentTaskRequest) -> AdvanceAgentTaskResponse:
    with _tasks_lock:
        state = load_state_or_404(task_id)
        state = apply_advance_request(state, request)
        state = run_agent_graph(state)
        state = _store.save(state)

    return advance_response(state)


@router.post("/{task_id}/validation-result", response_model=AdvanceAgentTaskResponse)
def submit_validation_result(
    task_id: str,
    request: ValidationResultRequest,
) -> AdvanceAgentTaskResponse:
    with _tasks_lock:
        state = load_state_or_404(task_id)
        validation_result = ValidationResult(
            runId=request.run_id,
            command=request.command or validation_command_for(state),
            cwd=request.cwd or validation_cwd_for(state),
            stdout=request.stdout,
            stderr=request.stderr,
            exitCode=request.exit_code,
            timedOut=request.timed_out,
            cancelled=request.cancelled,
        )
        state = state.model_copy(
            update={
                "validation_result": validation_result,
                "updated_at": utc_now(),
            }
        )
        state = append_log(state, "Validation result submitted to the Agent state machine.")
        state = run_agent_graph(state)
        state = _store.save(state)

    return advance_response(state)


@router.post(
    "/{task_id}/approvals/{approval_id}/resume",
    response_model=ResumeApprovalResponse,
)
def resume_approval(
    task_id: str,
    approval_id: str,
    request: ResumeApprovalRequest,
) -> ResumeApprovalResponse:
    with _tasks_lock:
        state = load_state_or_404(task_id)
        if state.approval is None or state.approval.id != approval_id:
            raise HTTPException(
                status_code=status.HTTP_404_NOT_FOUND,
                detail=f"Agent approval not found: {approval_id}",
            )

        decision = approval_decision_to_status(request.decision)
        approval = state.approval.model_copy(
            update={
                "status": decision,
                "decision": decision,
                "comment": request.comment,
                "decided_at": utc_now(),
            }
        )
        phase = AgentPhase.PLANNED if decision == ApprovalStatus.APPROVED else AgentPhase.FAILED
        state = state.model_copy(
            update={
                "approval": approval,
                "phase": phase,
                "requires_approval": False,
                "updated_at": utc_now(),
            }
        )
        state = append_log(state, f"Approval {approval_id} resumed with decision: {decision}.")
        if decision == ApprovalStatus.APPROVED:
            state = run_agent_graph(state)
        state = _store.save(state)

    return ResumeApprovalResponse(
        taskId=state.task_id,
        approvalId=approval_id,
        status=AgentTaskStatus.ACCEPTED,
        phase=state.phase,
        checkpointId=checkpoint_id(state),
        state=state,
    )


def apply_advance_request(
    state: AgentState,
    request: AdvanceAgentTaskRequest,
) -> AgentState:
    updates: dict[str, object] = {
        "requires_approval": state.requires_approval or request.require_approval,
        "updated_at": utc_now(),
    }
    if request.user_message:
        updates["context"] = state.context.model_copy(
            update={"user_messages": [*state.context.user_messages, request.user_message]}
        )

    state = state.model_copy(update=updates)
    if request.reason:
        state = append_log(state, f"Advance requested: {request.reason}.")
    return state


def advance_response(state: AgentState) -> AdvanceAgentTaskResponse:
    return AdvanceAgentTaskResponse(
        taskId=state.task_id,
        status=AgentTaskStatus.ACCEPTED,
        phase=state.phase,
        checkpointId=checkpoint_id(state),
        state=state,
    )


def approval_decision_to_status(decision: ApprovalDecision) -> ApprovalStatus:
    if decision == ApprovalDecision.APPROVED:
        return ApprovalStatus.APPROVED
    if decision == ApprovalDecision.REJECTED:
        return ApprovalStatus.REJECTED
    return ApprovalStatus.CANCELLED


def validation_command_for(state: AgentState) -> str:
    if state.validation_request is not None:
        return state.validation_request.command
    return state.validation_command


def validation_cwd_for(state: AgentState) -> str:
    if state.validation_request is not None:
        return state.validation_request.cwd
    return state.worktree_path


def memory_context_notes(request: CreateAgentTaskRequest) -> list[str]:
    bundle = _memory.load_context(
        conversation_id=request.task_id,
        repository_path=request.repository_path,
        task_id=request.task_id,
        query=f"{request.title} {request.description}",
    )
    notes: list[str] = []
    if bundle.rolling_summary is not None:
        notes.append(f"Conversation summary: {bundle.rolling_summary.summary}")
    for memory in bundle.long_term_memories:
        notes.append(f"Memory[{memory.category}:{memory.key}]: {memory.value}")
    return notes


def load_state_or_404(task_id: str) -> AgentState:
    state = _store.load(task_id)
    if state is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Agent task not found: {task_id}",
        )

    return state
