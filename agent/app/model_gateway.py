from __future__ import annotations

import hashlib
import json
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field, replace
from time import perf_counter
from typing import Protocol
from uuid import uuid4

from app.model_audit import (
    ModelAuditScope,
    ModelAuditSource,
    ModelRequestAudit,
    require_model_audit_scope,
)
from app.privacy import SanitizedPayload, sanitize_exception_message, sanitize_model_payload
from app.providers import (
    ModelChatResult,
    ModelMessage,
    ModelToolCall,
    ModelUsage,
    build_chat_client,
    load_model_config,
)
from app.providers.config import ModelConfig
from app.providers.errors import ModelProviderError


@dataclass(frozen=True, slots=True)
class ModelGatewayRequest:
    request_id: str
    model: str
    messages: list[ModelMessage] = field(repr=False)
    temperature: float | None = None
    max_tokens: int | None = None
    response_format: dict[str, object] | None = None
    tools: list[dict[str, object]] | None = field(default=None, repr=False)
    tool_choice: str | dict[str, object] | None = None


@dataclass(frozen=True, slots=True)
class ModelGatewayObservation:
    request_id: str
    model: str
    latency_ms: float
    usage: ModelUsage


@dataclass(frozen=True, slots=True)
class ModelGatewayResult:
    id: str
    request_id: str
    model: str
    content: str = field(repr=False)
    finish_reason: str | None
    latency_ms: float
    usage: ModelUsage
    tool_calls: tuple[ModelToolCall, ...] = field(default=(), repr=False)

    @property
    def observation(self) -> ModelGatewayObservation:
        return ModelGatewayObservation(
            request_id=self.request_id,
            model=self.model,
            latency_ms=self.latency_ms,
            usage=self.usage,
        )


class ModelGatewayTransport(Protocol):
    def chat(
        self,
        *,
        model: str,
        messages: list[ModelMessage],
        temperature: float | None = None,
        max_tokens: int | None = None,
        response_format: dict[str, object] | None = None,
        tools: list[dict[str, object]] | None = None,
        tool_choice: str | dict[str, object] | None = None,
    ) -> ModelChatResult: ...


class ModelGatewayInterceptor(Protocol):
    def before_request(self, request: ModelGatewayRequest) -> ModelGatewayRequest: ...

    def after_response(self, request: ModelGatewayRequest, result: ModelGatewayResult) -> None: ...


class ModelGatewayError(Exception):
    def __init__(self, code: str, message: str, http_status: int = 502) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.http_status = http_status

    def __repr__(self) -> str:
        return (
            f"ModelGatewayError(code={self.code!r}, message={self.message!r}, "
            f"http_status={self.http_status!r})"
        )

    def __str__(self) -> str:
        return self.message


class ModelGateway:
    def __init__(
        self,
        transport: ModelGatewayTransport,
        model: str,
        *,
        provider: str = "unknown",
        interceptors: Iterable[ModelGatewayInterceptor] = (),
        request_id_factory: Callable[[], str] | None = None,
        clock: Callable[[], float] = perf_counter,
        sensitive_values: Iterable[str] = (),
    ) -> None:
        self._transport = transport
        self._model = model
        self._provider = provider
        self._interceptors = tuple(interceptors)
        self._request_id_factory = request_id_factory or (lambda: str(uuid4()))
        self._clock = clock
        self._sensitive_values = tuple(value for value in sensitive_values if value)

    def __repr__(self) -> str:
        return (
            f"ModelGateway(model={self._model!r}, provider={self._provider!r}, "
            f"interceptors={len(self._interceptors)!r})"
        )

    def chat(
        self,
        messages: list[ModelMessage],
        temperature: float | None = None,
        max_tokens: int | None = None,
        response_format: dict[str, object] | None = None,
        tools: list[dict[str, object]] | None = None,
        tool_choice: str | dict[str, object] | None = None,
    ) -> ModelGatewayResult:
        try:
            scope = require_model_audit_scope()
        except RuntimeError:
            raise ModelGatewayError(
                code="model.auditContextRequired",
                message="Task-bound privacy ledger and budget context is required.",
                http_status=503,
            ) from None

        original_request_id = self._request_id_factory()
        request = ModelGatewayRequest(
            request_id=original_request_id,
            model=self._model,
            messages=messages,
            temperature=temperature,
            max_tokens=max_tokens,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        request = self._apply_before_interceptors(request)
        if request.request_id != original_request_id:
            raise ModelGatewayError(
                code="model.interceptorError",
                message="Model gateway interceptor changed an immutable request identity.",
                http_status=500,
            )

        request, sources = self._sanitize_request(request)
        input_tokens = sum(source.tokens_estimate for source in sources)
        digest = request_digest(request)

        if any(source.blocked for source in sources):
            reason = "Sensitive content was blocked before the model transport boundary."
            self._record_audit(scope, request, digest, sources, "blocked", input_tokens, 0, reason)
            raise ModelGatewayError("privacy.modelRequestBlocked", reason, 422)

        reserved_tokens = input_tokens + max(0, request.max_tokens or 0)
        budget_error = scope.budget_error(reserved_tokens)
        if budget_error is not None:
            self._record_audit(scope, request, digest, sources, "blocked", input_tokens, 0, budget_error)
            raise ModelGatewayError("budget.modelRequestBlocked", budget_error, 429)

        started_at = self._clock()
        raw_result: object | None = None
        transport_error: ModelGatewayError | None = None
        try:
            transport_request: dict[str, object] = {
                "model": request.model,
                "messages": request.messages,
                "temperature": request.temperature,
                "max_tokens": request.max_tokens,
                "response_format": request.response_format,
            }
            if request.tools is not None:
                transport_request["tools"] = request.tools
            if request.tool_choice is not None:
                transport_request["tool_choice"] = request.tool_choice
            raw_result = self._transport.chat(**transport_request)
        except ModelProviderError as error:
            message = sanitize_exception_message(
                redact_sensitive_text(error.message, self._sensitive_values)
            )
            transport_error = ModelGatewayError(error.code, message, error.http_status)
        except TimeoutError:
            transport_error = ModelGatewayError("model.timeout", "Model request timed out.", 504)
        except Exception as error:
            transport_error = ModelGatewayError(
                code="model.providerError",
                message=f"Model provider request failed ({type(error).__name__}).",
                http_status=502,
            )

        if transport_error is not None:
            self._record_audit(
                scope,
                request,
                digest,
                sources,
                "failed",
                input_tokens,
                0,
                "Provider request failed.",
            )
            raise transport_error

        latency_ms = max(0.0, (self._clock() - started_at) * 1000)
        result: ModelGatewayResult | None = None
        response_sources: tuple[ModelAuditSource, ...] = ()
        response_error: ModelGatewayError | None = None
        try:
            result = self._validated_result(raw_result, request, latency_ms)
            result, response_sources = self._sanitize_result(result)
        except ModelGatewayError as error:
            response_error = error
        except Exception as error:
            response_error = ModelGatewayError(
                code="model.invalidResponse",
                message=f"Model provider returned an invalid response ({type(error).__name__}).",
                http_status=502,
            )

        if response_error is not None or result is None:
            self._record_audit(
                scope,
                request,
                digest,
                sources,
                "failed",
                input_tokens,
                0,
                "Provider response was invalid.",
            )
            raise response_error or invalid_response_error()

        output_tokens = result.usage.completion_tokens or 0
        total_tokens = result.usage.total_tokens or input_tokens + output_tokens
        self._record_audit(
            scope,
            request,
            digest,
            (*sources, *response_sources),
            "succeeded",
            input_tokens,
            output_tokens,
            "",
            total_tokens=total_tokens,
        )
        self._apply_after_interceptors(request, result)
        return result

    def _sanitize_request(
        self, request: ModelGatewayRequest
    ) -> tuple[ModelGatewayRequest, tuple[ModelAuditSource, ...]]:
        sources: list[ModelAuditSource] = []
        safe_messages: list[ModelMessage] = []
        for index, message in enumerate(request.messages):
            content = sanitize_model_payload(message.content, f"messages[{index}].content")
            sources.append(audit_source("prompt", f"messages[{index}].content", content))
            safe_tool_calls: list[ModelToolCall] = []
            for call_index, call in enumerate(message.tool_calls):
                arguments = sanitize_model_payload(
                    call.arguments, f"messages[{index}].toolCalls[{call_index}].arguments"
                )
                sources.append(
                    audit_source(
                        "retry_or_tool_context",
                        f"messages[{index}].toolCalls[{call_index}].arguments",
                        arguments,
                    )
                )
                safe_tool_calls.append(replace(call, arguments=str(arguments.value)))
            safe_messages.append(
                replace(
                    message,
                    content=str(content.value),
                    tool_calls=tuple(safe_tool_calls),
                )
            )

        safe_response_format = request.response_format
        if request.response_format is not None:
            sanitized = sanitize_model_payload(request.response_format, "responseFormat")
            sources.append(audit_source("response_format", "responseFormat", sanitized))
            safe_response_format = sanitized.value

        safe_tools = request.tools
        if request.tools is not None:
            sanitized = sanitize_model_payload(request.tools, "tools")
            sources.append(audit_source("tool_definitions", "tools", sanitized))
            safe_tools = sanitized.value

        safe_tool_choice = request.tool_choice
        if request.tool_choice is not None:
            sanitized = sanitize_model_payload(request.tool_choice, "toolChoice")
            sources.append(audit_source("tool_choice", "toolChoice", sanitized))
            safe_tool_choice = sanitized.value

        return (
            replace(
                request,
                messages=safe_messages,
                response_format=safe_response_format,
                tools=safe_tools,
                tool_choice=safe_tool_choice,
            ),
            tuple(sources),
        )

    def _sanitize_result(
        self, result: ModelGatewayResult
    ) -> tuple[ModelGatewayResult, tuple[ModelAuditSource, ...]]:
        sources: list[ModelAuditSource] = []
        content = sanitize_model_payload(result.content, "response.content")
        sources.append(audit_source("model_response", "response.content", content))
        tool_calls: list[ModelToolCall] = []
        for index, call in enumerate(result.tool_calls):
            arguments = sanitize_model_payload(call.arguments, f"response.toolCalls[{index}].arguments")
            sources.append(
                audit_source(
                    "model_tool_call",
                    f"response.toolCalls[{index}].arguments",
                    arguments,
                )
            )
            tool_calls.append(replace(call, arguments=str(arguments.value)))
        return replace(result, content=str(content.value), tool_calls=tuple(tool_calls)), tuple(sources)

    def _record_audit(
        self,
        scope: ModelAuditScope,
        request: ModelGatewayRequest,
        digest: str,
        sources: tuple[ModelAuditSource, ...],
        status: str,
        input_tokens: int,
        output_tokens: int,
        blocked_reason: str,
        *,
        total_tokens: int | None = None,
    ) -> None:
        scope.record(
            ModelRequestAudit(
                request_id=request.request_id,
                task_id=scope.task_id,
                provider=self._provider,
                model_id=request.model,
                phase=scope.phase,
                status=status,
                request_digest=digest,
                input_tokens_estimate=input_tokens,
                output_tokens=output_tokens,
                total_tokens=total_tokens if total_tokens is not None else input_tokens + output_tokens,
                budget_limit=scope.budget_limit,
                budget_per_call=scope.budget_per_call,
                sources=sources,
                blocked_reason=blocked_reason,
            )
        )

    def _apply_before_interceptors(self, request: ModelGatewayRequest) -> ModelGatewayRequest:
        current = request
        for interceptor in self._interceptors:
            failure: ModelGatewayError | None = None
            updated: object | None = None
            try:
                updated = interceptor.before_request(current)
            except Exception:
                failure = interceptor_error()
            if failure is not None:
                raise failure
            if not isinstance(updated, ModelGatewayRequest):
                raise ModelGatewayError(
                    code="model.interceptorError",
                    message="Model gateway interceptor returned an invalid request.",
                    http_status=500,
                )
            current = updated
        return current

    def _apply_after_interceptors(self, request: ModelGatewayRequest, result: ModelGatewayResult) -> None:
        for interceptor in self._interceptors:
            failure: ModelGatewayError | None = None
            try:
                interceptor.after_response(request, result)
            except Exception:
                failure = interceptor_error()
            if failure is not None:
                raise failure

    def _validated_result(
        self, raw_result: object, request: ModelGatewayRequest, latency_ms: float
    ) -> ModelGatewayResult:
        if not isinstance(raw_result, ModelChatResult):
            raise invalid_response_error()
        if not raw_result.id or not raw_result.model:
            raise invalid_response_error()
        if not isinstance(raw_result.content, str):
            raise invalid_response_error()
        if raw_result.finish_reason is not None and not isinstance(raw_result.finish_reason, str):
            raise invalid_response_error()
        if not isinstance(raw_result.tool_calls, tuple) or not all(
            isinstance(tool_call, ModelToolCall) for tool_call in raw_result.tool_calls
        ):
            raise invalid_response_error()
        usage = raw_result.usage
        if usage is None or usage_is_missing(usage):
            raise ModelGatewayError(
                code="model.usageMissing",
                message="Model provider response did not include token usage.",
                http_status=502,
            )
        if not valid_usage(usage):
            raise invalid_response_error()
        return ModelGatewayResult(
            id=raw_result.id,
            request_id=request.request_id,
            model=raw_result.model,
            content=raw_result.content,
            finish_reason=raw_result.finish_reason,
            latency_ms=latency_ms,
            usage=usage,
            tool_calls=raw_result.tool_calls,
        )


def audit_source(data_kind: str, source_ref: str, sanitized: SanitizedPayload) -> ModelAuditSource:
    return ModelAuditSource(
        data_kind=data_kind,
        source_ref=source_ref,
        action=sanitized.action,
        sensitivity_level=sanitized.sensitivity_level,
        findings=sanitized.findings,
        redacted=sanitized.redacted,
        blocked=sanitized.blocked,
        size_bytes=sanitized.original_size_bytes,
        tokens_estimate=sanitized.tokens_estimate,
    )


def request_digest(request: ModelGatewayRequest) -> str:
    payload = {
        "model": request.model,
        "messages": [
            {
                "role": message.role,
                "content": message.content,
                "toolCallId": message.tool_call_id,
                "toolCalls": [
                    {"id": call.id, "name": call.name, "arguments": call.arguments}
                    for call in message.tool_calls
                ],
            }
            for message in request.messages
        ],
        "responseFormat": request.response_format,
        "tools": request.tools,
        "toolChoice": request.tool_choice,
        "temperature": request.temperature,
        "maxTokens": request.max_tokens,
    }
    canonical = json.dumps(payload, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def build_model_gateway(
    config: ModelConfig | None = None,
    *,
    interceptors: Iterable[ModelGatewayInterceptor] = (),
) -> ModelGateway:
    resolved_config = config or load_model_config()
    return ModelGateway(
        transport=build_chat_client(resolved_config),
        model=resolved_config.model_name,
        provider=resolved_config.provider.value,
        interceptors=interceptors,
        sensitive_values=(resolved_config.api_key,),
    )


def usage_is_missing(usage: ModelUsage) -> bool:
    return all(
        value is None
        for value in (usage.prompt_tokens, usage.completion_tokens, usage.total_tokens)
    )


def valid_usage(usage: object) -> bool:
    if not isinstance(usage, ModelUsage):
        return False
    return all(
        value is None or (isinstance(value, int) and not isinstance(value, bool) and value >= 0)
        for value in (usage.prompt_tokens, usage.completion_tokens, usage.total_tokens)
    )


def interceptor_error() -> ModelGatewayError:
    return ModelGatewayError(
        code="model.interceptorError",
        message="Model gateway interceptor failed.",
        http_status=500,
    )


def invalid_response_error() -> ModelGatewayError:
    return ModelGatewayError(
        code="model.invalidResponse",
        message="Model provider returned an invalid response.",
        http_status=502,
    )


def redact_sensitive_text(text: str, sensitive_values: Iterable[str]) -> str:
    redacted = text
    for value in sensitive_values:
        if value:
            redacted = redacted.replace(value, "[REDACTED]")
    return redacted
