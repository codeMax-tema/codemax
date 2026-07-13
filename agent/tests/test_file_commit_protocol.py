from pathlib import Path

from fastapi.testclient import TestClient

from app.main import create_app
from app.graph.state import AgentPhase


def test_python_editing_executor_is_disabled(tmp_path: Path) -> None:
    from app.editing.apply import EditSafetyError, apply_edit_plan
    from app.editing.models import EditingPlan
    plan = EditingPlan.model_validate({"edits": [{"operation": "create", "path": "x.txt", "content": "x", "summary": "x"}]})
    try:
        apply_edit_plan(tmp_path, plan)
    except EditSafetyError as error:
        assert "Rust safety service" in str(error)
    else:
        raise AssertionError("Python editing executor must remain disabled")
    assert not (tmp_path / "x.txt").exists()


def test_file_commit_result_requires_matching_pending_commit() -> None:
    client = TestClient(create_app())
    response = client.post("/api/v1/tasks/missing/file-commit-result", json={"commitId": "x", "success": True})
    assert response.status_code == 404


def test_model_graph_pauses_for_rust_file_commit(tmp_path: Path, monkeypatch) -> None:
    import json
    from types import SimpleNamespace

    from app.graph import create_initial_state, run_agent_graph
    from app.graph import nodes

    class Gateway:
        def chat(self, messages, **_kwargs):
            if "todo plan" in messages[0].content.lower():
                content = {"todos": [{"id": "edit", "title": "Edit", "description": ""}]}
            else:
                content = {"edits": [{"operation": "create", "path": "result.txt", "content": "ok\n", "summary": "Create result"}]}
            return SimpleNamespace(content=json.dumps(content))

    monkeypatch.setattr(nodes, "build_model_gateway", lambda: Gateway())
    state = create_initial_state(
        task_id="file-commit-pause",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="File commit pause",
        description="Create a result file",
        model_id="test-model",
        validation_command="python --version",
    )

    pending = run_agent_graph(state)

    assert pending.phase == AgentPhase.AWAITING_FILE_COMMIT
    assert pending.pending_file_commit_id
    assert pending.validation_request is None
    assert not (tmp_path / "result.txt").exists()
