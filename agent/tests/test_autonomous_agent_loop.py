from __future__ import annotations

from app.graph.state import AgentState, create_initial_state


def test_legacy_checkpoint_loads_with_safe_autonomous_defaults(tmp_path) -> None:
    payload = create_initial_state(
        task_id="legacy-task",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Legacy task",
    ).model_dump(mode="json", by_alias=True)
    payload.pop("workflowVersion")

    restored = AgentState.model_validate(payload)

    assert restored.workflow_version == 1
    assert restored.agent_messages == []
    assert restored.pending_tool_request is None
    assert restored.last_tool_result is None
    assert restored.agent_round == 0
    assert restored.consumed_tokens == 0


def test_workflow_v3_state_round_trips_tool_history_and_budget(tmp_path) -> None:
    state = create_initial_state(
        task_id="tool-task",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Tool task",
        model_id="model-1",
        workflow_version=3,
        max_agent_rounds=24,
        token_budget=50_000,
    )
    payload = state.model_dump(mode="json", by_alias=True)
    payload.update(
        {
            "agentMessages": [
                {"role": "user", "content": "Inspect the repository."},
                {
                    "role": "assistant",
                    "content": "",
                    "toolCalls": [
                        {
                            "id": "call-search-1",
                            "name": "search_text",
                            "arguments": {"query": "AgentState"},
                        }
                    ],
                },
            ],
            "pendingToolRequest": {
                "callId": "call-search-1",
                "toolName": "search_text",
                "arguments": {"query": "AgentState"},
                "reason": "Locate the state model.",
                "status": "requested",
            },
            "lastToolResult": {
                "callId": "call-list-0",
                "toolName": "list_files",
                "status": "succeeded",
                "output": {"paths": ["agent/app/graph/state.py"]},
                "artifactRefs": [],
                "truncated": False,
            },
            "agentRound": 2,
            "consumedTokens": 420,
        }
    )

    restored = AgentState.model_validate(payload)
    round_trip = AgentState.model_validate(
        restored.model_dump(mode="json", by_alias=True)
    )

    assert round_trip.workflow_version == 3
    assert round_trip.pending_tool_request is not None
    assert round_trip.pending_tool_request.call_id == "call-search-1"
    assert round_trip.agent_messages[1].tool_calls[0].name == "search_text"
    assert round_trip.last_tool_result is not None
    assert round_trip.last_tool_result.output == {
        "paths": ["agent/app/graph/state.py"]
    }
    assert round_trip.agent_round == 2
    assert round_trip.max_agent_rounds == 24
    assert round_trip.token_budget == 50_000
    assert round_trip.consumed_tokens == 420


def test_new_programming_task_defaults_to_workflow_v3(tmp_path, monkeypatch) -> None:
    from types import SimpleNamespace

    from app.api import tasks

    class Store:
        def exists(self, _task_id: str) -> bool:
            return False

        def save(self, state: AgentState) -> AgentState:
            return state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(submit=lambda _task_id: SimpleNamespace(status="running")),
    )
    monkeypatch.setattr(tasks, "load_settings", lambda: SimpleNamespace(max_repair_rounds=5))
    monkeypatch.setattr(
        tasks,
        "resolve_validation_command",
        lambda _request, _settings: ("python --version", []),
    )
    monkeypatch.setattr(tasks, "memory_context_notes", lambda _request: [])
    monkeypatch.setattr(tasks, "validation_context_notes", lambda _candidates: [])
    monkeypatch.setattr(tasks, "proposal_states_for", lambda _request: [])

    response = tasks.create_task(
        tasks.CreateAgentTaskRequest(
            taskId="new-v3-programming-task",
            repositoryPath=str(tmp_path),
            worktreePath=str(tmp_path),
            title="New programming task",
            modelId="test-model",
        )
    )

    assert response.state.workflow_version == 3


def test_workflow_v2_dispatches_to_full_langgraph_runner(tmp_path, monkeypatch) -> None:
    import app.graph as graph
    from app.graph.state import advance_state_for_workflow

    state = create_initial_state(
        task_id="v2-recovery",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Resume V2 task",
        model_id="test-model",
        workflow_version=2,
    )
    resumed = state.model_copy(update={"checkpoint_index": 1})
    calls: list[AgentState] = []

    def run_full_graph(received: AgentState) -> AgentState:
        calls.append(received)
        return resumed

    monkeypatch.setattr(graph, "run_agent_graph", run_full_graph)

    assert advance_state_for_workflow(state) is resumed
    assert calls == [state]


def test_legacy_workflow_v1_dispatches_to_full_langgraph_runner(tmp_path, monkeypatch) -> None:
    import app.graph as graph
    from app.graph.state import advance_state_for_workflow

    payload = create_initial_state(
        task_id="legacy-v1-recovery",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Resume legacy V1 task",
        model_id="test-model",
        workflow_version=2,
    ).model_dump(mode="json", by_alias=True)
    payload.pop("workflowVersion")
    legacy_state = AgentState.model_validate(payload)
    resumed = legacy_state.model_copy(update={"checkpoint_index": 1})
    calls: list[AgentState] = []

    def run_full_graph(received: AgentState) -> AgentState:
        calls.append(received)
        return resumed

    monkeypatch.setattr(graph, "run_agent_graph", run_full_graph)

    assert legacy_state.workflow_version == 1
    assert advance_state_for_workflow(legacy_state) is resumed
    assert calls == [legacy_state]
