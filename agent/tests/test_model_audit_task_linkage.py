from __future__ import annotations

from typing import Any

from app.api.tasks import run_with_model_audit
from app.graph.state import AgentState, create_initial_state
from app.model_gateway import ModelGateway
from app.providers import ModelChatResult, ModelMessage, ModelUsage


class RecordingTransport:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def chat(self, **request: Any) -> ModelChatResult:
        self.calls.append(request)
        return ModelChatResult(
            id="provider-response-task-linkage",
            model="configured-model",
            content='{"answer":"ok"}',
            finish_reason="stop",
            usage=ModelUsage(prompt_tokens=8, completion_tokens=3, total_tokens=11),
        )


def test_task_wrapper_persists_model_request_linkage_without_sensitive_plaintext(caplog) -> None:
    sensitive_canary = "ordinary-password-value-task-linkage"
    state = create_initial_state(
        task_id="task-model-audit-linkage",
        repository_path="C:/repo",
        worktree_path="C:/repo/.worktrees/task-model-audit-linkage",
        title="Verify model request audit linkage",
        description="",
        model_id="configured-model",
        workflow_version=3,
        token_budget=1_000,
        token_budget_per_call=100,
    )
    transport = RecordingTransport()
    gateway = ModelGateway(
        transport=transport,
        model="configured-model",
        provider="openai-compatible",
        request_id_factory=lambda: "request-task-linkage-1",
    )

    def operation(current: AgentState) -> AgentState:
        gateway.chat(
            messages=[
                ModelMessage(
                    role="user",
                    content=f"password={sensitive_canary}",
                )
            ]
        )
        return current

    updated = run_with_model_audit(state, operation)

    assert len(transport.calls) == 1
    assert sensitive_canary not in transport.calls[0]["messages"][0].content
    assert updated.consumed_tokens == 11
    assert len(updated.model_request_audits) == 1
    audit = updated.model_request_audits[0]
    assert audit.request_id == "request-task-linkage-1"
    assert audit.task_id == state.task_id
    assert audit.status == "succeeded"
    assert audit.total_tokens == 11
    assert audit.budget_limit == 1_000
    assert audit.budget_per_call == 100
    assert any(source.redacted for source in audit.sources)
    assert sensitive_canary not in updated.model_dump_json(by_alias=True)
    assert sensitive_canary not in caplog.text
