from dataclasses import dataclass

from openai import OpenAI

from app.providers.config import ModelConfig, validate_model_config
from app.providers.errors import map_openai_error


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


class OpenAICompatibleChatClient:
    def __init__(self, config: ModelConfig) -> None:
        validate_model_config(config)
        self.config = config
        client_kwargs: dict[str, str | float] = {
            "api_key": config.api_key,
            "timeout": config.timeout_seconds,
        }
        if config.base_url:
            client_kwargs["base_url"] = config.base_url

        self.client = OpenAI(**client_kwargs)

    def chat(
        self,
        messages: list[ModelMessage],
        temperature: float | None = None,
        max_tokens: int | None = None,
    ) -> ModelChatResult:
        request: dict[str, object] = {
            "model": self.config.model_name,
            "messages": [
                {"role": message.role, "content": message.content} for message in messages
            ],
        }
        if temperature is not None:
            request["temperature"] = temperature
        if max_tokens is not None:
            request["max_tokens"] = max_tokens

        try:
            response = self.client.chat.completions.create(**request)
        except Exception as error:
            mapped = map_openai_error(error)
            raise mapped from error

        choice = response.choices[0] if response.choices else None
        message = choice.message if choice is not None else None
        content = message.content if message is not None else ""

        return ModelChatResult(
            id=response.id,
            model=response.model,
            content=content or "",
            finish_reason=choice.finish_reason if choice is not None else None,
            usage=model_usage(response.usage),
        )


def model_usage(usage: object | None) -> ModelUsage | None:
    if usage is None:
        return None

    return ModelUsage(
        prompt_tokens=getattr(usage, "prompt_tokens", None),
        completion_tokens=getattr(usage, "completion_tokens", None),
        total_tokens=getattr(usage, "total_tokens", None),
    )


def build_authorization_header(config: ModelConfig) -> dict[str, str]:
    return {"Authorization": f"Bearer {config.api_key}"}
