from __future__ import annotations

import json
import math
from types import SimpleNamespace

import pytest
from app.graph.state import AgentPhase, AgentState, AgentToolRequest, create_initial_state
from app.main import create_app
from fastapi import HTTPException
from fastapi.testclient import TestClient


class InMemoryStore:
    def __init__(self, state: AgentState) -> None:
        self.state = state
        self.saved_states: list[AgentState] = []

    def load(self, task_id: str) -> AgentState | None:
        return self.state if task_id == self.state.task_id else None

    def save(self, state: AgentState) -> AgentState:
        saved = state.model_copy(update={"checkpoint_index": state.checkpoint_index + 1})
        self.saved_states.append(saved)
        self.state = saved
        return saved


class SchedulerSpy:
    def __init__(self, task_id: str, task_status: str = "running") -> None:
        self.task_id = task_id
        self.task_status = task_status
        self.mutations: list[tuple[str, object]] = []

    def status(self, task_id: str):
        if task_id != self.task_id:
            raise KeyError(task_id)
        return SimpleNamespace(status=self.task_status)

    def submit(self, task_id: str):
        self.mutations.append(("submit", task_id))
        self.task_id = task_id
        return SimpleNamespace(status=self.task_status)

    def finish(self, task_id: str, *, success: bool):
        self.mutations.append(("finish", task_id, success))
        return SimpleNamespace(status="completed" if success else "failed")

    def cancel(self, task_id: str, message: str = ""):
        self.mutations.append(("cancel", task_id, message))
        return SimpleNamespace(status="cancelled")


def waiting_state(tmp_path, *, workflow_version: int = 3) -> AgentState:
    state = create_initial_state(
        task_id="runtime-task",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Runtime task",
        model_id="test-model",
        workflow_version=workflow_version,
    )
    return state.model_copy(
        update={
            "phase": AgentPhase.WAITING_RUNTIME,
            "pending_tool_request": AgentToolRequest(
                callId="call-search-1",
                toolName="search_text",
                arguments={"query": "AgentState"},
            ),
        }
    )


def valid_payload(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        "callId": "call-search-1",
        "toolName": "search_text",
        "status": "succeeded",
        "output": {"matches": ["agent/app/graph/state.py"]},
        "artifactRefs": ["audit://runtime/call-search-1"],
        "truncated": False,
    }
    payload.update(overrides)
    return payload


def test_v3_tool_result_round_trip_then_identical_replay_is_200_without_new_mutation(
    tmp_path, monkeypatch
) -> None:
    from app.api import tasks

    store = InMemoryStore(waiting_state(tmp_path))
    scheduler = SchedulerSpy(store.state.task_id)
    monkeypatch.setattr(tasks, "_store", store)
    monkeypatch.setattr(tasks, "_scheduler", scheduler)
    import app.autonomous.loop as autonomous_loop

    monkeypatch.setattr(
        autonomous_loop, "advance_autonomous_turn", lambda state, gateway=None: state
    )
    client = TestClient(create_app())

    first = client.post(f"/api/v1/tasks/{store.state.task_id}/tool-result", json=valid_payload())
    assert first.status_code == 200
    assert len(store.saved_states) == 1
    checkpoint_index = store.state.checkpoint_index
    scheduler_mutations = list(scheduler.mutations)

    replay = client.post(f"/api/v1/tasks/{store.state.task_id}/tool-result", json=valid_payload())

    assert replay.status_code == 200
    assert replay.json()["checkpointId"].endswith(f":{checkpoint_index}")
    assert len(store.saved_states) == 1
    assert scheduler.mutations == scheduler_mutations


@pytest.mark.parametrize(
    "payload",
    [
        valid_payload(callId="x" * 257),
        valid_payload(toolName="x" * 257),
        valid_payload(artifactRefs=["x" * 4097]),
        valid_payload(artifactRefs=[42]),
        valid_payload(output={"value": math.nan}),
        valid_payload(
            output={
                "child": {
                    "child": {
                        "child": {
                            "child": {
                                "child": {
                                    "child": {
                                        "child": {
                                            "child": {
                                                "child": {
                                                    "child": {
                                                        "child": {"child": {"child": "too deep"}}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        ),
    ],
)
def test_tool_result_http_boundary_rejects_unsafe_payloads(tmp_path, monkeypatch, payload) -> None:
    from app.api import tasks

    store = InMemoryStore(waiting_state(tmp_path))
    monkeypatch.setattr(tasks, "_store", store)
    monkeypatch.setattr(tasks, "_scheduler", SchedulerSpy(store.state.task_id))
    client = TestClient(create_app())

    response = client.post(
        f"/api/v1/tasks/{store.state.task_id}/tool-result",
        content=json.dumps(payload, allow_nan=True),
        headers={"content-type": "application/json"},
    )

    assert response.status_code == 422
    assert store.saved_states == []


def test_tool_result_mismatch_or_conflicting_replay_is_409_without_checkpoint_or_scheduler_mutation(
    tmp_path, monkeypatch
) -> None:
    from app.api import tasks

    store = InMemoryStore(waiting_state(tmp_path))
    scheduler = SchedulerSpy(store.state.task_id)
    import app.autonomous.loop as autonomous_loop

    monkeypatch.setattr(tasks, "_store", store)
    monkeypatch.setattr(tasks, "_scheduler", scheduler)
    monkeypatch.setattr(
        autonomous_loop, "advance_autonomous_turn", lambda state, gateway=None: state
    )

    with pytest.raises(HTTPException) as mismatch:
        tasks.submit_tool_result(
            store.state.task_id,
            tasks.ToolResultRequest(**valid_payload(callId="wrong-call")),
        )
    assert mismatch.value.status_code == 409
    assert store.saved_states == []
    assert scheduler.mutations == []

    accepted = tasks.submit_tool_result(
        store.state.task_id,
        tasks.ToolResultRequest(**valid_payload()),
    )
    assert accepted.phase == AgentPhase.CREATED
    saved_count = len(store.saved_states)

    with pytest.raises(HTTPException) as conflict:
        tasks.submit_tool_result(
            store.state.task_id,
            tasks.ToolResultRequest(**valid_payload(output={"matches": ["different"]})),
        )
    assert conflict.value.status_code == 409
    assert len(store.saved_states) == saved_count
    assert scheduler.mutations == []


def test_tool_result_checkpoint_save_failure_does_not_sync_scheduler(tmp_path, monkeypatch) -> None:
    from app.api import tasks

    class FailingStore(InMemoryStore):
        def save(self, state: AgentState) -> AgentState:
            raise OSError("disk full")

    store = FailingStore(waiting_state(tmp_path))
    scheduler = SchedulerSpy(store.state.task_id)
    import app.autonomous.loop as autonomous_loop

    monkeypatch.setattr(tasks, "_store", store)
    monkeypatch.setattr(tasks, "_scheduler", scheduler)
    monkeypatch.setattr(
        autonomous_loop, "advance_autonomous_turn", lambda state, gateway=None: state
    )

    with pytest.raises(OSError, match="disk full"):
        tasks.submit_tool_result(
            store.state.task_id,
            tasks.ToolResultRequest(**valid_payload()),
        )

    assert scheduler.mutations == []


def test_unknown_workflow_version_is_fail_closed_without_mutation(tmp_path, monkeypatch) -> None:
    from app.api import tasks

    store = InMemoryStore(waiting_state(tmp_path, workflow_version=4))
    scheduler = SchedulerSpy(store.state.task_id)
    monkeypatch.setattr(tasks, "_store", store)
    monkeypatch.setattr(tasks, "_scheduler", scheduler)

    with pytest.raises(HTTPException) as raised:
        tasks.submit_tool_result(
            store.state.task_id,
            tasks.ToolResultRequest(**valid_payload()),
        )

    assert raised.value.status_code == 409
    assert store.saved_states == []
    assert scheduler.mutations == []


def test_state_dispatch_rejects_unknown_workflow_versions(tmp_path) -> None:
    from app.graph.state import advance_state_for_workflow

    state = waiting_state(tmp_path, workflow_version=4)

    with pytest.raises(ValueError, match="Unsupported workflow version"):
        advance_state_for_workflow(state)


@pytest.mark.parametrize(
    ("phase", "expected_mutation"),
    [
        (AgentPhase.CANCELLED, ("cancel", "runtime-task", "Runtime tool execution was cancelled.")),
        (AgentPhase.NEEDS_INTERVENTION, ("finish", "runtime-task", False)),
    ],
)
def test_v3_terminal_runtime_states_release_the_scheduler_slot(
    tmp_path, monkeypatch, phase: AgentPhase, expected_mutation: tuple[object, ...]
) -> None:
    from app.api import tasks

    scheduler = SchedulerSpy("runtime-task")
    monkeypatch.setattr(tasks, "_scheduler", scheduler)

    tasks.update_scheduler_from_state(waiting_state(tmp_path).model_copy(update={"phase": phase}))

    assert scheduler.mutations == [expected_mutation]
