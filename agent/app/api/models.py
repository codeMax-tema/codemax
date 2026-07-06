from typing import Literal

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, Field

from app.providers import (
    ModelConfigError,
    ModelConfigView,
    ModelMessage,
    ProviderSpec,
    build_chat_client,
    load_model_config,
    model_config_view,
)
from app.providers.config import PROVIDER_SPECS
from app.providers.errors import ModelProviderError

router = APIRouter(prefix="/api/v1/models", tags=["models"])


class AgentModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class ChatMessage(AgentModel):
    role: Literal["system", "user", "assistant"]
    content: str = Field(min_length=1)


class ChatCompletionRequest(AgentModel):
    messages: list[ChatMessage] = Field(min_length=1)
    temperature: float | None = Field(default=None, ge=0, le=2)
    max_tokens: int | None = Field(default=None, alias="maxTokens", ge=1)


class UsageResponse(AgentModel):
    prompt_tokens: int | None = Field(default=None, alias="promptTokens")
    completion_tokens: int | None = Field(default=None, alias="completionTokens")
    total_tokens: int | None = Field(default=None, alias="totalTokens")


class ChatCompletionResponse(AgentModel):
    id: str
    model: str
    content: str
    finish_reason: str | None = Field(default=None, alias="finishReason")
    usage: UsageResponse | None = None


@router.get("/config", response_model=ModelConfigView)
def get_model_config() -> ModelConfigView:
    try:
        return model_config_view(load_model_config())
    except ModelConfigError as error:
        raise model_config_http_error(error) from error


@router.get("/providers", response_model=list[ProviderSpec])
def list_model_providers() -> tuple[ProviderSpec, ...]:
    return PROVIDER_SPECS


@router.post("/chat", response_model=ChatCompletionResponse)
def create_chat_completion(request: ChatCompletionRequest) -> ChatCompletionResponse:
    try:
        config = load_model_config()
        client = build_chat_client(config)
        result = client.chat(
            messages=[
                ModelMessage(role=message.role, content=message.content)
                for message in request.messages
            ],
            temperature=request.temperature,
            max_tokens=request.max_tokens,
        )
    except ModelConfigError as error:
        raise model_config_http_error(error) from error
    except ModelProviderError as error:
        raise model_provider_http_error(error) from error

    usage = None
    if result.usage is not None:
        usage = UsageResponse(
            promptTokens=result.usage.prompt_tokens,
            completionTokens=result.usage.completion_tokens,
            totalTokens=result.usage.total_tokens,
        )

    return ChatCompletionResponse(
        id=result.id,
        model=result.model,
        content=result.content,
        finishReason=result.finish_reason,
        usage=usage,
    )


def model_config_http_error(error: ModelConfigError) -> HTTPException:
    return HTTPException(
        status_code=400,
        detail={"code": error.code, "message": error.message},
    )


def model_provider_http_error(error: ModelProviderError) -> HTTPException:
    return HTTPException(
        status_code=error.http_status,
        detail={"code": error.code, "message": error.message},
    )
