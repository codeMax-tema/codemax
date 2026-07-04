from dataclasses import replace

from app.graph.state import AgentPhase, AgentState, AgentTodo, with_phase


def plan_node(state: AgentState) -> AgentState:
    todo = AgentTodo(title="Analyze task and repository context", status="completed")
    return replace(state, phase=AgentPhase.PLANNED, todos=(todo,))


def edit_node(state: AgentState) -> AgentState:
    return with_phase(state, AgentPhase.EDITING)


def validate_node(state: AgentState) -> AgentState:
    return with_phase(state, AgentPhase.VALIDATING)

