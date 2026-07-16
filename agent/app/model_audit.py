from __future__ import annotations

from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass, field
from typing import Iterator


@dataclass(frozen=True, slots=True)
class ModelAuditSource:
    data_kind: str
    source_ref: str
    action: str
    sensitivity_level: str
    findings: tuple[str, ...]
    redacted: bool
    blocked: bool
    size_bytes: int
    tokens_estimate: int


@dataclass(frozen=True, slots=True)
class ModelRequestAudit:
    request_id: str
    task_id: str
    provider: str
    model_id: str
    phase: str
    status: str
    request_digest: str
    input_tokens_estimate: int
    output_tokens: int
    total_tokens: int
    budget_limit: int
    budget_per_call: int
    sources: tuple[ModelAuditSource, ...]
    blocked_reason: str = ""


@dataclass(slots=True)
class ModelAuditScope:
    task_id: str
    model_id: str
    phase: str
    budget_limit: int
    budget_per_call: int
    consumed_tokens: int
    records: list[ModelRequestAudit] = field(default_factory=list)

    @property
    def pending_tokens(self) -> int:
        return sum(record.total_tokens for record in self.records if record.status == "succeeded")

    def budget_error(self, input_tokens: int) -> str | None:
        if self.budget_per_call > 0 and input_tokens > self.budget_per_call:
            return "The model request exceeds the per-call token budget."
        if self.budget_limit > 0 and self.consumed_tokens + self.pending_tokens + input_tokens > self.budget_limit:
            return "The model request exceeds the remaining task token budget."
        return None

    def record(self, audit: ModelRequestAudit) -> None:
        if audit.task_id != self.task_id:
            raise ValueError("Model audit task mismatch.")
        self.records.append(audit)


_ACTIVE_SCOPE: ContextVar[ModelAuditScope | None] = ContextVar("codemax_model_audit_scope", default=None)


@contextmanager
def model_audit_scope(
    *,
    task_id: str,
    model_id: str,
    phase: str,
    budget_limit: int | None,
    budget_per_call: int | None,
    consumed_tokens: int,
) -> Iterator[ModelAuditScope]:
    scope = ModelAuditScope(
        task_id=task_id,
        model_id=model_id,
        phase=phase,
        budget_limit=max(0, budget_limit or 0),
        budget_per_call=max(0, budget_per_call or 0),
        consumed_tokens=max(0, consumed_tokens),
    )
    token = _ACTIVE_SCOPE.set(scope)
    try:
        yield scope
    finally:
        _ACTIVE_SCOPE.reset(token)


def require_model_audit_scope() -> ModelAuditScope:
    scope = _ACTIVE_SCOPE.get()
    if scope is None:
        raise RuntimeError("A task-bound model audit scope is required.")
    return scope
