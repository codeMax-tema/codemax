from __future__ import annotations

from dataclasses import dataclass
from typing import Any
from urllib.parse import urlsplit

from openai import OpenAI

from app.providers.config import ModelConfig, validate_model_config
from app.providers.errors import ModelProviderError, map_openai_error


@dataclass(frozen=True, slots=True)
class ModelMessage:
    role: str
    content: str


@dataclass(frozen=True, slots=True)
class ModelUsage:
    prompt_tokens: int | None
    completion_tokens: int | None
    total_tokens: int | None


@dataclass(frozen=True, slots=True)
class ModelChatResult:
    id: str
    model: str
    content: str
    finish_reason: str | None
    usage: ModelUsage | None


class OpenAICompatibleTransport:
    def __init__(self, config: ModelConfig, *, client: Any | None = None) -> None:
        validate_model_config(config)
        self._model_name = config.model_name
        self._base_url = safe_base_url_for_repr(config.base_url)
        self._api_key = config.api_key

        if client is not None:
            self._client = client
            return

        client_kwargs: dict[str, str | float] = {
            "api_key": config.api_key,
            "timeout": config.timeout_seconds,
        }
        if config.base_url:
            client_kwargs["base_url"] = config.base_url
        self._client = OpenAI(**client_kwargs)

    def __repr__(self) -> str:
        return (
            f"OpenAICompatibleTransport(model={self._model_name!r}, "
            f"base_url={self._base_url!r}, api_key='[REDACTED]')"
        )

    def chat(
        self,
        messages: list[ModelMessage],
        temperature: float | None = None,
        max_tokens: int | None = None,
        response_format: dict[str, object] | None = None,
        *,
        model: str | None = None,
    ) -> ModelChatResult:
        request: dict[str, object] = {
            "model": model or self._model_name,
            "messages": [
                {"role": message.role, "content": message.content} for message in messages
            ],
        }
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens
        if response_format is not None:
            request["response_format"] = response_format

        translated_error: ModelProviderError | None = None
        try:
            response = self._client.chat.completions.create(**request)
        except Exception as error:
            mapped = map_openai_error(error)
            translated_error = ModelProviderError(
                code=mapped.code,
                message=redact_api_key(mapped.message, self._api_key),
                http_status=mapped.http_status,
            )

        if translated_error is not None:
            raise translated_error from None
        return parse_chat_response(response)


OpenAICompatibleChatClient = OpenAICompatibleTransport


def parse_chat_response(response: object) -> ModelChatResult:
    response_id = getattr(response, "id", None)
    response_model = getattr(response, "model", None)
    choices = getattr(response, "choices", None)
    if not isinstance(response_id, str) or not response_id:
        raise invalid_provider_response()
    if not isinstance(response_model, str) or not response_model:
        raise invalid_provider_response()
    if not choices:
        raise invalid_provider_response()

    choice = choices[0]
    if not hasattr(choice, "message"):
        raise invalid_provider_response()
    message = choice.message
    if message is None or not hasattr(message, "content"):
        raise invalid_provider_response()
    content = message.content
    finish_reason = getattr(choice, "finish_reason", None)
    if content is not None and not isinstance(content, str):
        raise invalid_provider_response()
    if finish_reason is not None and not isinstance(finish_reason, str):
        raise invalid_provider_response()

    return ModelChatResult(
        id=response_id,
        model=response_model,
        content=content or "",
        finish_reason=finish_reason,
        usage=model_usage(getattr(response, "usage", None)),
    )


def model_usage(usage: object | None) -> ModelUsage | None:
    if usage is None:
        return None

    return ModelUsage(
        prompt_tokens=getattr(usage, "prompt_tokens", None),
        completion_tokens=getattr(usage, "completion_tokens", None),
        total_tokens=getattr(usage, "total_tokens", None),
    )


def invalid_provider_response() -> ModelProviderError:
    return ModelProviderError(
        code="model.invalidResponse",
        message="Model provider returned an invalid response.",
        http_status=502,
    )


def safe_base_url_for_repr(base_url: str) -> str:
    if not base_url:
        return ""
    try:
        parsed = urlsplit(base_url)
        hostname = parsed.hostname
        port = parsed.port
    except ValueError:
        return "[configured]"
    if not parsed.scheme or not hostname:
        return "[configured]"

    display_host = f"[{hostname}]" if ":" in hostname else hostname
    display_port = f":{port}" if port is not None else ""
    return f"{parsed.scheme}://{display_host}{display_port}"


def redact_api_key(text: str, api_key: str) -> str:
    if not api_key:
        return text
    return text.replace(api_key, "[REDACTED]")


def build_authorization_header(config: ModelConfig) -> dict[str, str]:
    return {"Authorization": f"Bearer {config.api_key}"}
