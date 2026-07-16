from __future__ import annotations


import traceback
from dataclasses import replace
from types import SimpleNamespace
from typing import Any

import pytest
from app.api import models as models_api
from app.model_audit import model_audit_scope
from app.model_gateway import (
    ModelGateway,
    ModelGatewayError,
    ModelGatewayRequest,
    ModelGatewayResult,
)
from app.providers import ModelChatResult, ModelMessage, ModelUsage
from app.providers.config import ModelConfig, ModelProvider
from app.providers.errors import ModelProviderError
from app.providers.openai_compatible import ModelToolCall, OpenAICompatibleTransport


@pytest.fixture(autouse=True)
def task_bound_model_audit(request: pytest.FixtureRequest):
    if request.node.name == "test_gateway_blocks_calls_without_task_audit_scope":
        yield None
        return
    with model_audit_scope(
        task_id="task-model-gateway-test",
        model_id="configured-model",
        phase="planning",
        budget_limit=120_000,
        budget_per_call=24_000,
        consumed_tokens=0,
    ) as scope:
        yield scope


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


def test_gateway_blocks_calls_without_task_audit_scope() -> None:
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(transport=transport, model="configured-model")

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert raised.value.code == "model.auditContextRequired"
    assert transport.calls == []


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


def test_gateway_forwards_tool_definitions_and_tool_choice() -> None:
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(
        transport=transport,
        model="configured-model",
        request_id_factory=lambda: "gateway-tool-request-1",
    )
    messages = [ModelMessage(role="user", content="Inspect the repository.")]
    tools = [
        {
            "type": "function",
            "function": {
                "name": "search_text",
                "description": "Search repository text.",
                "parameters": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"],
                    "additionalProperties": False,
                },
            },
        }
    ]

    gateway.chat(messages=messages, tools=tools, tool_choice="auto")

    assert transport.calls == [
        {
            "model": "configured-model",
            "messages": messages,
            "temperature": None,
            "max_tokens": None,
            "response_format": None,
            "tools": tools,
            "tool_choice": "auto",
        }
    ]


def test_gateway_returns_normalized_tool_calls() -> None:
    transport = RecordingTransport(
        ModelChatResult(
            id="provider-tool-response-1",
            model="configured-model",
            content="",
            finish_reason="tool_calls",
            usage=ModelUsage(prompt_tokens=5, completion_tokens=3, total_tokens=8),
            tool_calls=(
                ModelToolCall(
                    id="call-read-1",
                    name="read_file",
                    arguments='{"path":"agent/app/model_gateway.py"}',
                ),
            ),
        )
    )
    gateway = ModelGateway(transport=transport, model="configured-model")

    result = gateway.chat(messages=[ModelMessage(role="user", content="Read gateway.")])

    assert result.content == ""
    assert result.tool_calls == (
        ModelToolCall(
            id="call-read-1",
            name="read_file",
            arguments='{"path":"agent/app/model_gateway.py"}',
        ),
    )


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


def test_models_chat_api_is_disabled_without_task_audit_context(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    called = False

    def forbidden_config_load() -> object:
        nonlocal called
        called = True
        raise AssertionError("direct model route must not load provider configuration")

    monkeypatch.setattr(models_api, "load_model_config", forbidden_config_load)
    request = models_api.ChatCompletionRequest(
        messages=[models_api.ChatMessage(role="user", content="hello")]
    )

    with pytest.raises(Exception) as raised:
        models_api.create_chat_completion(request)

    assert raised.value.status_code == 403
    assert raised.value.detail["code"] == "model.taskAuditRequired"
    assert called is False


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


def test_direct_chat_route_does_not_observe_sensitive_payload(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    secret = "api-trace-secret"
    called = False

    def forbidden_config_load() -> object:
        nonlocal called
        called = True
        raise AssertionError(secret)

    monkeypatch.setattr(models_api, "load_model_config", forbidden_config_load)
    request = models_api.ChatCompletionRequest(
        messages=[models_api.ChatMessage(role="user", content=secret)]
    )

    with pytest.raises(Exception) as raised:
        models_api.create_chat_completion(request)

    assert raised.value.status_code == 403
    assert raised.value.detail["code"] == "model.taskAuditRequired"
    assert called is False
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


def test_openai_transport_serializes_tools_and_parses_tool_calls() -> None:
    requests: list[dict[str, Any]] = []

    class FakeCompletions:
        def create(self, **request: Any) -> object:
            requests.append(request)
            return SimpleNamespace(
                id="provider-tool-response-1",
                model="test-model",
                choices=[
                    SimpleNamespace(
                        message=SimpleNamespace(
                            content=None,
                            tool_calls=[
                                SimpleNamespace(
                                    id="call-search-1",
                                    type="function",
                                    function=SimpleNamespace(
                                        name="search_text",
                                        arguments='{"query":"AgentState"}',
                                    ),
                                )
                            ],
                        ),
                        finish_reason="tool_calls",
                    )
                ],
                usage=SimpleNamespace(
                    prompt_tokens=9,
                    completion_tokens=4,
                    total_tokens=13,
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
    tools = [
        {
            "type": "function",
            "function": {
                "name": "search_text",
                "description": "Search repository text.",
                "parameters": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"],
                    "additionalProperties": False,
                },
            },
        }
    ]

    result = transport.chat(
        model="test-model",
        messages=[ModelMessage(role="user", content="Find AgentState.")],
        tools=tools,
        tool_choice="auto",
    )

    assert requests[0]["tools"] == tools
    assert requests[0]["tool_choice"] == "auto"
    assert result.content == ""
    assert len(result.tool_calls) == 1
    assert result.tool_calls[0].id == "call-search-1"
    assert result.tool_calls[0].name == "search_text"
    assert result.tool_calls[0].arguments == '{"query":"AgentState"}'


def test_openai_transport_serializes_assistant_and_tool_history() -> None:
    requests: list[dict[str, Any]] = []

    class FakeCompletions:
        def create(self, **request: Any) -> object:
            requests.append(request)
            return SimpleNamespace(
                id="provider-final-response-1",
                model="test-model",
                choices=[
                    SimpleNamespace(
                        message=SimpleNamespace(content="Done.", tool_calls=None),
                        finish_reason="stop",
                    )
                ],
                usage=SimpleNamespace(
                    prompt_tokens=12,
                    completion_tokens=2,
                    total_tokens=14,
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
    call = ModelToolCall(
        id="call-read-1",
        name="read_file",
        arguments='{"path":"README.md"}',
    )

    transport.chat(
        model="test-model",
        messages=[
            ModelMessage(role="user", content="Read the README."),
            ModelMessage(role="assistant", content="", tool_calls=(call,)),
            ModelMessage(
                role="tool",
                content='{"content":"hello"}',
                tool_call_id="call-read-1",
            ),
        ],
    )

    assert requests[0]["messages"] == [
        {"role": "user", "content": "Read the README."},
        {
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": "call-read-1",
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": '{"path":"README.md"}',
                    },
                }
            ],
        },
        {
            "role": "tool",
            "content": '{"content":"hello"}',
            "tool_call_id": "call-read-1",
        },
    ]


def test_openai_transport_rejects_malformed_tool_arguments_without_leaking_payload() -> None:
    secret = "tool-arguments-secret"

    class FakeCompletions:
        def create(self, **request: Any) -> object:
            return SimpleNamespace(
                id="provider-tool-response-invalid",
                model="test-model",
                choices=[
                    SimpleNamespace(
                        message=SimpleNamespace(
                            content=None,
                            tool_calls=[
                                SimpleNamespace(
                                    id="call-invalid-1",
                                    type="function",
                                    function=SimpleNamespace(
                                        name="read_file",
                                        arguments=f'{{"path":"{secret}"',
                                    ),
                                )
                            ],
                        ),
                        finish_reason="tool_calls",
                    )
                ],
                usage=SimpleNamespace(
                    prompt_tokens=5,
                    completion_tokens=3,
                    total_tokens=8,
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
            messages=[ModelMessage(role="user", content="Read a file.")],
        )

    assert raised.value.code == "model.invalidResponse"
    assert raised.value.http_status == 502
    assert_secret_absent_from_exception(raised.value, secret)


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


def test_successful_request_records_audit_and_redacts_transport_payload(
    task_bound_model_audit: object,
) -> None:
    secret = "ordinary-password-value"
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(
        transport=transport,
        model="configured-model",
        provider="openai-compatible",
        request_id_factory=lambda: "request-linked-1",
    )

    result = gateway.chat(messages=[ModelMessage(role="user", content=f"password={secret}")])

    assert secret not in transport.calls[0]["messages"][0].content
    assert result.request_id == "request-linked-1"
    scope = task_bound_model_audit
    assert len(scope.records) == 1
    audit = scope.records[0]
    assert audit.request_id == "request-linked-1"
    assert audit.task_id == "task-model-gateway-test"
    assert audit.provider == "openai-compatible"
    assert audit.status == "succeeded"
    assert audit.total_tokens == 18
    assert len(audit.request_digest) == 64
    assert any(source.redacted for source in audit.sources)


def test_private_material_and_encoded_tokens_block_before_transport(
    task_bound_model_audit: object,
) -> None:
    import base64

    private_material = "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----"
    encoded = base64.b64encode(private_material.encode()).decode()
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(transport=transport, model="configured-model")

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=[ModelMessage(role="user", content=encoded)])

    assert raised.value.code == "privacy.modelRequestBlocked"
    assert transport.calls == []
    assert task_bound_model_audit.records[-1].status == "blocked"
    assert "base64_encoded_secret" in task_bound_model_audit.records[-1].sources[0].findings


def test_fragmented_percent_unicode_and_certificate_payloads_block_before_transport(
    task_bound_model_audit: object,
) -> None:
    assignment = "password=encoded-sensitive-value"
    percent_encoded = "".join(f"%{byte:02X}" for byte in assignment.encode())
    unicode_encoded = "".join(f"\\u{ord(character):04x}" for character in assignment)
    payloads = [
        "g h p _ a b c d e f g h i j k l",
        percent_encoded,
        unicode_encoded,
        "-----BEGIN CERTIFICATE-----\nfictional-binary-material\n-----END CERTIFICATE-----",
    ]

    for payload in payloads:
        transport = RecordingTransport(successful_transport_result())
        gateway = ModelGateway(transport=transport, model="configured-model")

        with pytest.raises(ModelGatewayError) as raised:
            gateway.chat(messages=[ModelMessage(role="user", content=payload)])

        assert raised.value.code == "privacy.modelRequestBlocked"
        assert transport.calls == []

    assert len(task_bound_model_audit.records) == len(payloads)
    assert all(record.status == "blocked" for record in task_bound_model_audit.records)


def test_nested_tool_schema_secret_is_redacted_before_transport() -> None:
    secret = "nested-schema-secret-value"
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(transport=transport, model="configured-model")

    gateway.chat(
        messages=[ModelMessage(role="user", content="use a tool")],
        tools=[{
            "type": "function",
            "function": {
                "name": "safe_tool",
                "parameters": {"type": "object", "x_api_key": secret},
            },
        }],
    )

    serialized = repr(transport.calls[0])
    assert secret not in serialized
    assert "[REDACTED]" in serialized


def test_sensitive_provider_response_is_sanitized_before_state_boundary() -> None:
    secret = "ghp_responseecho123456789"
    transport = RecordingTransport(
        ModelChatResult(
            id="response-secret",
            model="configured-model",
            content=f"token={secret}",
            finish_reason="stop",
            usage=ModelUsage(2, 2, 4),
        )
    )
    gateway = ModelGateway(transport=transport, model="configured-model")

    result = gateway.chat(messages=[ModelMessage(role="user", content="hello")])

    assert secret not in result.content
    assert "[BLOCKED:" in result.content


def test_retry_call_is_rescanned_and_audited_without_sensitive_plaintext(
    task_bound_model_audit: object,
) -> None:
    sensitive_canary = "retry-sensitive-password-value"

    class SequencedTransport:
        def __init__(self) -> None:
            self.calls: list[dict[str, Any]] = []

        def chat(self, **request: Any) -> ModelChatResult:
            self.calls.append(request)
            if len(self.calls) == 1:
                raise RuntimeError("transient provider failure")
            return successful_transport_result()

    request_ids = iter(("request-retry-1", "request-retry-2"))
    transport = SequencedTransport()
    gateway = ModelGateway(
        transport=transport,
        model="configured-model",
        request_id_factory=lambda: next(request_ids),
    )
    messages = [
        ModelMessage(role="user", content=f"password={sensitive_canary}"),
    ]

    with pytest.raises(ModelGatewayError) as raised:
        gateway.chat(messages=messages)
    result = gateway.chat(messages=messages)

    assert raised.value.code == "model.providerError"
    assert result.request_id == "request-retry-2"
    assert len(transport.calls) == 2
    assert all(sensitive_canary not in repr(call) for call in transport.calls)
    assert [record.request_id for record in task_bound_model_audit.records] == [
        "request-retry-1",
        "request-retry-2",
    ]
    assert [record.status for record in task_bound_model_audit.records] == [
        "failed",
        "succeeded",
    ]


def test_per_call_budget_blocks_transport_and_records_budget_audit() -> None:
    transport = RecordingTransport(successful_transport_result())
    gateway = ModelGateway(transport=transport, model="configured-model")
    with model_audit_scope(
        task_id="task-budget",
        model_id="configured-model",
        phase="planning",
        budget_limit=100,
        budget_per_call=10,
        consumed_tokens=0,
    ) as scope:
        with pytest.raises(ModelGatewayError) as raised:
            gateway.chat(
                messages=[ModelMessage(role="user", content="hello")],
                max_tokens=20,
            )

    assert raised.value.code == "budget.modelRequestBlocked"
    assert transport.calls == []
    assert scope.records[0].status == "blocked"
    assert scope.records[0].budget_per_call == 10
