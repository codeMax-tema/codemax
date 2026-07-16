from __future__ import annotations

import pytest
from app.graph.state import AgentState, create_initial_state
from app.model_gateway import ModelGatewayResult
from app.providers import ModelMessage, ModelToolCall, ModelUsage
from app.tools.protocol import ToolResult
from fastapi import HTTPException


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
    round_trip = AgentState.model_validate(restored.model_dump(mode="json", by_alias=True))

    assert round_trip.workflow_version == 3
    assert round_trip.pending_tool_request is not None
    assert round_trip.pending_tool_request.call_id == "call-search-1"
    assert round_trip.agent_messages[1].tool_calls[0].name == "search_text"
    assert round_trip.last_tool_result is not None
    assert round_trip.last_tool_result.output == {"paths": ["agent/app/graph/state.py"]}
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
    submitted_task_ids: list[str] = []
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(
            submit=lambda task_id: (
                submitted_task_ids.append(task_id) or SimpleNamespace(status="running")
            )
        ),
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
    assert submitted_task_ids == []


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


def test_v3_advance_is_rejected_without_mutating_state(tmp_path, monkeypatch) -> None:
    from types import SimpleNamespace

    from app.api import tasks

    state = create_initial_state(
        task_id="v3-advance-not-ready",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="V3 advance not ready",
        model_id="test-model",
        workflow_version=3,
    )
    original_payload = state.model_dump(mode="json", by_alias=True)
    saved_states: list[AgentState] = []

    class Store:
        def load(self, _task_id: str) -> AgentState:
            return state

        def save(self, saved_state: AgentState) -> AgentState:
            saved_states.append(saved_state)
            return saved_state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(status=lambda _task_id: SimpleNamespace(status="running")),
    )

    with pytest.raises(HTTPException) as error:
        tasks.advance_task(
            state.task_id,
            tasks.AdvanceAgentTaskRequest(
                reason="Continue V3 task",
                userMessage="Please continue.",
                requireApproval=True,
            ),
        )

    assert error.value.status_code == 503
    assert error.value.detail == "Workflow V3 autonomous runner is not ready."
    assert state.model_dump(mode="json", by_alias=True) == original_payload
    assert saved_states == []


def test_v3_validation_result_without_request_is_rejected_as_not_ready(
    tmp_path, monkeypatch
) -> None:
    from types import SimpleNamespace

    from app.api import tasks
    from app.graph.state import AgentPhase

    state = create_initial_state(
        task_id="v3-validation-not-ready",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="V3 validation not ready",
        model_id="test-model",
        workflow_version=3,
    ).model_copy(update={"phase": AgentPhase.VALIDATING})
    original_payload = state.model_dump(mode="json", by_alias=True)
    saved_states: list[AgentState] = []

    class Store:
        def load(self, _task_id: str) -> AgentState:
            return state

        def save(self, saved_state: AgentState) -> AgentState:
            saved_states.append(saved_state)
            return saved_state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(status=lambda _task_id: SimpleNamespace(status="running")),
    )

    with pytest.raises(HTTPException) as error:
        tasks.submit_validation_result(
            state.task_id,
            tasks.ValidationResultRequest(exitCode=0),
        )

    assert error.value.status_code == 503
    assert error.value.detail == "Workflow V3 autonomous runner is not ready."
    assert state.model_dump(mode="json", by_alias=True) == original_payload
    assert saved_states == []


def test_v3_validation_result_with_active_request_is_rejected_without_side_effects(
    tmp_path, monkeypatch
) -> None:
    from types import SimpleNamespace

    from app.api import tasks
    from app.graph.state import AgentPhase, ValidationRequest

    validation_request = ValidationRequest(
        command="python --version",
        cwd=str(tmp_path),
        reason="Validate V3 task",
    )
    state = create_initial_state(
        task_id="v3-validation-requested-not-ready",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="V3 validation request not ready",
        model_id="test-model",
        workflow_version=3,
    ).model_copy(
        update={
            "phase": AgentPhase.VALIDATING,
            "validation_request": validation_request,
        }
    )
    original_payload = state.model_dump(mode="json", by_alias=True)
    saved_states: list[AgentState] = []
    scheduler_status_calls: list[str] = []

    class Store:
        def load(self, _task_id: str) -> AgentState:
            return state

        def save(self, saved_state: AgentState) -> AgentState:
            saved_states.append(saved_state)
            return saved_state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(
            status=lambda task_id: (
                scheduler_status_calls.append(task_id) or SimpleNamespace(status="running")
            )
        ),
    )

    with pytest.raises(HTTPException) as error:
        tasks.submit_validation_result(
            state.task_id,
            tasks.ValidationResultRequest(exitCode=0),
        )

    assert error.value.status_code == 503
    assert error.value.detail == "Workflow V3 autonomous runner is not ready."
    assert state.model_dump(mode="json", by_alias=True) == original_payload
    assert saved_states == []
    assert scheduler_status_calls == []


def test_v3_approval_resume_is_rejected_without_side_effects(tmp_path, monkeypatch) -> None:
    from types import SimpleNamespace

    from app.api import tasks
    from app.graph.state import AgentApproval, AgentPhase

    approval = AgentApproval(
        id="approval-v3-not-ready",
        content="Approve V3 task",
        reason="V3 approval resume regression coverage",
    )
    state = create_initial_state(
        task_id="v3-approval-not-ready",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="V3 approval not ready",
        model_id="test-model",
        workflow_version=3,
    ).model_copy(
        update={
            "phase": AgentPhase.WAITING_APPROVAL,
            "requires_approval": True,
            "approval": approval,
        }
    )
    original_payload = state.model_dump(mode="json", by_alias=True)
    saved_states: list[AgentState] = []
    scheduler_status_calls: list[str] = []

    class Store:
        def load(self, _task_id: str) -> AgentState:
            return state

        def save(self, saved_state: AgentState) -> AgentState:
            saved_states.append(saved_state)
            return saved_state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(
            status=lambda task_id: (
                scheduler_status_calls.append(task_id) or SimpleNamespace(status="running")
            )
        ),
    )

    with pytest.raises(HTTPException) as error:
        tasks.resume_approval(
            state.task_id,
            approval.id,
            tasks.ResumeApprovalRequest(decision=tasks.ApprovalDecision.APPROVED),
        )

    assert error.value.status_code == 503
    assert error.value.detail == "Workflow V3 autonomous runner is not ready."
    assert state.model_dump(mode="json", by_alias=True) == original_payload
    assert saved_states == []
    assert scheduler_status_calls == []


def test_v3_file_commit_result_is_rejected_without_side_effects(tmp_path, monkeypatch) -> None:
    from types import SimpleNamespace

    from app.api import tasks
    from app.graph.state import AgentPhase

    pending_commit_id = "commit-v3-not-ready"
    state = create_initial_state(
        task_id="v3-file-commit-not-ready",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="V3 file commit not ready",
        model_id="test-model",
        workflow_version=3,
    ).model_copy(
        update={
            "phase": AgentPhase.AWAITING_FILE_COMMIT,
            "pending_file_commit_id": pending_commit_id,
        }
    )
    original_payload = state.model_dump(mode="json", by_alias=True)
    saved_states: list[AgentState] = []
    scheduler_status_calls: list[str] = []

    class Store:
        def load(self, _task_id: str) -> AgentState:
            return state

        def save(self, saved_state: AgentState) -> AgentState:
            saved_states.append(saved_state)
            return saved_state

    monkeypatch.setattr(tasks, "_store", Store())
    monkeypatch.setattr(
        tasks,
        "_scheduler",
        SimpleNamespace(
            status=lambda task_id: (
                scheduler_status_calls.append(task_id) or SimpleNamespace(status="running")
            )
        ),
    )

    with pytest.raises(HTTPException) as error:
        tasks.submit_file_commit_result(
            state.task_id,
            tasks.FileCommitResultRequest(
                commitId=pending_commit_id,
                success=True,
            ),
        )

    assert error.value.status_code == 503
    assert error.value.detail == "Workflow V3 autonomous runner is not ready."
    assert state.model_dump(mode="json", by_alias=True) == original_payload
    assert saved_states == []
    assert scheduler_status_calls == []


class ScriptedGateway:
    def __init__(self, *results: ModelGatewayResult) -> None:
        self._results = list(results)
        self.requests: list[
            tuple[
                list[ModelMessage], list[dict[str, object]] | None, str | dict[str, object] | None
            ]
        ] = []

    def chat(
        self,
        messages: list[ModelMessage],
        *,
        tools: list[dict[str, object]] | None = None,
        tool_choice: str | dict[str, object] | None = None,
    ) -> ModelGatewayResult:
        self.requests.append((messages, tools, tool_choice))
        return self._results.pop(0)


def scripted_tool_result(*calls: ModelToolCall, total_tokens: int = 7) -> ModelGatewayResult:
    return ModelGatewayResult(
        id="response-1",
        request_id="request-1",
        model="test-model",
        content="",
        finish_reason="tool_calls",
        latency_ms=1.0,
        usage=ModelUsage(
            prompt_tokens=total_tokens - 2,
            completion_tokens=2,
            total_tokens=total_tokens,
        ),
        tool_calls=calls,
    )


def v3_state(tmp_path, **updates: object) -> AgentState:
    return create_initial_state(
        task_id="v3-autonomous-loop",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Inspect AgentState",
        description="Find the AgentState model before editing.",
        model_id="test-model",
        workflow_version=3,
        **updates,
    )


def test_advance_autonomous_turn_creates_runtime_request_from_scripted_search(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState","path":"agent/app"}',
            )
        )
    )

    advanced = advance_autonomous_turn(v3_state(tmp_path), gateway=gateway)

    assert advanced.phase is AgentPhase.WAITING_RUNTIME
    assert advanced.agent_round == 1
    assert advanced.consumed_tokens == 7
    assert advanced.pending_tool_request is not None
    assert advanced.pending_tool_request.call_id == "call-search-1"
    assert advanced.pending_tool_request.tool_name == "search_text"
    assert advanced.pending_tool_request.arguments == {
        "query": "AgentState",
        "path": "agent/app",
    }
    assert [message.role for message in advanced.agent_messages] == ["user", "assistant"]
    assert advanced.agent_messages[-1].tool_calls[0].id == "call-search-1"
    request_messages, tools, tool_choice = gateway.requests[0]
    assert request_messages[0].role == "user"
    assert tools is not None
    assert {tool["function"]["name"] for tool in tools} >= {"search_text", "read_file"}
    assert tool_choice == "auto"


def test_runtime_tool_result_is_backfilled_before_next_model_read_request(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState"}',
            )
        ),
        scripted_tool_result(
            ModelToolCall(
                id="call-read-2",
                name="read_file",
                arguments='{"path":"agent/app/graph/state.py","startLine":1,"lineCount":12}',
            )
        ),
    )
    waiting = advance_autonomous_turn(v3_state(tmp_path), gateway=gateway)

    advanced = apply_runtime_tool_result(
        waiting,
        ToolResult(
            call_id="call-search-1",
            tool_name="search_text",
            status="succeeded",
            output={"matches": ["agent/app/graph/state.py:201"]},
        ),
        gateway=gateway,
    )

    assert advanced.phase is AgentPhase.WAITING_RUNTIME
    assert advanced.pending_tool_request is not None
    assert advanced.pending_tool_request.call_id == "call-read-2"
    assert advanced.last_tool_result is not None
    assert advanced.last_tool_result.call_id == "call-search-1"
    second_messages, _, _ = gateway.requests[1]
    assert [message.role for message in second_messages] == ["user", "assistant", "tool"]
    assert second_messages[-1].tool_call_id == "call-search-1"
    assert '"toolName":"search_text"' in second_messages[-1].content


@pytest.mark.parametrize(
    "tool_call, error_code",
    [
        (
            ModelToolCall(id="call-unknown", name="shell", arguments='{"command":"dir"}'),
            "tool.unknown",
        ),
        (
            ModelToolCall(id="call-invalid", name="read_file", arguments='{"path":42}'),
            "tool.invalidArguments",
        ),
        (
            ModelToolCall(id="", name="read_file", arguments='{"path":"README.md"}'),
            "tool.invalidCallId",
        ),
    ],
)
def test_invalid_model_tool_call_becomes_protocol_tool_message(
    tmp_path,
    tool_call: ModelToolCall,
    error_code: str,
) -> None:
    from app.autonomous import advance_autonomous_turn
    from app.graph.state import AgentPhase

    advanced = advance_autonomous_turn(
        v3_state(tmp_path),
        gateway=ScriptedGateway(scripted_tool_result(tool_call)),
    )

    assert advanced.phase is AgentPhase.NEEDS_INTERVENTION
    assert advanced.pending_tool_request is None
    assert advanced.last_tool_result is not None
    assert advanced.last_tool_result.error_code == error_code
    assert [message.role for message in advanced.agent_messages] == ["user", "assistant", "tool"]
    expected_call_id = tool_call.id or f"model-round-{advanced.agent_round}"
    assert advanced.agent_messages[-1].tool_call_id == expected_call_id
    assert error_code in advanced.agent_messages[-1].content


def test_call_id_mismatch_requires_intervention_without_accepting_result(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    waiting = advance_autonomous_turn(
        v3_state(tmp_path),
        gateway=ScriptedGateway(
            scripted_tool_result(
                ModelToolCall(
                    id="call-search-1",
                    name="search_text",
                    arguments='{"query":"AgentState"}',
                )
            )
        ),
    )

    resolved = apply_runtime_tool_result(
        waiting,
        ToolResult(
            call_id="call-forged",
            tool_name="search_text",
            status="succeeded",
            output={},
        ),
    )

    assert resolved.phase is AgentPhase.NEEDS_INTERVENTION
    assert resolved.pending_tool_request == waiting.pending_tool_request
    assert len(resolved.agent_messages) == len(waiting.agent_messages)


def test_repeated_no_progress_tool_call_requires_intervention(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState"}',
            )
        ),
        scripted_tool_result(
            ModelToolCall(
                id="call-search-2",
                name="search_text",
                arguments='{"query":"AgentState"}',
            )
        ),
    )
    waiting = advance_autonomous_turn(v3_state(tmp_path, max_agent_rounds=4), gateway=gateway)
    state = waiting.model_copy(update={"max_duplicate_calls": 1})

    resolved = apply_runtime_tool_result(
        state,
        ToolResult(
            call_id="call-search-1",
            tool_name="search_text",
            status="succeeded",
            output={"matches": []},
        ),
        gateway=gateway,
    )

    assert resolved.phase is AgentPhase.NEEDS_INTERVENTION
    assert resolved.pending_tool_request is None
    assert resolved.consecutive_duplicate_calls == 1
    assert len(gateway.requests) == 2


def test_duplicate_model_call_id_requires_intervention(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn
    from app.graph.state import AgentMessage, AgentPhase, AgentToolCall

    state = v3_state(tmp_path).model_copy(
        update={
            "agent_messages": [
                AgentMessage(role="user", content="Inspect the state."),
                AgentMessage(
                    role="assistant",
                    toolCalls=[
                        AgentToolCall(
                            id="call-reused",
                            name="search_text",
                            arguments={"query": "AgentState"},
                        )
                    ],
                ),
            ]
        }
    )

    advanced = advance_autonomous_turn(
        state,
        gateway=ScriptedGateway(
            scripted_tool_result(
                ModelToolCall(
                    id="call-reused",
                    name="search_text",
                    arguments='{"query":"AgentState"}',
                )
            )
        ),
    )

    assert advanced.phase is AgentPhase.NEEDS_INTERVENTION
    assert advanced.pending_tool_request is None
    assert advanced.last_tool_result is not None
    assert advanced.last_tool_result.error_code == "tool.duplicateCallId"


def test_loop_fingerprint_resets_when_runtime_result_status_changes(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState","path":"agent/app"}',
            )
        ),
        scripted_tool_result(
            ModelToolCall(
                id="call-search-2",
                name="search_text",
                arguments='{"path":"agent/app","query":"AgentState"}',
            )
        ),
        scripted_tool_result(
            ModelToolCall(
                id="call-search-3",
                name="search_text",
                arguments='{"query":"AgentState","path":"agent/app"}',
            )
        ),
    )
    waiting = advance_autonomous_turn(
        v3_state(tmp_path, max_agent_rounds=4).model_copy(update={"max_duplicate_calls": 2}),
        gateway=gateway,
    )

    after_success = apply_runtime_tool_result(
        waiting,
        ToolResult(
            call_id="call-search-1",
            tool_name="search_text",
            status="succeeded",
            output={"matches": []},
        ),
        gateway=gateway,
    )
    assert after_success.phase is AgentPhase.WAITING_RUNTIME
    assert after_success.consecutive_duplicate_calls == 1

    after_failure = apply_runtime_tool_result(
        after_success,
        ToolResult(
            call_id="call-search-2",
            tool_name="search_text",
            status="failed",
            output={},
            error_code="runtime.failed",
        ),
        gateway=gateway,
    )

    assert after_failure.phase is AgentPhase.WAITING_RUNTIME
    assert after_failure.pending_tool_request is not None
    assert after_failure.pending_tool_request.call_id == "call-search-3"
    assert after_failure.consecutive_duplicate_calls == 1


def test_runtime_result_requires_waiting_runtime_and_unconsumed_pending_call(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    waiting = advance_autonomous_turn(
        v3_state(tmp_path),
        gateway=ScriptedGateway(
            scripted_tool_result(
                ModelToolCall(
                    id="call-search-1",
                    name="search_text",
                    arguments='{"query":"AgentState"}',
                )
            )
        ),
    )
    result = ToolResult(
        call_id="call-search-1",
        tool_name="search_text",
        status="succeeded",
        output={},
    )

    wrong_phase = apply_runtime_tool_result(
        waiting.model_copy(update={"phase": AgentPhase.CREATED}),
        result,
    )
    consumed = apply_runtime_tool_result(
        waiting.model_copy(update={"executed_tool_call_ids": ["call-search-1"]}),
        result,
    )

    assert wrong_phase.phase is AgentPhase.NEEDS_INTERVENTION
    assert consumed.phase is AgentPhase.NEEDS_INTERVENTION
    assert len(wrong_phase.agent_messages) == len(waiting.agent_messages)
    assert len(consumed.agent_messages) == len(waiting.agent_messages)


def test_waiting_approval_duplicate_runtime_result_is_idempotent(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-approval-1",
                name="request_approval",
                arguments=(
                    '{"approvalType":"high_risk_operation",'
                    '"content":"Inspect diff","reason":"Need confirmation"}'
                ),
            )
        )
    )
    waiting = advance_autonomous_turn(v3_state(tmp_path), gateway=gateway)
    result = ToolResult(
        call_id="call-approval-1",
        tool_name="request_approval",
        status="waiting_approval",
        output={"approvalId": "approval-1"},
    )

    awaiting_approval = apply_runtime_tool_result(waiting, result, gateway=gateway)
    replayed = apply_runtime_tool_result(awaiting_approval, result, gateway=gateway)

    assert awaiting_approval.phase is AgentPhase.WAITING_APPROVAL
    assert replayed is awaiting_approval
    assert [message.role for message in replayed.agent_messages].count("tool") == 1
    assert replayed.executed_tool_call_ids == ["call-approval-1"]


def test_workflow_versions_after_v3_use_the_autonomous_loop(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn
    from app.graph.state import AgentPhase

    advanced = advance_autonomous_turn(
        v3_state(tmp_path).model_copy(update={"workflow_version": 4}),
        gateway=ScriptedGateway(
            scripted_tool_result(
                ModelToolCall(
                    id="call-search-v4",
                    name="search_text",
                    arguments='{"query":"AgentState"}',
                )
            )
        ),
    )

    assert advanced.phase is AgentPhase.WAITING_RUNTIME
    assert advanced.pending_tool_request is not None
    assert advanced.pending_tool_request.call_id == "call-search-v4"


@pytest.mark.parametrize(
    "state_updates, expected_rounds",
    [
        ({"max_agent_rounds": 1, "agent_round": 1}, 1),
        ({"token_budget": 7, "consumed_tokens": 7}, 0),
    ],
)
def test_round_or_budget_limit_prevents_another_model_turn(
    tmp_path,
    state_updates: dict[str, object],
    expected_rounds: int,
) -> None:
    from app.autonomous import advance_autonomous_turn
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState"}',
            )
        )
    )
    state = v3_state(tmp_path).model_copy(update=state_updates)

    stopped = advance_autonomous_turn(state, gateway=gateway)

    assert stopped.phase is AgentPhase.NEEDS_INTERVENTION
    assert stopped.agent_round == expected_rounds
    assert stopped.pending_tool_request is None
    assert gateway.requests == []


def _waiting_for_complete_task(tmp_path) -> AgentState:
    from app.graph.state import AgentPhase, AgentToolRequest, ToolRequestStatus

    return v3_state(tmp_path).model_copy(
        update={
            "phase": AgentPhase.WAITING_RUNTIME,
            "pending_tool_request": AgentToolRequest(
                callId="call-complete-1",
                toolName="complete_task",
                arguments={
                    "summary": "Finished safely.",
                    "changedFiles": [],
                    "remainingRisks": [],
                },
                status=ToolRequestStatus.WAITING_RUNTIME,
            ),
        }
    )


def test_consumed_runtime_results_replay_strictly_after_failure_and_completion(tmp_path) -> None:
    from app.autonomous import advance_autonomous_turn, apply_runtime_tool_result
    from app.graph.state import AgentPhase

    gateway = ScriptedGateway(
        scripted_tool_result(
            ModelToolCall(
                id="call-search-1",
                name="search_text",
                arguments='{"query":"AgentState"}',
            )
        ),
        scripted_tool_result(
            ModelToolCall(
                id="call-complete-2",
                name="complete_task",
                arguments=(
                    '{"summary":"Search failed","changedFiles":[],"remainingRisks":[]}'
                ),
            )
        ),
    )
    waiting = advance_autonomous_turn(v3_state(tmp_path), gateway=gateway)
    failed = ToolResult(
        call_id="call-search-1",
        tool_name="search_text",
        status="failed",
        output={"details": "search service unavailable"},
        error_code="runtime.unavailable",
        error_message="Try again later.",
        artifact_refs=("runtime-log-1",),
    )

    after_failure = apply_runtime_tool_result(waiting, failed, gateway=gateway)
    replayed_failure = apply_runtime_tool_result(after_failure, failed, gateway=gateway)
    conflicted_failure = apply_runtime_tool_result(
        after_failure,
        ToolResult(
            call_id=failed.call_id,
            tool_name=failed.tool_name,
            status=failed.status,
            output={"details": "a different failure payload"},
            error_code=failed.error_code,
            error_message=failed.error_message,
            artifact_refs=failed.artifact_refs,
            truncated=failed.truncated,
        ),
    )

    assert after_failure.phase is AgentPhase.WAITING_RUNTIME
    assert replayed_failure is after_failure
    assert conflicted_failure.phase is AgentPhase.NEEDS_INTERVENTION
    assert [item.call_id for item in after_failure.consumed_tool_results] == ["call-search-1"]

    complete = ToolResult(
        call_id="call-complete-2",
        tool_name="complete_task",
        status="succeeded",
        output={"summary": "Search failed", "changedFiles": [], "remainingRisks": []},
    )
    completed = apply_runtime_tool_result(after_failure, complete)
    replayed_completion = apply_runtime_tool_result(completed, complete)
    conflicted_completion = apply_runtime_tool_result(
        completed,
        ToolResult(
            call_id=complete.call_id,
            tool_name=complete.tool_name,
            status=complete.status,
            output={"summary": "Different delivery", "changedFiles": [], "remainingRisks": []},
        ),
    )

    assert completed.phase is AgentPhase.COMPLETED
    assert replayed_completion is completed
    assert conflicted_completion.phase is AgentPhase.NEEDS_INTERVENTION


def test_runtime_result_redacts_sensitive_checkpoint_context_and_replays_raw_payload(
    tmp_path,
) -> None:
    from app.autonomous import apply_runtime_tool_result
    from app.graph.state import AgentPhase

    secret = "sk-runtime-sensitive-token"
    result = ToolResult(
        call_id="call-complete-1",
        tool_name="complete_task",
        status="succeeded",
        output={"summary": "Finished", "token": secret, "changedFiles": [], "remainingRisks": []},
    )

    completed = apply_runtime_tool_result(_waiting_for_complete_task(tmp_path), result)
    replayed = apply_runtime_tool_result(completed, result)
    checkpoint = completed.model_dump_json(by_alias=True)

    assert completed.phase is AgentPhase.COMPLETED
    assert replayed is completed
    assert secret not in checkpoint
    assert completed.last_tool_result is not None
    assert completed.last_tool_result.output["token"] == "[REDACTED]"
    assert secret not in completed.agent_messages[-1].content


def test_runtime_result_truncates_large_output_when_runtime_marks_it_truncated(tmp_path) -> None:
    from app.autonomous import apply_runtime_tool_result

    original_output = {
        "summary": "Finished",
        "log": "x" * 80_000,
        "changedFiles": [],
        "remainingRisks": [],
    }
    completed = apply_runtime_tool_result(
        _waiting_for_complete_task(tmp_path),
        ToolResult(
            call_id="call-complete-1",
            tool_name="complete_task",
            status="succeeded",
            output=original_output,
            truncated=True,
        ),
    )

    assert completed.last_tool_result is not None
    assert completed.last_tool_result.truncated is True
    assert completed.last_tool_result.output != original_output
    assert len(completed.agent_messages[-1].content.encode("utf-8")) < 20_000


def _too_deep_runtime_output() -> dict[str, object]:
    output: dict[str, object] = {"child": "too deep"}
    for _ in range(15):
        output = {"child": output}
    return output


@pytest.mark.parametrize(
    "output",
    [{"value": object()}, _too_deep_runtime_output()],
)
def test_runtime_result_invalid_or_too_deep_payload_needs_intervention_without_throwing(
    tmp_path, output: dict[str, object]
) -> None:
    from app.autonomous import apply_runtime_tool_result
    from app.graph.state import AgentPhase

    waiting = _waiting_for_complete_task(tmp_path)
    recovered = apply_runtime_tool_result(
        waiting,
        ToolResult(
            call_id="call-complete-1",
            tool_name="complete_task",
            status="succeeded",
            output=output,
        ),
    )

    assert recovered.phase is AgentPhase.NEEDS_INTERVENTION
    assert recovered.pending_tool_request == waiting.pending_tool_request
    assert recovered.agent_messages == waiting.agent_messages


def test_workflow_v4_dispatches_to_the_autonomous_runner(tmp_path, monkeypatch) -> None:
    import app.autonomous as autonomous
    from app.graph.state import advance_state_for_workflow

    state = v3_state(tmp_path).model_copy(update={"workflow_version": 4})
    resumed = state.model_copy(update={"checkpoint_index": 1})
    calls: list[AgentState] = []

    def advance(received: AgentState) -> AgentState:
        calls.append(received)
        return resumed

    monkeypatch.setattr(autonomous, "advance_autonomous_turn", advance)

    assert advance_state_for_workflow(state) is resumed
    assert calls == [state]
