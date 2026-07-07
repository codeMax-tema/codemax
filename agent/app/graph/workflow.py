from collections.abc import Callable
from functools import cache

from langgraph.graph import END, START, StateGraph

from app.graph.nodes import (
    approval_interrupt_node,
    complete_node,
    edit_node,
    error_analysis_node,
    plan_node,
    route_after_approval,
    route_after_edit,
    route_after_error_analysis,
    route_after_validate,
    validate_node,
)
from app.graph.state import AgentState

GraphNode = Callable[[AgentState], AgentState]
GraphRouter = Callable[[AgentState], str]


def run_agent_graph(state: AgentState) -> AgentState:
    result = compiled_graph().invoke(state.model_dump(mode="json"))
    return AgentState.model_validate(result)


@cache
def compiled_graph():
    graph = StateGraph(dict)
    graph.add_node("plan", wrap_node(plan_node))
    graph.add_node("approval", wrap_node(approval_interrupt_node))
    graph.add_node("edit", wrap_node(edit_node))
    graph.add_node("validate", wrap_node(validate_node))
    graph.add_node("error_analysis", wrap_node(error_analysis_node))
    graph.add_node("complete", wrap_node(complete_node))

    graph.add_edge(START, "plan")
    graph.add_edge("plan", "approval")
    graph.add_conditional_edges(
        "approval",
        wrap_router(route_after_approval),
        {"edit": "edit", "end": END},
    )
    graph.add_conditional_edges(
        "edit",
        wrap_router(route_after_edit),
        {"validate": "validate", "end": END},
    )
    graph.add_conditional_edges(
        "validate",
        wrap_router(route_after_validate),
        {"complete": "complete", "error_analysis": "error_analysis", "end": END},
    )
    graph.add_conditional_edges(
        "error_analysis",
        wrap_router(route_after_error_analysis),
        {"edit": "edit", "end": END},
    )
    graph.add_edge("complete", END)

    return graph.compile()


def wrap_node(node: GraphNode):
    def wrapped(raw_state: dict) -> dict:
        state = AgentState.model_validate(raw_state)
        return node(state).model_dump(mode="json")

    return wrapped


def wrap_router(router: GraphRouter):
    def wrapped(raw_state: dict) -> str:
        state = AgentState.model_validate(raw_state)
        return router(state)

    return wrapped
