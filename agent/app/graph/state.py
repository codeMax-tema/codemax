from datetime import UTC, datetime
from enum import StrEnum

from pydantic import BaseModel, ConfigDict, Field


def utc_now() -> str:
    return datetime.now(tz=UTC).isoformat()


class AgentPhase(StrEnum):
    CREATED = "created"
    PLANNED = "planned"
    EDITING = "editing"
    VALIDATING = "validating"
    ANALYZING_ERROR = "analyzing_error"
    REPAIRING = "repairing"
    WAITING_APPROVAL = "waiting_approval"
    NEEDS_INTERVENTION = "needs_intervention"
    COMPLETED = "completed"
    FAILED = "failed"


class TodoStatus(StrEnum):
    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    COMPLETED = "completed"
    FAILED = "failed"
    SKIPPED = "skipped"


class ApprovalStatus(StrEnum):
    PENDING = "pending"
    APPROVED = "approved"
    REJECTED = "rejected"
    CANCELLED = "cancelled"


class ValidationStatus(StrEnum):
    REQUESTED = "requested"
    PASSED = "passed"
    FAILED = "failed"
    CANCELLED = "cancelled"
    TIMED_OUT = "timed_out"


class AgentModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class AgentTodo(AgentModel):
    id: str
    title: str
    description: str = ""
    status: TodoStatus = TodoStatus.PENDING
    error_message: str | None = Field(default=None, alias="errorMessage")


class AgentLogEntry(AgentModel):
    id: str
    level: str = "info"
    message: str
    created_at: str = Field(default_factory=utc_now, alias="createdAt")


class TaskContext(AgentModel):
    repository_path: str = Field(alias="repositoryPath")
    title: str
    description: str = ""
    user_messages: list[str] = Field(default_factory=list, alias="userMessages")
    notes: list[str] = Field(default_factory=list)


class AgentApproval(AgentModel):
    id: str
    approval_type: str = Field(default="high_risk_operation", alias="approvalType")
    risk_level: str = Field(default="high", alias="riskLevel")
    content: str
    reason: str
    status: ApprovalStatus = ApprovalStatus.PENDING
    decision: ApprovalStatus | None = None
    comment: str | None = None
    created_at: str = Field(default_factory=utc_now, alias="createdAt")
    decided_at: str | None = Field(default=None, alias="decidedAt")


class ValidationRequest(AgentModel):
    command: str
    cwd: str
    reason: str
    status: ValidationStatus = ValidationStatus.REQUESTED
    created_at: str = Field(default_factory=utc_now, alias="createdAt")


class ValidationResult(AgentModel):
    run_id: str | None = Field(default=None, alias="runId")
    command: str
    cwd: str
    stdout: str = ""
    stderr: str = ""
    exit_code: int | None = Field(default=None, alias="exitCode")
    timed_out: bool = Field(default=False, alias="timedOut")
    cancelled: bool = False
    created_at: str = Field(default_factory=utc_now, alias="createdAt")

    @property
    def passed(self) -> bool:
        return self.exit_code == 0 and not self.timed_out and not self.cancelled


class AgentFileEdit(AgentModel):
    path: str
    operation: str
    summary: str


class AgentRepairPlan(AgentModel):
    summary: str
    suspected_causes: list[str] = Field(default_factory=list, alias="suspectedCauses")
    next_actions: list[str] = Field(default_factory=list, alias="nextActions")


class AgentProposalState(AgentModel):
    id: str
    title: str
    summary: str
    advantages: list[str] = Field(default_factory=list)
    drawbacks: list[str] = Field(default_factory=list)
    risks: list[str] = Field(default_factory=list)
    impact: str = "medium"
    estimated_effort: str = Field(default="medium", alias="estimatedEffort")
    recommended: bool = False
    rationale: str = ""


class ValidationCommandCandidate(AgentModel):
    language: str
    ecosystem: str
    command: str
    reason: str
    evidence: list[str] = Field(default_factory=list)
    priority: int = 0


class AgentState(AgentModel):
    task_id: str = Field(alias="taskId")
    repository_path: str = Field(alias="repositoryPath")
    worktree_path: str = Field(alias="worktreePath")
    title: str
    description: str = ""
    model_id: str | None = Field(default=None, alias="modelId")
    phase: AgentPhase = AgentPhase.CREATED
    context: TaskContext
    todos: list[AgentTodo] = Field(default_factory=list)
    logs: list[AgentLogEntry] = Field(default_factory=list)
    requires_approval: bool = Field(default=False, alias="requiresApproval")
    approval: AgentApproval | None = None
    validation_command: str = Field(default="python --version", alias="validationCommand")
    validation_candidates: list[ValidationCommandCandidate] = Field(
        default_factory=list,
        alias="validationCandidates",
    )
    validation_request: ValidationRequest | None = Field(default=None, alias="validationRequest")
    validation_result: ValidationResult | None = Field(default=None, alias="validationResult")
    file_edits: list[AgentFileEdit] = Field(default_factory=list, alias="fileEdits")
    repair_plan: AgentRepairPlan | None = Field(default=None, alias="repairPlan")
    proposals: list[AgentProposalState] = Field(default_factory=list)
    selected_proposal_id: str | None = Field(default=None, alias="selectedProposalId")
    repair_round: int = Field(default=0, alias="repairRound")
    max_repair_rounds: int = Field(default=5, alias="maxRepairRounds")
    checkpoint_index: int = Field(default=0, alias="checkpointIndex")
    created_at: str = Field(default_factory=utc_now, alias="createdAt")
    updated_at: str = Field(default_factory=utc_now, alias="updatedAt")


def create_initial_state(
    task_id: str,
    repository_path: str,
    worktree_path: str,
    title: str,
    description: str = "",
    model_id: str | None = None,
    validation_command: str = "python --version",
    validation_candidates: list[ValidationCommandCandidate] | None = None,
    max_repair_rounds: int = 5,
    context_notes: list[str] | None = None,
) -> AgentState:
    context = TaskContext(
        repositoryPath=repository_path,
        title=title,
        description=description,
        notes=context_notes or [],
    )
    state = AgentState(
        taskId=task_id,
        repositoryPath=repository_path,
        worktreePath=worktree_path,
        title=title,
        description=description,
        modelId=model_id,
        context=context,
        validationCommand=validation_command,
        validationCandidates=validation_candidates or [],
        maxRepairRounds=max_repair_rounds,
    )
    return append_log(state, "Agent task state created.")


def checkpoint_id(state: AgentState) -> str:
    return f"{state.task_id}:checkpoint:{state.checkpoint_index}"


def append_log(state: AgentState, message: str, level: str = "info") -> AgentState:
    entry = AgentLogEntry(
        id=f"log-{len(state.logs) + 1:04d}",
        level=level,
        message=message,
    )
    return state.model_copy(update={"logs": [*state.logs, entry], "updated_at": utc_now()})


def set_todo_status(
    state: AgentState,
    todo_id: str,
    status: TodoStatus,
    error_message: str | None = None,
) -> AgentState:
    todos = [
        todo.model_copy(update={"status": status, "error_message": error_message})
        if todo.id == todo_id
        else todo
        for todo in state.todos
    ]
    return state.model_copy(update={"todos": todos, "updated_at": utc_now()})
