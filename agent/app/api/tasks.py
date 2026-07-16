from enum import StrEnum
from threading import Lock
from typing import Literal

from fastapi import APIRouter, HTTPException, status
from pydantic import BaseModel, ConfigDict, Field, field_validator

from app.core.config import AgentSettings, load_settings
from app.graph import (
    AgentPhase,
    AgentState,
    CheckpointStore,
    checkpoint_id,
    create_initial_state,
)
from app.graph.state import (
    AgentProposalState,
    ApprovalStatus,
    ValidationCommandCandidate,
    ValidationResult,
    advance_state_for_workflow,
    append_log,
    utc_now,
)
from app.memory import MemoryService
from app.proposals import ProposalService
from app.scheduler import TaskScheduler
from app.validation import detect_validation_candidates

router = APIRouter(prefix="/api/v1/tasks", tags=["tasks"])
_store = CheckpointStore()
_memory = MemoryService()
_proposal_service = ProposalService()
_scheduler = TaskScheduler(max_concurrent_tasks=2)
_tasks_lock = Lock()

WORKFLOW_V3_NOT_READY_DETAIL = "Workflow V3 autonomous runner is not ready."


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
    model_id: str = Field(alias="modelId", min_length=1)

    @field_validator("model_id")
    @classmethod
    def require_model_id(cls, value: str) -> str:
        model_id = value.strip()
        if not model_id:
            raise ValueError("modelId is required for new Agent tasks")
        return model_id
    validation_command: str | None = Field(default=None, alias="validationCommand")


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

        settings = load_settings()
        validation_command, validation_candidates = resolve_validation_command(request, settings)
        state = create_initial_state(
            task_id=request.task_id,
            repository_path=request.repository_path,
            worktree_path=request.worktree_path,
            title=request.title,
            description=request.description,
            model_id=request.model_id,
            validation_command=validation_command,
            validation_candidates=validation_candidates,
            max_repair_rounds=settings.max_repair_rounds,
            context_notes=[
                *memory_context_notes(request),
                *validation_context_notes(validation_candidates),
            ],
            workflow_version=3,
        )
        if state.workflow_version != 3:
            scheduled = _scheduler.submit(request.task_id)
            state = append_log(
                state,
                f"Scheduler admitted task as {scheduled.status}.",
            )
        state = state.model_copy(
            update={
                "proposals": proposal_states_for(request),
                "updated_at": utc_now(),
            }
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
        if state.workflow_version == 3:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail=WORKFLOW_V3_NOT_READY_DETAIL,
            )

        scheduled = scheduled_task_for(task_id)
        if scheduled.status == "queued":
            state = append_log(state, "Task is queued until a scheduler slot is available.")
            state = _store.save(state)
            return advance_response(state)

        state = apply_advance_request(state, request)
        state = advance_state_for_workflow(state)
        update_scheduler_from_state(state)
        state = _store.save(state)

    return advance_response(state)




class FileCommitResultRequest(AgentModel):
    commit_id: str = Field(alias="commitId", min_length=1)
    success: bool
    error: str | None = None


@router.post("/{task_id}/file-commit-result", response_model=AdvanceAgentTaskResponse)
def submit_file_commit_result(task_id: str, request: FileCommitResultRequest) -> AdvanceAgentTaskResponse:
    with _tasks_lock:
        state = load_state_or_404(task_id)
        if state.workflow_version == 3:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail=WORKFLOW_V3_NOT_READY_DETAIL,
            )

        if state.phase != AgentPhase.AWAITING_FILE_COMMIT or state.pending_file_commit_id != request.commit_id:
            raise HTTPException(status_code=status.HTTP_409_CONFLICT, detail="File commit result does not match the pending commit.")
        if not request.success:
            message = request.error or "Rust safety service rejected the file commit."
            state = state.model_copy(update={"phase": AgentPhase.FAILED, "pending_file_commit_id": None, "updated_at": utc_now()})
            state = append_log(state, message, "error")
        else:
            edits = [
                {"path": edit.path, "operation": edit.operation, "summary": edit.summary}
                for edit in (state.edit_plan.edits if state.edit_plan else [])
            ]
            is_repair = state.repair_round > 0
            state = state.model_copy(update={
                "phase": AgentPhase.EDITING,
                "edit_plan_applied": True,
                "pending_file_commit_id": None,
                "file_edits": [*state.file_edits, *edits],
                "repair_file_edits": edits if is_repair else [],
                "updated_at": utc_now(),
            })
            state = append_log(state, f"File commit {request.commit_id} completed through the Rust safety service.")
            state = advance_state_for_workflow(state)
        update_scheduler_from_state(state)
        state = _store.save(state)
    return advance_response(state)


@router.post("/{task_id}/validation-result", response_model=AdvanceAgentTaskResponse)
def submit_validation_result(
    task_id: str,
    request: ValidationResultRequest,
) -> AdvanceAgentTaskResponse:
    with _tasks_lock:
        state = load_state_or_404(task_id)
        if state.workflow_version == 3:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail=WORKFLOW_V3_NOT_READY_DETAIL,
            )

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
        state = advance_state_for_workflow(state)
        update_scheduler_from_state(state)
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
        if state.workflow_version == 3:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail=WORKFLOW_V3_NOT_READY_DETAIL,
            )

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
            state = advance_state_for_workflow(state)
        update_scheduler_from_state(state)
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


def resolve_validation_command(
    request: CreateAgentTaskRequest,
    settings: AgentSettings,
) -> tuple[str, list[ValidationCommandCandidate]]:
    explicit_command = (request.validation_command or "").strip()
    raw_candidates = detect_validation_candidates(request.worktree_path)
    if not raw_candidates:
        raw_candidates = detect_validation_candidates(request.repository_path)

    candidates = [
        ValidationCommandCandidate(
            language=item.language,
            ecosystem=item.ecosystem,
            command=item.command,
            reason=item.reason,
            evidence=list(item.evidence),
            priority=item.priority,
        )
        for item in raw_candidates
    ]
    if explicit_command:
        return explicit_command, candidates
    if candidates:
        return candidates[0].command, candidates
    return settings.default_validation_command, candidates


def validation_context_notes(candidates: list[ValidationCommandCandidate]) -> list[str]:
    if not candidates:
        return ["Validation command detection: no project-specific command detected."]

    summary = "; ".join(
        f"{candidate.language}/{candidate.ecosystem} -> {candidate.command}"
        for candidate in candidates[:8]
    )
    return [f"Validation command detection: {summary}."]


def proposal_states_for(request: CreateAgentTaskRequest) -> list[AgentProposalState]:
    proposals = _proposal_service.generate(request.title, request.description)
    return [
        AgentProposalState(
            id=proposal.id,
            title=proposal.title,
            summary=proposal.summary,
            advantages=proposal.advantages,
            drawbacks=proposal.drawbacks,
            risks=proposal.risks,
            impact=proposal.impact,
            estimatedEffort=proposal.estimated_effort,
            recommended=proposal.recommended,
            rationale=proposal.rationale,
        )
        for proposal in proposals
    ]


def scheduled_task_for(task_id: str):
    try:
        return _scheduler.status(task_id)
    except KeyError:
        return _scheduler.submit(task_id)


def update_scheduler_from_state(state: AgentState) -> None:
    scheduled_task_for(state.task_id)
    if state.phase == AgentPhase.COMPLETED:
        _scheduler.finish(state.task_id, success=True)
    elif state.phase == AgentPhase.FAILED:
        _scheduler.finish(state.task_id, success=False)


def load_state_or_404(task_id: str) -> AgentState:
    state = _store.load(task_id)
    if state is None:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Agent task not found: {task_id}",
        )

    return state
