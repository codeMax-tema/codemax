import os
import sys
from pathlib import Path

import pytest
from pydantic import ValidationError

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.api.tasks import CreateAgentTaskRequest
from app.editing.apply import EditSafetyError, apply_edit_plan
from app.editing.models import EditingPlan


def test_new_agent_task_requires_a_model_id() -> None:
    payload = {
        "taskId": "task-model-required",
        "repositoryPath": "D:/repo",
        "worktreePath": "D:/repo/.worktrees/task-model-required",
        "title": "Require a model",
    }

    with pytest.raises(ValidationError):
        CreateAgentTaskRequest.model_validate(payload)

    with pytest.raises(ValidationError):
        CreateAgentTaskRequest.model_validate({**payload, "modelId": "   "})


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



def test_apply_revalidates_target_after_preparation_before_writing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    import app.editing.apply as apply_module

    inside_parent = tmp_path / "nested"
    inside_parent.mkdir()
    inside_target = inside_parent / "target.txt"
    inside_target.write_text("inside\n", encoding="utf-8")
    outside = tmp_path.parent / f"{tmp_path.name}-race-outside"
    outside.mkdir()
    outside_target = outside / "target.txt"
    outside_target.write_text("outside\n", encoding="utf-8")
    original_prepare = apply_module._prepare_edit

    def prepare_with_changed_target(root: Path, edit: object):
        prepared = original_prepare(root, edit)
        return type(prepared)(
            edit=prepared.edit,
            target=outside_target,
            relative_path=prepared.relative_path,
        )

    monkeypatch.setattr(apply_module, "_prepare_edit", prepare_with_changed_target)

    with pytest.raises(EditSafetyError, match="workspace|changed"):
        apply_module.apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "update",
                    "path": "nested/target.txt",
                    "content": "mutated\n",
                    "summary": "Exercise operation-time path validation",
                }
            ),
        )

    assert outside_target.read_text(encoding="utf-8") == "outside\n"

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



def test_historical_checkpoint_without_workflow_version_keeps_legacy_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentState, AgentTodo

    current = make_state(tmp_path / "historical", "Historical task").model_copy(
        update={"todos": [AgentTodo(id="legacy", title="Legacy todo")]}
    )
    payload = current.model_dump(mode="json", by_alias=True)
    payload.pop("workflowVersion", None)
    historical = AgentState.model_validate(payload)

    class UnexpectedGateway:
        def chat(self, *args, **kwargs):
            raise AssertionError("historical checkpoints must not call the model planner")

    monkeypatch.setattr(nodes, "build_model_gateway", lambda: UnexpectedGateway())

    planned = nodes.plan_node(historical)

    assert planned.workflow_version == 1
    assert [todo.title for todo in planned.todos] == ["Legacy todo"]


def test_plan_node_replaces_legacy_todos_for_a_real_model_task(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentTodo

    gateway = FakeGateway()
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = make_state(tmp_path / "alpha", "Alpha task").model_copy(
        update={
            "todos": [AgentTodo(id="legacy", title="Plan the task")],
            "todo_plan": None,
        }
    )

    planned = nodes.plan_node(state)

    assert [todo.title for todo in planned.todos] == ["Handle alpha"]
    assert planned.todo_plan is not None
    assert len(gateway.calls) == 1


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
    assert [(edit.operation, edit.path) for edit in alpha.file_edits] == [("create", "alpha.txt")]
    assert [(edit.operation, edit.path) for edit in beta.file_edits] == [("create", "beta.txt")]
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
            {"todos": [{"id": "edit-binary", "title": "Edit binary", "description": "Unsafe"}]},
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
    assert any(
        "binary" in log.message.lower() or "utf-8" in log.message.lower() for log in state.logs
    )


def test_edit_node_delete_requires_approval_then_applies_after_approval(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ApprovalStatus

    target = tmp_path / "obsolete.txt"
    target.write_text("obsolete\n", encoding="utf-8")
    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete-file", "title": "Delete file", "description": "Cleanup"}]},
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




def test_model_delete_parent_traversal_fails_safely_without_reading_outside_workspace(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    outside = tmp_path.parent / f"{tmp_path.name}-outside-delete.txt"
    outside.write_text("protected\n", encoding="utf-8")
    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete", "title": "Delete", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": f"../{outside.name}",
                        "summary": "Unsafe delete",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    result = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Unsafe delete")))

    assert result.phase == AgentPhase.FAILED
    assert result.approval is None
    assert outside.read_text(encoding="utf-8") == "protected\n"
    assert outside.name not in result.model_dump_json(by_alias=True)

def test_model_delete_resolved_escape_fails_safely(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    candidate = tmp_path / "linked" / "protected.txt"
    candidate.parent.mkdir()
    candidate.write_text("workspace copy\n", encoding="utf-8")
    outside = tmp_path.parent / f"{tmp_path.name}-resolved-outside.txt"
    outside.write_text("protected\n", encoding="utf-8")
    original_resolve = Path.resolve

    def resolve_with_escape(self: Path, strict: bool = False) -> Path:
        if self == candidate:
            return outside
        return original_resolve(self, strict=strict)

    monkeypatch.setattr(Path, "resolve", resolve_with_escape)
    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete", "title": "Delete", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": "linked/protected.txt",
                        "summary": "Unsafe delete",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    result = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Unsafe delete")))

    assert result.phase == AgentPhase.FAILED
    assert result.approval is None
    assert outside.read_text(encoding="utf-8") == "protected\n"


def test_model_delete_symlink_parent_escape_fails_safely(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    outside = tmp_path.parent / f"{tmp_path.name}-outside-delete-dir"
    outside.mkdir()
    protected = outside / "protected.txt"
    protected.write_text("protected\n", encoding="utf-8")
    link = tmp_path / "linked"
    try:
        os.symlink(outside, link, target_is_directory=True)
    except (OSError, NotImplementedError) as error:
        pytest.skip(f"symlinks unavailable: {error}")

    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete", "title": "Delete", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": "linked/protected.txt",
                        "summary": "Unsafe delete",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    result = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Unsafe delete")))

    assert result.phase == AgentPhase.FAILED
    assert result.approval is None
    assert protected.read_text(encoding="utf-8") == "protected\n"


def test_delete_approval_is_invalidated_when_target_changes_after_review(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ApprovalStatus

    target = tmp_path / "mutable.txt"
    target.write_text("reviewed content\n", encoding="utf-8")
    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete", "title": "Delete", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": "mutable.txt",
                        "summary": "Delete mutable file",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    waiting = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Delete reviewed file")))
    assert waiting.approval is not None
    reviewed_content = waiting.approval.content

    target.write_text("replacement content\n", encoding="utf-8")
    approved = waiting.model_copy(
        update={"approval": waiting.approval.model_copy(update={"status": ApprovalStatus.APPROVED})}
    )
    replayed = nodes.edit_node(approved)

    assert replayed.phase == AgentPhase.WAITING_APPROVAL
    assert replayed.approval is not None
    assert replayed.approval.status == ApprovalStatus.PENDING
    assert replayed.approval.content != reviewed_content
    assert target.read_text(encoding="utf-8") == "replacement content\n"

def test_unrelated_approved_action_cannot_authorize_model_deletion(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentApproval, AgentPhase, ApprovalStatus

    target = tmp_path / "protected.txt"
    target.write_text("keep\n", encoding="utf-8")
    gateway = StaticGateway(
        [
            {"todos": [{"id": "delete", "title": "Delete", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "delete",
                        "path": "protected.txt",
                        "summary": "Delete protected file",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = make_state(tmp_path, "Delete protected").model_copy(
        update={
            "approval": AgentApproval(
                id="approval-unrelated",
                content="Approve network access",
                reason="Unrelated action",
                status=ApprovalStatus.APPROVED,
            )
        }
    )

    result = nodes.edit_node(nodes.plan_node(state))

    assert result.phase == AgentPhase.WAITING_APPROVAL
    assert result.approval is not None
    assert result.approval.approval_type == "model_delete"
    assert target.exists()



def test_successful_model_edit_does_not_persist_generated_content_in_state(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes

    secret_content = "fictional-sensitive-generated-content"
    secret_summary = "token fictional-sensitive-summary"
    gateway = StaticGateway(
        [
            {"todos": [{"id": "write", "title": "Write", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": "generated.txt",
                        "content": secret_content,
                        "summary": secret_summary,
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    edited = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Persist safely")))

    assert (tmp_path / "generated.txt").read_text(encoding="utf-8") == secret_content
    serialized = edited.model_dump_json(by_alias=True)
    assert secret_content not in serialized
    assert "fictional-sensitive-summary" not in serialized
    assert edited.edit_plan is not None
    assert edited.edit_plan.edits[0].content == "[REDACTED]"
    assert edited.file_edits[0].summary == "token [REDACTED]"

def test_model_edit_plan_is_not_reapplied_after_success(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    gateway = FakeGateway()
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    root = tmp_path / "alpha"
    root.mkdir()

    edited = nodes.edit_node(nodes.plan_node(make_state(root, "Alpha task")))
    resumed = nodes.edit_node(nodes.plan_node(edited))

    assert resumed.phase == AgentPhase.EDITING
    assert (root / "alpha.txt").read_text(encoding="utf-8") == "alpha result\n"
    assert [(edit.operation, edit.path) for edit in resumed.file_edits] == [("create", "alpha.txt")]
    assert len(gateway.calls) == 2


def test_apply_edit_plan_rolls_back_all_files_when_later_write_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    first = tmp_path / "first.txt"
    second = tmp_path / "second.txt"
    first.write_text("first before\n", encoding="utf-8")
    second.write_text("second before\n", encoding="utf-8")
    original_write_text = Path.write_text

    def fail_second_write(path: Path, data: str, *args, **kwargs) -> int:
        if path == second:
            raise OSError("simulated second write failure")
        return original_write_text(path, data, *args, **kwargs)

    monkeypatch.setattr(Path, "write_text", fail_second_write)

    with pytest.raises(OSError, match="simulated second write failure"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "update",
                    "path": "first.txt",
                    "content": "first after\n",
                    "summary": "Update first file",
                },
                {
                    "operation": "update",
                    "path": "second.txt",
                    "content": "second after\n",
                    "summary": "Update second file",
                },
            ),
        )

    assert first.read_text(encoding="utf-8") == "first before\n"
    assert second.read_text(encoding="utf-8") == "second before\n"


def test_invalid_model_payload_is_not_written_to_persistent_logs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    secret = "super-secret-model-payload"
    gateway = StaticGateway(
        [
            {
                "todos": [
                    {
                        "id": "unsafe",
                        "title": "Unsafe payload",
                        "description": "",
                        "secret": secret,
                    }
                ]
            }
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    failed = nodes.plan_node(make_state(tmp_path, "Secret task"))

    assert failed.phase == AgentPhase.FAILED
    assert all(secret not in log.message for log in failed.logs)


def test_complete_node_marks_all_model_generated_todos_completed(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import TodoStatus, ValidationResult

    gateway = FakeGateway()
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    root = tmp_path / "alpha"
    root.mkdir()
    edited = nodes.edit_node(nodes.plan_node(make_state(root, "Alpha task")))
    validated = edited.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="run-1",
                command="python --version",
                cwd=str(root),
                exitCode=0,
            )
        }
    )

    completed = nodes.complete_node(validated)

    assert completed.todos
    assert all(todo.status == TodoStatus.COMPLETED for todo in completed.todos)


def test_apply_edit_plan_rolls_back_create_and_delete_when_update_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    deleted = tmp_path / "deleted.txt"
    failing = tmp_path / "failing.txt"
    deleted.write_text("restore me\n", encoding="utf-8")
    failing.write_text("before\n", encoding="utf-8")
    original_write_text = Path.write_text

    def fail_update(path: Path, data: str, *args, **kwargs) -> int:
        if path == failing:
            raise OSError("simulated mixed transaction failure")
        return original_write_text(path, data, *args, **kwargs)

    monkeypatch.setattr(Path, "write_text", fail_update)

    with pytest.raises(OSError, match="simulated mixed transaction failure"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "create",
                    "path": "created.txt",
                    "content": "temporary\n",
                    "summary": "Create temporary file",
                },
                {
                    "operation": "delete",
                    "path": "deleted.txt",
                    "summary": "Delete existing file",
                },
                {
                    "operation": "update",
                    "path": "failing.txt",
                    "content": "after\n",
                    "summary": "Trigger rollback",
                },
            ),
            allow_deletes=True,
        )

    assert not (tmp_path / "created.txt").exists()
    assert deleted.read_text(encoding="utf-8") == "restore me\n"
    assert failing.read_text(encoding="utf-8") == "before\n"


def test_invalid_edit_model_payload_is_not_written_to_persistent_logs(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    secret = "super-secret-edit-payload"
    gateway = StaticGateway(
        [
            {"todos": [{"id": "edit", "title": "Edit", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": "result.txt",
                        "content": "safe content\n",
                        "summary": "Create result",
                        "secret": secret,
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    root = tmp_path / "edit-secret"
    root.mkdir()

    failed = nodes.edit_node(nodes.plan_node(make_state(root, "Secret edit task")))

    assert failed.phase == AgentPhase.FAILED
    assert all(secret not in log.message for log in failed.logs)
    assert not (root / "result.txt").exists()


def test_apply_edit_plan_removes_partial_create_and_new_directories_on_failure(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    target = tmp_path / "nested" / "result.txt"
    original_write_text = Path.write_text

    def write_partial_then_fail(path: Path, data: str, *args, **kwargs) -> int:
        if path == target:
            original_write_text(path, "partial", *args, **kwargs)
            raise OSError("simulated partial create failure")
        return original_write_text(path, data, *args, **kwargs)

    monkeypatch.setattr(Path, "write_text", write_partial_then_fail)

    with pytest.raises(OSError, match="simulated partial create failure"):
        apply_edit_plan(
            tmp_path,
            plan(
                {
                    "operation": "create",
                    "path": "nested/result.txt",
                    "content": "complete\n",
                    "summary": "Create nested result",
                }
            ),
        )

    assert not target.exists()
    assert not target.parent.exists()


class CapturingStaticGateway(StaticGateway):
    def __init__(self, responses: list[dict[str, object]]) -> None:
        super().__init__(responses)
        self.calls: list[dict[str, object]] = []

    def chat(self, messages, **kwargs):
        self.calls.append({"messages": messages, **kwargs})
        return super().chat(messages, **kwargs)


def test_model_validation_failure_generates_and_applies_structured_repair(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ValidationResult

    target = tmp_path / "feature.py"
    target.write_text("def enabled():\n    return False\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix feature", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "def enabled():\n    return False\n",
                        "summary": "Prepare feature implementation",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "def enabled():\n    return True\n",
                        "summary": "Repair failing feature",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.validate_node(
        nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Repair feature task")))
    )
    failed = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="repair-run-1",
                command=state.validation_command,
                cwd=state.worktree_path,
                stdout="collected 1 failing test",
                stderr="AssertionError: enabled() must return True",
                exitCode=1,
            )
        }
    )

    repairing = nodes.error_analysis_node(failed)
    repaired = nodes.edit_node(repairing)
    validating = nodes.validate_node(repaired)

    assert repairing.phase == AgentPhase.REPAIRING
    assert repairing.repair_round == 1
    assert repairing.edit_plan is not None
    assert repairing.edit_plan_applied is False
    assert repairing.repair_file_edits == []
    assert target.read_text(encoding="utf-8") == "def enabled():\n    return True\n"
    assert repaired.edit_plan_applied is True
    assert [edit.path for edit in repaired.repair_file_edits] == ["feature.py"]
    assert validating.phase == AgentPhase.VALIDATING
    assert validating.validation_request is not None
    assert validating.validation_request.reason == "Run after generated repair round 1."
    assert len(gateway.calls) == 3
    repair_prompt = gateway.calls[2]["messages"][-1].content
    assert "AssertionError: enabled() must return True" in repair_prompt
    assert "collected 1 failing test" in repair_prompt
    assert "return False" in repair_prompt


def test_model_repair_stops_at_max_rounds_without_another_model_call(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ValidationResult

    target = tmp_path / "feature.py"
    target.write_text("return False\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return False\n",
                        "summary": "Initial edit",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Max repair task")))
    failed = state.model_copy(
        update={
            "repair_round": state.max_repair_rounds,
            "validation_result": ValidationResult(
                runId="max-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr="still failing",
                exitCode=1,
            ),
        }
    )

    result = nodes.error_analysis_node(failed)

    assert result.phase == AgentPhase.NEEDS_INTERVENTION
    assert result.repair_round == state.max_repair_rounds
    assert len(gateway.calls) == 2
    assert target.read_text(encoding="utf-8") == "return False\n"


def test_invalid_model_repair_payload_fails_without_leaking_or_mutating(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ValidationResult

    secret = "super-secret-repair-payload"
    target = tmp_path / "feature.py"
    target.write_text("return False\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return False\n",
                        "summary": "Initial edit",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return True\n",
                        "summary": "Invalid repair",
                        "secret": secret,
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Invalid repair task")))
    failed = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="invalid-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr="repair this failure",
                exitCode=1,
            )
        }
    )

    result = nodes.error_analysis_node(failed)

    assert result.phase == AgentPhase.FAILED
    assert all(secret not in log.message for log in result.logs)
    assert target.read_text(encoding="utf-8") == "return False\n"
    assert result.repair_round == 1
    assert result.repair_plan is not None
    assert result.repair_file_edits == []


def test_run_agent_graph_resumes_failed_validation_through_model_repair(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ValidationResult
    from app.graph.workflow import run_agent_graph

    target = tmp_path / "feature.py"
    target.write_text("return False\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return False\n",
                        "summary": "Initial edit",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return True\n",
                        "summary": "Repair feature",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = run_agent_graph(make_state(tmp_path, "Graph repair task"))
    failed = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="graph-repair-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr="AssertionError: expected True",
                exitCode=1,
            )
        }
    )

    resumed = run_agent_graph(failed)

    assert resumed.phase == AgentPhase.VALIDATING
    assert resumed.repair_round == 1
    assert resumed.validation_request is not None
    assert resumed.validation_result is None
    assert resumed.edit_plan_applied is True
    assert target.read_text(encoding="utf-8") == "return True\n"
    assert len(gateway.calls) == 3


def test_approved_delete_cannot_be_replayed_for_a_later_repair_round(tmp_path: Path) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, ApprovalStatus

    target = tmp_path / "obsolete.txt"
    target.write_text("first version\n", encoding="utf-8")
    delete_plan = plan(
        {
            "operation": "delete",
            "path": "obsolete.txt",
            "summary": "Delete obsolete file",
        }
    )
    first_round = make_state(tmp_path, "Delete replay task").model_copy(
        update={
            "phase": AgentPhase.REPAIRING,
            "repair_round": 1,
            "edit_plan": delete_plan,
            "edit_plan_applied": False,
        }
    )
    waiting = nodes.edit_node(first_round)
    approved = waiting.model_copy(
        update={"approval": waiting.approval.model_copy(update={"status": ApprovalStatus.APPROVED})}
    )
    applied = nodes.edit_node(approved)
    assert not target.exists()

    target.write_text("second version\n", encoding="utf-8")
    second_round = applied.model_copy(
        update={
            "phase": AgentPhase.REPAIRING,
            "repair_round": 2,
            "edit_plan": delete_plan,
            "edit_plan_applied": False,
            "requires_approval": False,
        }
    )

    replay = nodes.edit_node(second_round)

    assert replay.phase == AgentPhase.WAITING_APPROVAL
    assert replay.approval is not None
    assert replay.approval.status == ApprovalStatus.PENDING
    assert target.read_text(encoding="utf-8") == "second version\n"


def test_model_repair_prompt_preserves_tail_of_long_validation_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import ValidationResult

    target = tmp_path / "feature.py"
    target.write_text("return False\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return False\n",
                        "summary": "Initial edit",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "feature.py",
                        "content": "return True\n",
                        "summary": "Repair feature",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Long log repair task")))
    long_stderr = "LOG_START\n" + ("noise\n" * 4_000) + "ROOT_CAUSE_AT_TAIL\n"
    failed = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="long-log-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr=long_stderr,
                exitCode=1,
            )
        }
    )

    nodes.error_analysis_node(failed)

    repair_prompt = gateway.calls[2]["messages"][-1].content
    assert "LOG_START" in repair_prompt
    assert "ROOT_CAUSE_AT_TAIL" in repair_prompt


def test_model_repair_prompt_redacts_secrets_from_logs_and_workspace_context(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import ValidationResult

    log_secret = "repair-log-secret-value"
    diff_secret = "repair-diff-secret-value"
    target = tmp_path / "config.txt"
    target.write_text("safe=true\n", encoding="utf-8")
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix config", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "config.txt",
                        "content": f"API_KEY={diff_secret}\n",
                        "summary": "Prepare config",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "config.txt",
                        "content": "API_KEY=[REDACTED]\n",
                        "summary": "Repair config",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Private repair task")))
    failed = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="private-repair-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stdout=f"Authorization: Bearer {log_secret}",
                stderr=f"token={log_secret}",
                exitCode=1,
            )
        }
    )

    nodes.error_analysis_node(failed)

    repair_prompt = gateway.calls[2]["messages"][-1].content
    assert log_secret not in repair_prompt
    assert diff_secret not in repair_prompt
    assert "[REDACTED]" in repair_prompt


def test_delete_approval_cannot_be_reused_across_tasks(tmp_path: Path) -> None:
    from app.graph import nodes
    from app.graph.state import ApprovalStatus

    workspace_a = tmp_path / "task-a"
    workspace_b = tmp_path / "task-b"
    workspace_a.mkdir()
    workspace_b.mkdir()
    (workspace_a / "obsolete.txt").write_text("task a\n", encoding="utf-8")
    target_b = workspace_b / "obsolete.txt"
    target_b.write_text("task b\n", encoding="utf-8")
    delete_plan = plan(
        {
            "operation": "delete",
            "path": "obsolete.txt",
            "summary": "Delete obsolete file",
        }
    )
    waiting_a = nodes.edit_node(
        make_state(workspace_a, "Delete task A").model_copy(update={"edit_plan": delete_plan})
    )
    approved_a = waiting_a.approval.model_copy(update={"status": ApprovalStatus.APPROVED})
    waiting_b = nodes.edit_node(
        make_state(workspace_b, "Delete task B").model_copy(update={"edit_plan": delete_plan})
    )
    replayed = nodes.edit_node(waiting_b.model_copy(update={"approval": approved_a}))

    assert target_b.read_text(encoding="utf-8") == "task b\n"
    assert replayed.approval is not None
    assert replayed.approval.status != ApprovalStatus.APPROVED


def test_model_todos_fail_when_repair_limit_requires_intervention(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, TodoStatus, ValidationResult

    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "fix", "title": "Fix", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": "feature.py",
                        "content": "return False\n",
                        "summary": "Create feature",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Repair limit todo task")))
    failed = state.model_copy(
        update={
            "repair_round": state.max_repair_rounds,
            "validation_result": ValidationResult(
                runId="repair-limit-todo-run",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr="AssertionError: still failing",
                exitCode=1,
            ),
        }
    )

    stopped = nodes.error_analysis_node(failed)

    assert stopped.phase == AgentPhase.NEEDS_INTERVENTION
    assert all(todo.status == TodoStatus.FAILED for todo in stopped.todos)
    assert all(todo.error_message for todo in stopped.todos)


def test_edit_safety_failure_does_not_persist_model_supplied_secret_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase

    secret = "model-path-secret-value"
    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "edit", "title": "Edit", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": f"../{secret}.txt",
                        "content": "unsafe",
                        "summary": "Unsafe model edit",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)

    failed = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Unsafe path task")))

    assert failed.phase == AgentPhase.FAILED
    persisted = "\n".join(
        [
            *(entry.message for entry in failed.logs),
            *(todo.error_message or "" for todo in failed.todos),
        ]
    )
    assert secret not in persisted
    assert secret not in failed.model_dump_json(by_alias=True)


def test_task_context_messages_and_notes_reach_all_model_planning_prompts(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    from app.graph import nodes
    from app.graph.state import ValidationResult

    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "context", "title": "Use context", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": "context.txt",
                        "content": "initial\n",
                        "summary": "Create context file",
                    }
                ]
            },
            {
                "edits": [
                    {
                        "operation": "update",
                        "path": "context.txt",
                        "content": "repaired\n",
                        "summary": "Repair context file",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    state = make_state(tmp_path, "Context-aware task")
    state = state.model_copy(
        update={
            "context": state.context.model_copy(
                update={
                    "user_messages": ["Keep the public API stable."],
                    "notes": ["Memory: prefer a minimal patch."],
                }
            )
        }
    )

    edited = nodes.edit_node(nodes.plan_node(state))
    failed = edited.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="context-repair-run",
                command=edited.validation_command,
                cwd=edited.worktree_path,
                stderr="AssertionError: context repair required",
                exitCode=1,
            )
        }
    )
    nodes.error_analysis_node(failed)

    assert len(gateway.calls) == 3
    for call in gateway.calls:
        prompt = call["messages"][-1].content
        assert "Keep the public API stable." in prompt
        assert "Memory: prefer a minimal patch." in prompt
        assert str(tmp_path) not in prompt
        assert "Repository:" not in prompt
        assert "Workspace:" not in prompt


@pytest.mark.parametrize(
    ("result_updates", "expected_message"),
    [
        ({"cancelled": True}, "cancelled"),
        ({"timedOut": True}, "timed out"),
    ],
)
def test_non_actionable_validation_result_does_not_trigger_model_repair(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    result_updates: dict[str, bool],
    expected_message: str,
) -> None:
    from app.graph import nodes
    from app.graph.state import AgentPhase, TodoStatus, ValidationResult

    gateway = CapturingStaticGateway(
        [
            {"todos": [{"id": "validate", "title": "Validate", "description": ""}]},
            {
                "edits": [
                    {
                        "operation": "create",
                        "path": "result.txt",
                        "content": "initial\n",
                        "summary": "Create result",
                    }
                ]
            },
        ]
    )
    monkeypatch.setattr(nodes, "build_model_gateway", lambda: gateway)
    edited = nodes.edit_node(nodes.plan_node(make_state(tmp_path, "Non-actionable validation")))
    validation_data: dict[str, object] = {
        "runId": "non-actionable-run",
        "command": edited.validation_command,
        "cwd": edited.worktree_path,
        "stderr": "Validation did not produce an actionable code failure.",
        "exitCode": None,
        **result_updates,
    }
    stopped = nodes.error_analysis_node(
        edited.model_copy(update={"validation_result": ValidationResult(**validation_data)})
    )

    assert stopped.phase == AgentPhase.NEEDS_INTERVENTION
    assert stopped.repair_round == 0
    assert len(gateway.calls) == 2
    assert all(todo.status == TodoStatus.FAILED for todo in stopped.todos)
    assert any(expected_message in log.message.lower() for log in stopped.logs)
    assert (tmp_path / "result.txt").read_text(encoding="utf-8") == "initial\n"
