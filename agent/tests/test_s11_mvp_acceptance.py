import pytest
pytestmark = pytest.mark.skip(reason="UNSAFE_DIRECT_EDIT_CONTRACT_RETIRED: replaced by Rust two-phase file commit tests")

import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.graph.nodes import (
    approval_interrupt_node,
    complete_node,
    edit_node,
    error_analysis_node,
    plan_node,
    validate_node,
)
from app.graph.state import AgentPhase, ValidationResult
from app.graph.state import create_initial_state


def write_demo_validation_repo(root: Path) -> None:
    source = root / "src"
    source.mkdir(parents=True)
    (source / "feature.py").write_text(
        "def enabled():\n    return False\n",
        encoding="utf-8",
    )
    (root / "validate.py").write_text(
        "\n".join(
            [
                "from pathlib import Path",
                "text = Path('src/feature.py').read_text(encoding='utf-8')",
                "if 'return True' in text:",
                "    print('feature enabled')",
                "    raise SystemExit(0)",
                "print('feature still disabled')",
                "print('CODEMAX_REPAIR {\"path\":\"src/feature.py\",\"find\":\"return False\",\"replace\":\"return True\"}')",
                "raise SystemExit(1)",
                "",
            ]
        ),
        encoding="utf-8",
    )


def run_validation(state, run_id: str) -> ValidationResult:
    completed = subprocess.run(
        state.validation_request.command,
        cwd=state.validation_request.cwd,
        shell=True,
        text=True,
        capture_output=True,
        check=False,
    )
    return ValidationResult(
        runId=run_id,
        command=state.validation_request.command,
        cwd=state.validation_request.cwd,
        stdout=completed.stdout,
        stderr=completed.stderr,
        exitCode=completed.returncode,
    )


def advance_to_validation(state):
    state = plan_node(state)
    state = approval_interrupt_node(state)
    state = edit_node(state)
    return validate_node(state)


def submit_failed_validation(state, result: ValidationResult):
    state = state.model_copy(update={"validation_result": result})
    state = error_analysis_node(state)
    state = edit_node(state)
    return validate_node(state)


def submit_passed_validation(state, result: ValidationResult):
    state = state.model_copy(update={"validation_result": result})
    return complete_node(state)


def test_s11_agent_repairs_demo_repo_until_validation_passes(tmp_path: Path) -> None:
    write_demo_validation_repo(tmp_path)
    command = f'"{sys.executable}" validate.py'
    state = create_initial_state(
        task_id="task-s11-agent",
        repository_path=str(tmp_path),
        worktree_path=str(tmp_path),
        title="Repair demo validation failure",
        description="S11 demo repository has a repairable feature flag bug.",
        validation_command=command,
        max_repair_rounds=2,
    )

    state = advance_to_validation(state)
    assert state.phase == AgentPhase.VALIDATING
    assert state.validation_request is not None
    assert (tmp_path / ".codemax" / "agent-edit-plan.md").is_file()

    first_result = run_validation(state, "run-s11-1")
    assert first_result.exit_code == 1
    assert "CODEMAX_REPAIR" in first_result.stdout

    state = submit_failed_validation(state, first_result)
    assert state.phase == AgentPhase.VALIDATING
    assert state.repair_round == 1
    assert state.validation_request is not None
    assert state.validation_request.reason == "Run after generated repair round 1."
    assert (tmp_path / ".codemax" / "agent-repair-round-1.md").is_file()
    assert "return True" in (tmp_path / "src" / "feature.py").read_text(encoding="utf-8")

    second_result = run_validation(state, "run-s11-2")
    assert second_result.exit_code == 0

    state = submit_passed_validation(state, second_result)
    assert state.phase == AgentPhase.COMPLETED
    assert state.repair_round == 1
    assert state.validation_request is not None
    assert state.validation_request.status == "passed"


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="codemax-s11-agent-") as root:
        test_s11_agent_repairs_demo_repo_until_validation_passes(Path(root))
    print("S11 Agent acceptance passed")
