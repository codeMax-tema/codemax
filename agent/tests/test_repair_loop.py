from pathlib import Path

from app.graph import create_initial_state, run_agent_graph
from app.graph.state import AgentPhase, ValidationResult


def fail_validation(state, run_id: str):
    return state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId=run_id,
                command=state.validation_command,
                cwd=state.worktree_path,
                stdout="",
                stderr="AssertionError: expected true",
                exitCode=1,
            )
        }
    )


def test_validation_failure_enters_repair_loop_until_max_rounds(tmp_path: Path):
    state = create_initial_state(
        task_id="task-s7",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Repair validation failure",
        validation_command="python -m pytest",
        max_repair_rounds=2,
    )

    state = run_agent_graph(state)
    assert state.phase == AgentPhase.VALIDATING
    assert state.validation_request is not None
    assert state.repair_round == 0

    state = run_agent_graph(fail_validation(state, "run-1"))
    assert state.phase == AgentPhase.VALIDATING
    assert state.validation_request is not None
    assert state.validation_request.reason == "Run after generated repair round 1."
    assert state.repair_round == 1
    assert (tmp_path / ".codemax" / "agent-repair-round-1.md").is_file()

    state = run_agent_graph(fail_validation(state, "run-2"))
    assert state.phase == AgentPhase.VALIDATING
    assert state.validation_request is not None
    assert state.repair_round == 2
    assert (tmp_path / ".codemax" / "agent-repair-round-2.md").is_file()

    state = run_agent_graph(fail_validation(state, "run-3"))
    assert state.phase == AgentPhase.NEEDS_INTERVENTION
    assert state.repair_round == 2
    assert state.validation_request is not None


def test_repair_loop_applies_structured_repair_directive_inside_worktree(tmp_path: Path):
    target = tmp_path / "src" / "feature.py"
    target.parent.mkdir()
    target.write_text("def enabled():\n    return False\n", encoding="utf-8")

    state = create_initial_state(
        task_id="task-s7-repair",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Repair structured validation failure",
        validation_command="python -m pytest",
        max_repair_rounds=1,
    )
    state = run_agent_graph(state)
    state = state.model_copy(
        update={
            "validation_result": ValidationResult(
                runId="run-1",
                command=state.validation_command,
                cwd=state.worktree_path,
                stderr=(
                    "AssertionError: expected enabled feature\n"
                    'CODEMAX_REPAIR {"path":"src/feature.py","find":"return False","replace":"return True"}'
                ),
                exitCode=1,
            )
        }
    )

    state = run_agent_graph(state)

    assert state.phase == AgentPhase.VALIDATING
    assert "return True" in target.read_text(encoding="utf-8")
    assert any(edit.operation == "replace" and edit.path == str(target) for edit in state.file_edits)
