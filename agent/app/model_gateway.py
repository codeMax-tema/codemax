from __future__ import annotations

from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from time import perf_counter
from typing import Protocol
from uuid import uuid4

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

    def after_response(
        self,
        request: ModelGatewayRequest,
        result: ModelGatewayResult,
    ) -> None: ...


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
        interceptors: Iterable[ModelGatewayInterceptor] = (),
        request_id_factory: Callable[[], str] | None = None,
        clock: Callable[[], float] = perf_counter,
        sensitive_values: Iterable[str] = (),
    ) -> None:
        self._transport = transport
        self._model = model
        self._interceptors = tuple(interceptors)
        self._request_id_factory = request_id_factory or (lambda: str(uuid4()))
        self._clock = clock
        self._sensitive_values = tuple(value for value in sensitive_values if value)

    def __repr__(self) -> str:
        return (
            f"ModelGateway(model={self._model!r}, "
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
        request = ModelGatewayRequest(
            request_id=self._request_id_factory(),
            model=self._model,
            messages=messages,
            temperature=temperature,
            max_tokens=max_tokens,
            response_format=response_format,
            tools=tools,
            tool_choice=tool_choice,
        )
        request = self._apply_before_interceptors(request)

        started_at = self._clock()
        translated_error: ModelGatewayError | None = None
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
            translated_error = ModelGatewayError(
                code=error.code,
                message=redact_sensitive_text(error.message, self._sensitive_values),
                http_status=error.http_status,
            )
        except TimeoutError:
            translated_error = ModelGatewayError(
                code="model.timeout",
                message="Model request timed out.",
                http_status=504,
            )
        except Exception as error:
            translated_error = ModelGatewayError(
                code="model.providerError",
                message=f"Model provider request failed ({type(error).__name__}).",
                http_status=502,
            )

        if translated_error is not None:
            raise translated_error from None
        latency_ms = max(0.0, (self._clock() - started_at) * 1000)

        result = self._validated_result(raw_result, request, latency_ms)
        self._apply_after_interceptors(request, result)
        return result

    def _apply_before_interceptors(self, request: ModelGatewayRequest) -> ModelGatewayRequest:
        current = request
        for interceptor in self._interceptors:
            translated_error: ModelGatewayError | None = None
            try:
                updated = interceptor.before_request(current)
            except Exception:
                translated_error = interceptor_error()

            if translated_error is not None:
                raise translated_error from None
            if not isinstance(updated, ModelGatewayRequest):
                raise ModelGatewayError(
                    code="model.interceptorError",
                    message="Model gateway interceptor returned an invalid request.",
                    http_status=500,
                )
            current = updated
        return current

    def _apply_after_interceptors(
        self,
        request: ModelGatewayRequest,
        result: ModelGatewayResult,
    ) -> None:
        for interceptor in self._interceptors:
            translated_error: ModelGatewayError | None = None
            try:
                interceptor.after_response(request, result)
            except Exception:
                translated_error = interceptor_error()

            if translated_error is not None:
                raise translated_error from None

    def _validated_result(
        self,
        raw_result: object,
        request: ModelGatewayRequest,
        latency_ms: float,
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


def build_model_gateway(
    config: ModelConfig | None = None,
    *,
    interceptors: Iterable[ModelGatewayInterceptor] = (),
) -> ModelGateway:
    resolved_config = config or load_model_config()
    return ModelGateway(
        transport=build_chat_client(resolved_config),
        model=resolved_config.model_name,
        interceptors=interceptors,
        sensitive_values=(resolved_config.api_key,),
    )


def usage_is_missing(usage: ModelUsage) -> bool:
    return all(
        value is None
        for value in (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
        )
    )


def valid_usage(usage: object) -> bool:
    if not isinstance(usage, ModelUsage):
        return False
    return all(
        value is None or (isinstance(value, int) and not isinstance(value, bool) and value >= 0)
        for value in (
            usage.prompt_tokens,
            usage.completion_tokens,
            usage.total_tokens,
        )
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
