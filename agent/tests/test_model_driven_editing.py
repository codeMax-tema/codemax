import os
import sys
from pathlib import Path

import pytest
from pydantic import ValidationError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.editing.apply import EditSafetyError, apply_edit_plan
from app.editing.models import EditingPlan


def plan(*edits: dict[str, object]) -> EditingPlan:
    return EditingPlan.model_validate({"edits": list(edits)})


def test_editing_plan_is_strict_and_discriminates_operations() -> None:
    parsed = plan(
        {
            "operation": "create",
            "path": "src/new.txt",
            "content": "created\n",
            "summary": "Create text file",
        },
        {
            "operation": "update",
            "path": "src/existing.txt",
            "content": "updated\n",
            "summary": "Update text file",
        },
        {
            "operation": "delete",
            "path": "src/obsolete.txt",
            "summary": "Delete obsolete file",
        },
    )

    assert [edit.operation for edit in parsed.edits] == ["create", "update", "delete"]

    with pytest.raises(ValidationError):
        plan(
            {
                "operation": "delete",
                "path": "src/obsolete.txt",
                "content": "delete must not accept content",
                "summary": "Invalid delete",
            }
        )

    with pytest.raises(ValidationError):
        EditingPlan.model_validate({"edits": [], "unexpected": True})


def test_apply_create_and_update_records_relative_file_edits(tmp_path: Path) -> None:
    existing = tmp_path / "src" / "existing.txt"
    existing.parent.mkdir(parents=True)
    existing.write_text("before\n", encoding="utf-8")

    result = apply_edit_plan(
        tmp_path,
        plan(
            {
                "operation": "create",
                "path": "src/new.txt",
                "content": "created\n",
                "summary": "Create text file",
            },
            {
                "operation": "update",
                "path": "src/existing.txt",
                "content": "after\n",
                "summary": "Update text file",
            },
        ),
    )

    assert (tmp_path / "src" / "new.txt").read_text(encoding="utf-8") == "created\n"
    assert existing.read_text(encoding="utf-8") == "after\n"
    assert [(edit.operation, edit.path) for edit in result.file_edits] == [
        ("create", "src/new.txt"),
        ("update", "src/existing.txt"),
    ]
    assert result.requires_approval is False


@pytest.mark.parametrize("unsafe_path", ["../escape.txt", "nested/../../escape.txt"])
def test_apply_rejects_parent_traversal(tmp_path: Path, unsafe_path: str) -> None:
    with pytest.raises(EditSafetyError, match="relative path|workspace"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "create",
                    "path": unsafe_path,
                    "content": "unsafe",
                    "summary": "Escape workspace",
                }
            ),
        )


def test_apply_rejects_absolute_path(tmp_path: Path) -> None:
    with pytest.raises(EditSafetyError, match="relative path"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "create",
                    "path": str((tmp_path.parent / "outside.txt").resolve()),
                    "content": "unsafe",
                    "summary": "Absolute path",
                }
            ),
        )


def test_apply_rejects_symlink_escape(tmp_path: Path) -> None:
    outside = tmp_path.parent / f"{tmp_path.name}-outside"
    outside.mkdir()
    link = tmp_path / "linked"
    try:
        os.symlink(outside, link, target_is_directory=True)
    except (OSError, NotImplementedError) as error:
        pytest.skip(f"symlinks unavailable: {error}")

    with pytest.raises(EditSafetyError, match="workspace"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "create",
                    "path": "linked/escape.txt",
                    "content": "unsafe",
                    "summary": "Symlink escape",
                }
            ),
        )


def test_apply_rejects_binary_update_without_overwriting(tmp_path: Path) -> None:
    binary = tmp_path / "asset.bin"
    original = b"\xff\xfe\x00\x01"
    binary.write_bytes(original)

    with pytest.raises(EditSafetyError, match="UTF-8|binary"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "update",
                    "path": "asset.bin",
                    "content": "must not overwrite",
                    "summary": "Unsafe binary overwrite",
                }
            ),
        )

    assert binary.read_bytes() == original


def test_delete_is_returned_as_approval_required_without_mutation(tmp_path: Path) -> None:
    target = tmp_path / "obsolete.txt"
    target.write_text("keep until approved\n", encoding="utf-8")

    result = apply_edit_plan(
        tmp_path,
        plan(
            {
                "operation": "delete",
                "path": "obsolete.txt",
                "summary": "Delete obsolete file",
            }
        ),
    )

    assert result.requires_approval is True
    assert [edit.path for edit in result.pending_approval] == ["obsolete.txt"]
    assert result.file_edits == []
    assert target.read_text(encoding="utf-8") == "keep until approved\n"

class FakeGateway:
    def __init__(self) -> None:
        self.calls: list[dict[str, object]] = []

    def chat(
        self,
        messages,
        temperature=None,
        max_tokens=None,
        response_format=None,
    ):
        import json
        from types import SimpleNamespace

        self.calls.append(
            {
                "messages": messages,
                "temperature": temperature,
                "max_tokens": max_tokens,
                "response_format": response_format,
            }
        )
        system_prompt = messages[0].content
        task_prompt = messages[-1].content
        if "todo plan" in system_prompt.lower():
            task_name = "alpha" if "Alpha task" in task_prompt else "beta"
            content = {
                "todos": [
                    {
                        "id": f"{task_name}-todo",
                        "title": f"Handle {task_name}",
                        "description": f"Task-specific {task_name} work",
                    }
                ]
            }
        elif "editing plan" in system_prompt.lower():
            if "Alpha task" in task_prompt:
                content = {
                    "edits": [
                        {
                            "operation": "create",
                            "path": "alpha.txt",
                            "content": "alpha result\n",
                            "summary": "Create alpha result",
                        }
                    ]
                }
            else:
                content = {
                    "edits": [
                        {
                            "operation": "create",
                            "path": "beta.txt",
                            "content": "beta result\n",
                            "summary": "Create beta result",
                        }
                    ]
                }
        else:  # pragma: no cover - makes prompt regressions obvious
            raise AssertionError(f"Unexpected system prompt: {system_prompt}")
        return SimpleNamespace(content=json.dumps(content))


def make_state(tmp_path: Path, title: str):
    from app.graph.state import create_initial_state

    return create_initial_state(
        task_id=title.lower().replace(" ", "-"),
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title=title,
        description=f"Implement {title}",
        model_id="test-model",
    )


def test_plan_node_uses_gateway_and_persists_task_specific_todos(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes

    gateway = FakeGateway()
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    alpha = nodes.plan_node(make_state(tmp_path / "alpha", "Alpha task"))
    beta = nodes.plan_node(make_state(tmp_path / "beta", "Beta task"))

    assert [todo.title for todo in alpha.todos] == ["Handle alpha"]
    assert [todo.title for todo in beta.todos] == ["Handle beta"]
    assert alpha.todo_plan is not None
    assert beta.todo_plan is not None
    assert alpha.todo_plan != beta.todo_plan
    assert all(todo.title != "Plan the task" for todo in [*alpha.todos, *beta.todos])
    assert len(gateway.calls) == 2
    assert all(call["response_format"]["type"] == "json_schema" for call in gateway.calls)


def test_edit_node_uses_gateway_and_applies_task_specific_edit_plan(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes

    gateway = FakeGateway()
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    alpha_root = tmp_path / "alpha"
    beta_root = tmp_path / "beta"
    alpha_root.mkdir()
    beta_root.mkdir()

    alpha = nodes.edit_node(nodes.plan_node(make_state(alpha_root, "Alpha task")))
    beta = nodes.edit_node(nodes.plan_node(make_state(beta_root, "Beta task")))

    assert (alpha_root / "alpha.txt").read_text(encoding="utf-8") == "alpha result\n"
    assert (beta_root / "beta.txt").read_text(encoding="utf-8") == "beta result\n"
    assert alpha.edit_plan is not None
    assert beta.edit_plan is not None
    assert [(edit.operation, edit.path) for edit in alpha.file_edits] == [
        ("create", "alpha.txt")
    ]
    assert [(edit.operation, edit.path) for edit in beta.file_edits] == [
        ("create", "beta.txt")
    ]
    assert len(gateway.calls) == 4


class StaticGateway:
    def __init__(self, responses: list[dict[str, object]]) -> None:
        self.responses = responses

    def chat(self, messages, **kwargs):
        import json
        from types import SimpleNamespace

        return SimpleNamespace(content=json.dumps(self.responses.pop(0)))


def test_edit_node_turns_binary_update_into_failed_state(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    (tmp_path / "asset.bin").write_bytes(b"\xff\xfe\x00\x01")
    gateway = StaticGateway(
        [
            {
                "todos": [
                    {"id": "edit-binary", "title": "Edit binary", "description": "Unsafe"}
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "asset.bin",
                        "content": "overwrite",
                        "summary": "Unsafe binary overwrite",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Binary task")))

    assert state.phase == AgentPhase.FAILED
    assert state.edit_plan is not None
    assert state.file_edits == []
    assert (tmp_path / "asset.bin").read_bytes() == b"\xff\xfe\x00\x01"
    assert any("binary" in log.message.lower() or "utf-8" in log.message.lower() for log in state.logs)


def test_edit_node_delete_requires_approval_then_applies_after_approval(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ApprovalStatus

    target = tmp_path / "obsolete.txt"
    target.write_text("obsolete\n", encoding="utf-8")
    gateway = StaticGateway(
        [
            {
                "todos": [
                    {"id": "delete-file", "title": "Delete file", "description": "Cleanup"}
                ]
            },
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": "obsolete.txt",
                        "summary": "Delete obsolete file",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Delete task")))

    assert state.phase == AgentPhase.WAITING_APPROVAL
    assert state.requires_approval is True
    assert state.approval is not None
    assert target.exists()
    assert state.file_edits == []

    approved = state.model_copy(
        update={"approval": state.approval.model_copy(update={"status": ApprovalStatus.APPROVED})}
    )
    completed_edit = nodes.edit_node(approved)

    assert completed_edit.phase == AgentPhase.EDITING
    assert not target.exists()
    assert [(edit.operation, edit.path) for edit in completed_edit.file_edits] == [
        ("delete", "obsolete.txt")
    ]
