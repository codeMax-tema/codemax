from dataclasses import dataclass, field, replace
from enum import StrEnum


class AgentPhase(StrEnum):
    CREATED = "created"
    PLANNED = "planned"
    EDITING = "editing"
    VALIDATING = "validating"
    WAITING_APPROVAL = "waiting_approval"
    COMPLETED = "completed"
    FAILED = "failed"


@dataclass(frozen=True, slots=True)
class AgentTodo:
    title: str
    status: str = "pending"


@dataclass(frozen=True, slots=True)
class AgentState:
    task_id: str
    worktree_path: str
    phase: AgentPhase = AgentPhase.CREATED
    todos: tuple[AgentTodo, ...] = field(default_factory=tuple)
    repair_round: int = 0


def with_phase(state: AgentState, phase: AgentPhase) -> AgentState:
    return replace(state, phase=phase)

