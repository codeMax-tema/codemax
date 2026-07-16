from __future__ import annotations

from pathlib import Path

from app.graph.state import AgentPhase, AgentState, AgentToolRequest, create_initial_state
from app.main import create_app
from fastapi.testclient import TestClient


class InMemoryStore:
    def __init__(self, state: AgentState) -> None:
        self.state = state
        self.saved: list[AgentState] = []

    def load(self, task_id: str) -> AgentState | None:
        return self.state if task_id == self.state.task_id else None

    def save(self, state: AgentState) -> AgentState:
        self.state = state
        self.saved.append(state)
        return state


def _waiting_for_complete_task(
    tmp_path: Path, *, task_id: str = "runtime-tool-result"
) -> AgentState:
    state = create_initial_state(
        task_id=task_id,
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Runtime tool result",
        model_id="test-model",
        workflow_version=3,
    )
    return state.model_copy(
        update={
            "phase": AgentPhase.WAITING_RUNTIME,
            "pending_tool_request": AgentToolRequest(
                callId="call-complete-1",
                toolName="complete_task",
                arguments={
                    "summary": "Finish the task",
                    "changedFiles": [],
                    "remainingRisks": [],
                },
            ),
        }
    )


def _client_for(state: AgentState, monkeypatch) -> tuple[TestClient, InMemoryStore]:
    from app.api import tasks

    store = InMemoryStore(state)
    monkeypatch.setattr(tasks, "_store", store)
    return TestClient(create_app()), store


def _completed_result(
    *, call_id: str = "call-complete-1", output: dict[str, object] | None = None
) -> dict[str, object]:
    return {
        "callId": call_id,
        "toolName": "complete_task",
        "status": "succeeded",
        "output": output
        or {
            "summary": "Task completed through Runtime.",
            "changedFiles": [],
            "remainingRisks": [],
        },
    }


def test_tool_result_api_round_trip_saves_v3_runtime_completion(
    tmp_path: Path, monkeypatch
) -> None:
    state = _waiting_for_complete_task(tmp_path)
    client, store = _client_for(state, monkeypatch)

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json=_completed_result(),
    )

    assert response.status_code == 200
    body = response.json()
    assert body["taskId"] == state.task_id
    assert body["status"] == "accepted"
    assert body["phase"] == "completed"
    assert body["checkpointId"] == f"{state.task_id}:checkpoint:0"
    assert body["state"]["phase"] == "completed"
    assert body["state"]["pendingToolRequest"] is None
    assert body["state"]["lastToolResult"]["callId"] == "call-complete-1"
    assert len(store.saved) == 1
    assert store.saved[0].phase is AgentPhase.COMPLETED


def test_tool_result_api_mismatched_call_is_saved_as_runtime_intervention(
    tmp_path: Path, monkeypatch
) -> None:
    state = _waiting_for_complete_task(tmp_path)
    client, store = _client_for(state, monkeypatch)

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json=_completed_result(call_id="call-other"),
    )

    assert response.status_code == 200
    body = response.json()
    assert body["phase"] == "needs_intervention"
    assert body["state"]["pendingToolRequest"]["callId"] == "call-complete-1"
    assert body["state"]["lastToolResult"] is None
    assert len(store.saved) == 1
    assert store.saved[0].phase is AgentPhase.NEEDS_INTERVENTION


def test_tool_result_api_returns_redacted_runtime_state(tmp_path: Path, monkeypatch) -> None:
    secret = "runtime-secret-must-not-leak"
    state = _waiting_for_complete_task(tmp_path)
    client, _store = _client_for(state, monkeypatch)

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json=_completed_result(
            output={
                "summary": "Task completed through Runtime.",
                "changedFiles": [],
                "remainingRisks": [],
                "giteeToken": secret,
            }
        ),
    )

    assert response.status_code == 200
    assert secret not in response.text
    result = response.json()["state"]["lastToolResult"]
    assert result["output"]["giteeToken"] == "[REDACTED]"


def test_tool_result_api_returns_redacted_and_budgeted_runtime_state(
    tmp_path: Path, monkeypatch
) -> None:
    secret = "runtime-secret-must-not-leak"
    state = _waiting_for_complete_task(tmp_path)
    client, _store = _client_for(state, monkeypatch)
    output = {
        "summary": "Task completed through Runtime.",
        "changedFiles": [],
        "remainingRisks": [],
        "giteeToken": secret,
        "log": "x" * 80_000,
    }

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json={**_completed_result(output=output), "truncated": True},
    )

    assert response.status_code == 200
    assert secret not in response.text
    state_payload = response.json()["state"]
    result = state_payload["lastToolResult"]
    assert result["truncated"] is True
    assert result["output"]["truncated"] is True
    assert len(state_payload["agentMessages"][-1]["content"].encode("utf-8")) <= 16_000


def test_tool_result_api_rejects_unknown_request_fields(tmp_path: Path, monkeypatch) -> None:
    state = _waiting_for_complete_task(tmp_path)
    client, store = _client_for(state, monkeypatch)

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json={**_completed_result(), "unexpected": "not permitted"},
    )

    assert response.status_code == 422
    assert store.saved == []


def test_tool_result_api_requires_a_pending_v3_tool_request(tmp_path: Path, monkeypatch) -> None:
    state = create_initial_state(
        task_id="v3-no-pending-tool-result",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="No pending Runtime tool",
        model_id="test-model",
        workflow_version=3,
    )
    client, store = _client_for(state, monkeypatch)

    response = client.post(
        f"/api/v1/tasks/{state.task_id}/tool-result",
        json=_completed_result(),
    )

    assert response.status_code == 409
    assert store.saved == []


def test_tool_result_api_remains_closed_for_legacy_workflows(tmp_path: Path, monkeypatch) -> None:
    for workflow_version in (1, 2):
        state = create_initial_state(
            task_id=f"legacy-v{workflow_version}-tool-result",
            repository_path=str(tmp_path),
            worktree_path=str(tmp_path),
            title="Legacy Runtime tool result",
            model_id="test-model",
            workflow_version=workflow_version,
        )
        client, store = _client_for(state, monkeypatch)

        response = client.post(
            f"/api/v1/tasks/{state.task_id}/tool-result",
            json=_completed_result(),
        )

        assert response.status_code == 503
        assert store.saved == []
