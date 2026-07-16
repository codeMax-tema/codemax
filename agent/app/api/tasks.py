import json
import math
from enum import StrEnum
from threading import Lock
from typing import Literal

from fastapi import APIRouter, HTTPException, Request, status
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    StrictBool,
    StrictStr,
    ValidationError,
    field_validator,
)

from app.autonomous import apply_runtime_tool_result
from app.autonomous.loop import _bounded_runtime_result
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
    ToolResultStatus,
    ValidationCommandCandidate,
    ValidationResult,
    advance_state_for_workflow,
    append_log,
    utc_now,
)
from app.memory import MemoryService
from app.proposals import ProposalService
from app.scheduler import TaskScheduler
from app.tools.protocol import ToolResult
from app.validation import detect_validation_candidates

router = APIRouter(prefix="/api/v1/tasks", tags=["tasks"])
_store = CheckpointStore()
_memory = MemoryService()
_proposal_service = ProposalService()
MAX_CONCURRENT_TASKS = 2
SUPPORTED_WORKFLOW_VERSIONS = frozenset({1, 2, 3})
_TOOL_RESULT_MAX_IDENTIFIER_LENGTH = 256
_TOOL_RESULT_MAX_ARTIFACT_REF_LENGTH = 4_096
_TOOL_RESULT_MAX_STRING_BYTES = 16_000
_TOOL_RESULT_MAX_DEPTH = 12
_TOOL_RESULT_MAX_PAYLOAD_BYTES = 1_000_000

_scheduler = TaskScheduler(max_concurrent_tasks=MAX_CONCURRENT_TASKS)
_tasks_lock = Lock()
_task_locks: dict[str, Lock] = {}

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


class ToolResultRequest(AgentModel):
    """Validate Runtime callbacks before they can alter a durable V3 task."""

    model_config = ConfigDict(populate_by_name=True, extra="forbid")

    call_id: StrictStr = Field(
        alias="callId", min_length=1, max_length=_TOOL_RESULT_MAX_IDENTIFIER_LENGTH
    )
    tool_name: StrictStr = Field(
        alias="toolName", min_length=1, max_length=_TOOL_RESULT_MAX_IDENTIFIER_LENGTH
    )
    status: ToolResultStatus
    output: dict[str, object] = Field(default_factory=dict)
    error_code: StrictStr | None = Field(
        default=None, alias="errorCode", max_length=_TOOL_RESULT_MAX_IDENTIFIER_LENGTH
    )
    error_message: StrictStr | None = Field(
        default=None, alias="errorMessage", max_length=_TOOL_RESULT_MAX_STRING_BYTES
    )
    artifact_refs: list[StrictStr] = Field(
        default_factory=list, alias="artifactRefs", max_length=16
    )
    truncated: StrictBool = False

    @field_validator("call_id", "tool_name")
    @classmethod
    def require_non_blank_identifier(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("identifier is required")
        return value

    @field_validator("error_code", "error_message")
    @classmethod
    def limit_optional_text(cls, value: str | None) -> str | None:
        if value is not None and len(value.encode("utf-8")) > _TOOL_RESULT_MAX_STRING_BYTES:
            raise ValueError("text exceeds the Runtime callback limit")
        return value

    @field_validator("artifact_refs")
    @classmethod
    def validate_artifact_refs(cls, values: list[str]) -> list[str]:
        for value in values:
            if not value or len(value.encode("utf-8")) > _TOOL_RESULT_MAX_ARTIFACT_REF_LENGTH:
                raise ValueError("artifact reference exceeds the Runtime callback limit")
        return values

    @field_validator("output")
    @classmethod
    def validate_json_output(cls, value: dict[str, object]) -> dict[str, object]:
        _validate_tool_result_json(value, depth=1)
        try:
            encoded = json.dumps(value, ensure_ascii=False, allow_nan=False).encode("utf-8")
        except (TypeError, ValueError) as error:
            raise ValueError("output must be JSON serializable") from error
        if len(encoded) > _TOOL_RESULT_MAX_PAYLOAD_BYTES:
            raise ValueError("output exceeds the Runtime callback limit")
        return value

    def to_tool_result(self) -> ToolResult:
        return ToolResult(
            call_id=self.call_id,
            tool_name=self.tool_name,
            status=self.status.value,
            output=self.output,
            error_code=self.error_code,
            error_message=self.error_message,
            artifact_refs=tuple(self.artifact_refs),
            truncated=self.truncated,
        )


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
    state = load_state_or_404(task_id)
    require_supported_workflow(state)
    return state


@router.post("/{task_id}/advance", response_model=AdvanceAgentTaskResponse)
def advance_task(task_id: str, request: AdvanceAgentTaskRequest) -> AdvanceAgentTaskResponse:
    # The global lock only locates the per-task lease.  The V3 model call below
    # therefore cannot serialize unrelated Agent tasks.
    with task_lock_for(task_id):
        state = load_state_or_404(task_id)
        require_supported_workflow(state)
        scheduled = scheduler_status(state.task_id)
        if state.workflow_version in {1, 2}:
            state, scheduled = ensure_scheduler_after_checkpoint(state)
        if scheduled is not None and scheduled.status == "queued":
            state = append_log(state, "Task is queued until a scheduler slot is available.")
            state = persist_state_and_sync_scheduler(state)
            return advance_response(state)

        expected_checkpoint_index = state.checkpoint_index
        state = apply_advance_request(state, request)
        state = advance_state_for_workflow(state)
        if state.workflow_version == 3:
            revalidate_v3_lease(task_id, expected_checkpoint_index)
        state = persist_state_and_sync_scheduler(state)

    return advance_response(state)


@router.post("/{task_id}/tool-result", response_model=AdvanceAgentTaskResponse)
async def submit_tool_result_http(task_id: str, request: Request) -> AdvanceAgentTaskResponse:
    """Parse callbacks strictly so JSON NaN/Infinity become a normal 422 response."""
    try:
        payload = json.loads(
            await request.body(),
            parse_constant=_reject_non_finite_json_constant,
        )
        parsed = ToolResultRequest.model_validate(payload)
    except (json.JSONDecodeError, UnicodeDecodeError, ValueError, ValidationError) as error:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail="Invalid Runtime tool-result payload.",
        ) from error
    return submit_tool_result(task_id, parsed)


def submit_tool_result(task_id: str, request: ToolResultRequest) -> AdvanceAgentTaskResponse:
    runtime_result = request.to_tool_result()
    with task_lock_for(task_id):
        state = load_state_or_404(task_id)
        require_supported_workflow(state)
        if state.workflow_version != 3:
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Runtime tool results are supported only for workflow version 3.",
            )

        replay = classify_tool_result_delivery(state, runtime_result)
        if replay == "identical":
            return advance_response(state)
        if replay == "conflict":
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="Runtime tool result conflicts with the persisted task state.",
            )

        expected_checkpoint_index = state.checkpoint_index
        state = apply_runtime_tool_result(state, runtime_result)
        revalidate_v3_lease(task_id, expected_checkpoint_index)
        state = persist_state_and_sync_scheduler(state)

    return advance_response(state)


class FileCommitResultRequest(AgentModel):
    commit_id: str = Field(alias="commitId", min_length=1)
    success: bool
    error: str | None = None


@router.post("/{task_id}/file-commit-result", response_model=AdvanceAgentTaskResponse)
def submit_file_commit_result(
    task_id: str, request: FileCommitResultRequest
) -> AdvanceAgentTaskResponse:
    with task_lock_for(task_id):
        state = load_state_or_404(task_id)
        require_supported_workflow(state)
        if state.workflow_version == 3:
            raise HTTPException(
                status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
                detail=WORKFLOW_V3_NOT_READY_DETAIL,
            )

        if (
            state.phase != AgentPhase.AWAITING_FILE_COMMIT
            or state.pending_file_commit_id != request.commit_id
        ):
            raise HTTPException(
                status_code=status.HTTP_409_CONFLICT,
                detail="File commit result does not match the pending commit.",
            )
        if not request.success:
            message = request.error or "Rust safety service rejected the file commit."
            state = state.model_copy(
                update={
                    "phase": AgentPhase.FAILED,
                    "pending_file_commit_id": None,
                    "updated_at": utc_now(),
                }
            )
            state = append_log(state, message, "error")
        else:
            edits = [
                {"path": edit.path, "operation": edit.operation, "summary": edit.summary}
                for edit in (state.edit_plan.edits if state.edit_plan else [])
            ]
            is_repair = state.repair_round > 0
            state = state.model_copy(
                update={
                    "phase": AgentPhase.EDITING,
                    "edit_plan_applied": True,
                    "pending_file_commit_id": None,
                    "file_edits": [*state.file_edits, *edits],
                    "repair_file_edits": edits if is_repair else [],
                    "updated_at": utc_now(),
                }
            )
            state = append_log(
                state, f"File commit {request.commit_id} completed through the Rust safety service."
            )
            state = advance_state_for_workflow(state)
        state = persist_state_and_sync_scheduler(state)
    return advance_response(state)


@router.post("/{task_id}/validation-result", response_model=AdvanceAgentTaskResponse)
def submit_validation_result(
    task_id: str,
    request: ValidationResultRequest,
) -> AdvanceAgentTaskResponse:
    with task_lock_for(task_id):
        state = load_state_or_404(task_id)
        require_supported_workflow(state)
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
        state = persist_state_and_sync_scheduler(state)

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
    with task_lock_for(task_id):
        state = load_state_or_404(task_id)
        require_supported_workflow(state)
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
        state = persist_state_and_sync_scheduler(state)

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


def _reject_non_finite_json_constant(value: str) -> None:
    raise ValueError(f"non-finite JSON number: {value}")


def _validate_tool_result_json(value: object, *, depth: int) -> None:
    if depth > _TOOL_RESULT_MAX_DEPTH:
        raise ValueError("output exceeds the maximum nesting depth")
    if value is None or isinstance(value, bool) or isinstance(value, int):
        return
    if isinstance(value, float):
        if not math.isfinite(value):
            raise ValueError("output contains a non-finite number")
        return
    if isinstance(value, str):
        if len(value.encode("utf-8")) > _TOOL_RESULT_MAX_STRING_BYTES:
            raise ValueError("output string exceeds the Runtime callback limit")
        return
    if isinstance(value, list):
        for item in value:
            _validate_tool_result_json(item, depth=depth + 1)
        return
    if isinstance(value, dict):
        for key, item in value.items():
            if not isinstance(key, str):
                raise ValueError("output contains a non-string object key")
            if len(key.encode("utf-8")) > _TOOL_RESULT_MAX_IDENTIFIER_LENGTH:
                raise ValueError("output key exceeds the Runtime callback limit")
            _validate_tool_result_json(item, depth=depth + 1)
        return
    raise ValueError("output must contain JSON values only")


def task_lock_for(task_id: str) -> Lock:
    with _tasks_lock:
        return _task_locks.setdefault(task_id, Lock())


def require_supported_workflow(state: AgentState) -> None:
    if state.workflow_version not in SUPPORTED_WORKFLOW_VERSIONS:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail=f"Unsupported workflow version: {state.workflow_version}.",
        )


def classify_tool_result_delivery(
    state: AgentState, result: ToolResult
) -> Literal["accept", "identical", "conflict"]:
    try:
        _, payload_fingerprint = _bounded_runtime_result(result)
    except ValueError as error:
        raise HTTPException(
            status_code=status.HTTP_422_UNPROCESSABLE_ENTITY,
            detail=f"Invalid Runtime tool result: {error}.",
        ) from error

    for consumed in state.consumed_tool_results:
        if consumed.call_id == result.call_id:
            return (
                "identical" if consumed.payload_fingerprint == payload_fingerprint else "conflict"
            )
    if result.call_id in state.executed_tool_call_ids:
        return "conflict"

    pending = state.pending_tool_request
    if (
        state.phase != AgentPhase.WAITING_RUNTIME
        or pending is None
        or pending.call_id != result.call_id
        or pending.tool_name != result.tool_name
    ):
        return "conflict"
    return "accept"


def revalidate_v3_lease(task_id: str, expected_checkpoint_index: int) -> None:
    latest = load_state_or_404(task_id)
    if latest.checkpoint_index != expected_checkpoint_index:
        raise HTTPException(
            status_code=status.HTTP_409_CONFLICT,
            detail="Task changed while its V3 model turn was in progress.",
        )


def scheduler_status(task_id: str):
    try:
        return _scheduler.status(task_id)
    except KeyError:
        return None


def ensure_scheduler_after_checkpoint(state: AgentState):
    scheduled = scheduler_status(state.task_id)
    if scheduled is not None:
        return state, scheduled
    saved = persist_state_and_sync_scheduler(state)
    scheduled = scheduler_status(saved.task_id)
    if scheduled is None:
        raise RuntimeError(f"Scheduler did not admit persisted task: {saved.task_id}")
    return saved, scheduled


def persist_state_and_sync_scheduler(state: AgentState) -> AgentState:
    saved = _store.save(state)
    update_scheduler_from_state(saved)
    return saved


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
    if state.workflow_version == 3:
        scheduled = scheduler_status(state.task_id)
        if state.phase == AgentPhase.COMPLETED:
            if scheduled is not None:
                _scheduler.finish(state.task_id, success=True)
            return
        if state.phase == AgentPhase.CANCELLED:
            if scheduled is not None:
                _scheduler.cancel(state.task_id, "Runtime tool execution was cancelled.")
            return
        if state.phase in {AgentPhase.FAILED, AgentPhase.NEEDS_INTERVENTION}:
            if scheduled is not None:
                _scheduler.finish(state.task_id, success=False)
            return
        scheduled_task_for(state.task_id)
        return

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
