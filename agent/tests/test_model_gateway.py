from __future__ import annotations

import traceback
from dataclasses import replace
from types import SimpleNamespace
from typing import Any

import pytest
from app.api import models as models_api
from app.model_gateway import (
    ModelGateway,
    ModelGatewayError,
    ModelGatewayRequest,
    ModelGatewayResult,
)
from app.providers import ModelChatResult, ModelMessage, ModelUsage
from app.providers.config import ModelConfig, ModelProvider
from app.providers.errors import ModelProviderError
from app.providers.openai_compatible import OpenAICompatibleTransport


class RecordingTransport:
    def __init__(self, result: object) -> None:
        self.result = result
        self.calls: list[dict[str, Any]] = []

    def chat(self, **request: Any) -> object:
        self.calls.append(request)
        return self.result


class RaisingTransport:
    def __init__(self, error: Exception) -> None:
        self.error = error

    def chat(self, **request: Any) -> ModelChatResult:
        raise self.error


class RecordingInterceptor:
    def __init__(self) -> None:
        self.before: list[ModelGatewayRequest] = []
        self.after: list[tuple[ModelGatewayRequest, ModelGatewayResult]] = []

    def before_request(self, request: ModelGatewayRequest) -> ModelGatewayRequest:
        self.before.append(request)
        return replace(request, model="arena-selected-model")

    def after_response(
        self,
        request: ModelGatewayRequest,
        result: ModelGatewayResult,
    ) -> None:
        self.after.append((request, result))


def successful_transport_result(*, usage: ModelUsage | None = None) -> ModelChatResult:
    return ModelChatResult(
        id="provider-response-1",
        model="arena-selected-model",
        content='{"answer":"ok"}',
        finish_reason="stop",
        usage=usage
        or ModelUsage(prompt_tokens=11, completion_tokens=7, total_tokens=18),
    )


def test_gateway_forwards_model_messages_and_structured_response_format() -> None:
    transport = RecordingTransport(successful_transport_result())
    interceptor = RecordingInterceptor()
    ticks = iter((10.0, 10.125))
    gateway = ModelGateway(
        transport=transport,
        model="configured-model",
        interceptors=(interceptor,),
        request_id_factory=lambda: "gateway-request-1",
        clock=lambda: next(ticks),
    )
    messages = [
        ModelMessage(role="system", content="Return JSON."),
        ModelMessage(role="user", content="Say ok."),
    ]
    response_format = {
        "type": "json_schema",
        "json_schema": {
            "name": "answer",
            "schema": {
                "type": "object",
                "properties": {"answer": {"type": "string"}},
                "required": ["answer"],
            },
        },
    }

    result = gateway.chat(
        messages=messages,
        temperature=0.2,
        max_tokens=128,
        response_format=response_format,
    )

    assert transport.calls == [
        {
            "model": "arena-selected-model",
            "messages": messages,
            "temperature": 0.2,
            "max_tokens": 128,
            "response_format": response_format,
        }
    ]
    assert result.request_id == "gateway-request-1"
    assert result.model == "arena-selected-model"
    assert result.latency_ms == pytest.approx(125.0)
    assert result.usage == ModelUsage(11, 7, 18)
    assert result.observation.request_id == result.request_id
    assert result.observation.model == result.model
    assert result.observation.latency_ms == result.latency_ms
    assert result.observation.usage == result.usage
    assert interceptor.before[0].response_format == response_format
    assert interceptor.after == [(interceptor.before[0].__class__(
        request_id="gateway-request-1",
        model="arena-selected-model",
        messages=messages,
        temperature=0.2,
        max_tokens=128,
        response_format=response_format,
    ), result)]


@pytest.mark.parametrize(
    ("error", "expected_code", "expected_status"),
    [
        (TimeoutError("slow"), "model.timeout", 504),
        (RuntimeError("provider exploded"), "model.providerError", 502),
    ],
)
def test_gateway_maps_transport_failures_to_stable_errors(
    error: Exception,
    expected_code: str,
    expected_status: int,
) -> None:
    gateway = ModelGateway(transport=RaisingTransport(error), model="test-model")

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == expected_code
    assert raised.value.http_status == expected_status


def test_gateway_rejects_missing_usage_with_stable_error() -> None:
    transport = RecordingTransport(successful_transport_result(usage=None))
    transport.result = ModelChatResult(
        id="provider-response-1",
        model="test-model",
        content="ok",
        finish_reason="stop",
        usage=None,
    )
    gateway = ModelGateway(transport=transport, model="test-model")

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == "model.usageMissing"
    assert raised.value.http_status == 502


@pytest.mark.parametrize(
    "invalid_result",
    [
        None,
        object(),
        ModelChatResult(
            id="",
            model="test-model",
            content="ok",
            finish_reason="stop",
            usage=ModelUsage(1, 1, 2),
        ),
        ModelChatResult(
            id="provider-response-1",
            model="",
            content="ok",
            finish_reason="stop",
            usage=ModelUsage(1, 1, 2),
        ),
    ],
)
def test_gateway_rejects_invalid_transport_responses(invalid_result: object) -> None:
    gateway = ModelGateway(transport=RecordingTransport(invalid_result), model="test-model")

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == "model.invalidResponse"
    assert raised.value.http_status == 502


def test_gateway_and_provider_adapter_never_expose_api_key_in_repr_or_errors() -> None:
    secret = "super-secret-api-key"
    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        baseUrl="https://example.invalid/v1",
        apiKey=secret,
        modelName="test-model",
        timeoutSeconds=1,
    )
    adapter = OpenAICompatibleTransport(config, client=SimpleNamespace())
    gateway = ModelGateway(
        transport=RaisingTransport(
            ModelProviderError(
                code="model.badRequest",
                message=f"provider echoed {secret}",
                http_status=400,
            )
        ),
        model="test-model",
        sensitive_values=(secret,),
    )

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    combined = " ".join((repr(adapter), repr(gateway), repr(raised.value), str(raised.value)))
    assert secret not in combined
    assert "[REDACTED]" in combined
    assert raised.value.code == "model.badRequest"


def test_openai_compatible_transport_forwards_gateway_request_fields() -> None:
    class FakeCompletions:
        def __init__(self) -> None:
            self.calls: list[dict[str, Any]] = []

        def create(self, **request: Any) -> object:
            self.calls.append(request)
            return SimpleNamespace(
                id="provider-response-1",
                model="requested-model",
                choices=[
                    SimpleNamespace(
                        message=SimpleNamespace(content='{"answer":"ok"}'),
                        finish_reason="stop",
                    )
                ],
                usage=SimpleNamespace(
                    prompt_tokens=3,
                    completion_tokens=4,
                    total_tokens=7,
                ),
            )

    completions = FakeCompletions()
    sdk_client = SimpleNamespace(chat=SimpleNamespace(completions=completions))
    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        apiKey="adapter-secret",
        modelName="configured-model",
        timeoutSeconds=1,
    )
    transport = OpenAICompatibleTransport(config, client=sdk_client)
    messages = [ModelMessage(role="user", content="hello")]
    response_format = {"type": "json_object"}

    result = transport.chat(
        model="requested-model",
        messages=messages,
        temperature=0,
        max_tokens=64,
        response_format=response_format,
    )

    assert completions.calls == [
        {
            "model": "requested-model",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 0,
            "max_tokens": 64,
            "response_format": response_format,
        }
    ]
    assert result.model == "requested-model"
    assert result.usage == ModelUsage(3, 4, 7)


def test_models_chat_api_routes_through_model_gateway(monkeypatch: pytest.MonkeyPatch) -> None:
    captured: dict[str, Any] = {}
    config = SimpleNamespace(model_name="configured-model")
    gateway_result = ModelGatewayResult(
        id="provider-response-1",
        request_id="gateway-request-1",
        model="configured-model",
        content='{"answer":"ok"}',
        finish_reason="stop",
        latency_ms=42.5,
        usage=ModelUsage(5, 6, 11),
    )

    class FakeGateway:
        def chat(self, **request: Any) -> ModelGatewayResult:
            captured["request"] = request
            return gateway_result

    def fake_build_model_gateway(received_config: object) -> FakeGateway:
        captured["config"] = received_config
        return FakeGateway()

    monkeypatch.setattr(models_api, "load_model_config", lambda: config)
    monkeypatch.setattr(models_api, "build_model_gateway", fake_build_model_gateway)
    request = models_api.ChatCompletionRequest(
        messages=[models_api.ChatMessage(role="user", content="hello")],
        temperature=0.1,
        maxTokens=32,
        responseFormat={"type": "json_object"},
    )

    response = models_api.create_chat_completion(request)

    assert captured["config"] is config
    assert captured["request"] == {
        "messages": [ModelMessage(role="user", content="hello")],
        "temperature": 0.1,
        "max_tokens": 32,
        "response_format": {"type": "json_object"},
    }
    assert response.id == "provider-response-1"
    assert response.request_id == "gateway-request-1"
    assert response.model == "configured-model"
    assert response.latency_ms == 42.5
    assert response.usage == models_api.UsageResponse(
        promptTokens=5,
        completionTokens=6,
        totalTokens=11,
    )

def formatted_exception(error: BaseException) -> str:
    return "".join(
        traceback.format_exception(type(error), error, error.__traceback__)
    )


def observable_exception_chain(error: BaseException) -> str:
    observed: list[str] = []
    seen: set[int] = set()
    current: BaseException | None = error
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        observed.extend((repr(current), str(current)))
        current = current.__cause__ or current.__context__
    return "\n".join(observed)


def assert_secret_absent_from_exception(error: BaseException, secret: str) -> None:
    assert secret not in formatted_exception(error)
    assert secret not in observable_exception_chain(error)
    assert error.__cause__ is None
    assert error.__context__ is None


def test_openai_transport_drops_sensitive_low_level_exception_context() -> None:
    secret = "transport-trace-secret"

    class SecretFailingCompletions:
        def create(self, **request: Any) -> object:
            raise RuntimeError(f"upstream echoed {secret}")

    sdk_client = SimpleNamespace(
        chat=SimpleNamespace(completions=SecretFailingCompletions())
    )
    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        apiKey=secret,
        modelName="test-model",
        timeoutSeconds=1,
    )
    transport = OpenAICompatibleTransport(config, client=sdk_client)

    with pytest.raises(ModelProviderError) as raised:
        transport.chat(
            model="test-model",
            messages=[ModelMessage(role="user", content="hello")],
        )

    assert raised.value.code == "model.unknownError"
    assert raised.value.http_status == 500
    assert_secret_absent_from_exception(raised.value, secret)


def test_gateway_drops_sensitive_transport_exception_context() -> None:
    secret = "gateway-trace-secret"
    gateway = ModelGateway(
        transport=RaisingTransport(RuntimeError(f"transport echoed {secret}")),
        model="test-model",
        sensitive_values=(secret,),
    )

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == "model.providerError"
    assert raised.value.http_status == 502
    assert_secret_absent_from_exception(raised.value, secret)


def test_api_mapping_drops_sensitive_transport_gateway_exception_chain(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secret = "api-trace-secret"

    class SecretFailingCompletions:
        def create(self, **request: Any) -> object:
            raise RuntimeError(f"provider echoed {secret}")

    sdk_client = SimpleNamespace(
        chat=SimpleNamespace(completions=SecretFailingCompletions())
    )
    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        apiKey=secret,
        modelName="test-model",
        timeoutSeconds=1,
    )
    gateway = ModelGateway(
        transport=OpenAICompatibleTransport(config, client=sdk_client),
        model=config.model_name,
        sensitive_values=(secret,),
    )
    monkeypatch.setattr(models_api, "load_model_config", lambda: config)
    monkeypatch.setattr(models_api, "build_model_gateway", lambda received: gateway)
    request = models_api.ChatCompletionRequest(
        messages=[models_api.ChatMessage(role="user", content="hello")]
    )

    with pytest.raises(Exception) as raised:
        models_api.create_chat_completion(request)

    assert raised.value.status_code == 500
    assert raised.value.detail["code"] == "model.unknownError"
    assert_secret_absent_from_exception(raised.value, secret)

class SecretBeforeInterceptor:
    def __init__(self, secret: str) -> None:
        self.secret = secret

    def before_request(self, request: ModelGatewayRequest) -> ModelGatewayRequest:
        raise RuntimeError(f"before interceptor leaked {self.secret}")

    def after_response(
        self,
        request: ModelGatewayRequest,
        result: ModelGatewayResult,
    ) -> None:
        raise AssertionError("after_response must not run")


class SecretAfterInterceptor:
    def __init__(self, secret: str) -> None:
        self.secret = secret

    def before_request(self, request: ModelGatewayRequest) -> ModelGatewayRequest:
        return request

    def after_response(
        self,
        request: ModelGatewayRequest,
        result: ModelGatewayResult,
    ) -> None:
        raise RuntimeError(f"after interceptor leaked {self.secret}")


@pytest.mark.parametrize(
    "interceptor_factory",
    [SecretBeforeInterceptor, SecretAfterInterceptor],
)
def test_gateway_interceptor_failures_are_stable_and_drop_sensitive_context(
    interceptor_factory: type[SecretBeforeInterceptor] | type[SecretAfterInterceptor],
) -> None:
    secret = "interceptor-trace-secret"
    gateway = ModelGateway(
        transport=RecordingTransport(successful_transport_result()),
        model="test-model",
        interceptors=(interceptor_factory(secret),),
        sensitive_values=(secret,),
    )

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == "model.interceptorError"
    assert raised.value.http_status == 500
    assert_secret_absent_from_exception(raised.value, secret)


@pytest.mark.parametrize(
    "choice",
    [
        SimpleNamespace(finish_reason="stop"),
        SimpleNamespace(message=SimpleNamespace(), finish_reason="stop"),
    ],
)
def test_openai_transport_rejects_choice_without_message_or_content_attribute(
    choice: object,
) -> None:
    class FakeCompletions:
        def create(self, **request: Any) -> object:
            return SimpleNamespace(
                id="provider-response-1",
                model="test-model",
                choices=[choice],
                usage=SimpleNamespace(
                    prompt_tokens=1,
                    completion_tokens=1,
                    total_tokens=2,
                ),
            )

    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        apiKey="test-key",
        modelName="test-model",
        timeoutSeconds=1,
    )
    transport = OpenAICompatibleTransport(
        config,
        client=SimpleNamespace(chat=SimpleNamespace(completions=FakeCompletions())),
    )

    with pytest.raises(ModelProviderError) as raised:
        transport.chat(
            model="test-model",
            messages=[ModelMessage(role="user", content="hello")],
        )

    assert raised.value.code == "model.invalidResponse"
    assert raised.value.http_status == 502


def test_openai_transport_allows_explicit_none_content_as_empty_response() -> None:
    class FakeCompletions:
        def create(self, **request: Any) -> object:
            return SimpleNamespace(
                id="provider-response-1",
                model="test-model",
                choices=[
                    SimpleNamespace(
                        message=SimpleNamespace(content=None),
                        finish_reason="stop",
                    )
                ],
                usage=SimpleNamespace(
                    prompt_tokens=1,
                    completion_tokens=0,
                    total_tokens=1,
                ),
            )

    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        apiKey="test-key",
        modelName="test-model",
        timeoutSeconds=1,
    )
    transport = OpenAICompatibleTransport(
        config,
        client=SimpleNamespace(chat=SimpleNamespace(completions=FakeCompletions())),
    )

    result = transport.chat(
        model="test-model",
        messages=[ModelMessage(role="user", content="hello")],
    )

    assert result.content == ""


def test_gateway_and_transport_repr_hide_prompt_response_and_url_credentials() -> None:
    prompt_secret = "prompt-repr-secret"
    response_secret = "response-repr-secret"
    url_user_secret = "url-user-secret"
    url_password_secret = "url-password-secret"
    url_query_secret = "url-query-secret"
    url_fragment_secret = "url-fragment-secret"
    request = ModelGatewayRequest(
        request_id="request-1",
        model="test-model",
        messages=[ModelMessage(role="user", content=prompt_secret)],
    )
    result = ModelGatewayResult(
        id="response-1",
        request_id="request-1",
        model="test-model",
        content=response_secret,
        finish_reason="stop",
        latency_ms=1.0,
        usage=ModelUsage(1, 1, 2),
    )
    config = ModelConfig(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        baseUrl=(
            f"https://{url_user_secret}:{url_password_secret}@example.invalid/v1"
            f"?token={url_query_secret}#{url_fragment_secret}"
        ),
        apiKey="transport-api-key-secret",
        modelName="test-model",
        timeoutSeconds=1,
    )
    transport = OpenAICompatibleTransport(config, client=SimpleNamespace())

    combined_repr = "\n".join((repr(request), repr(result), repr(transport)))

    for secret in (
        prompt_secret,
        response_secret,
        url_user_secret,
        url_password_secret,
        url_query_secret,
        url_fragment_secret,
        "transport-api-key-secret",
    ):
        assert secret not in combined_repr
    assert "example.invalid" in repr(transport)
