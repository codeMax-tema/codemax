from typing import Any, Literal

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, ConfigDict, Field

from app.model_gateway import ModelGatewayError
from app.providers import (
    ModelConfigError,
    ModelConfigView,
    ProviderSpec,
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
    response_format: dict[str, Any] | None = Field(default=None, alias="responseFormat")


class UsageResponse(AgentModel):
    prompt_tokens: int | None = Field(default=None, alias="promptTokens")
    completion_tokens: int | None = Field(default=None, alias="completionTokens")
    total_tokens: int | None = Field(default=None, alias="totalTokens")


class ChatCompletionResponse(AgentModel):
    id: str
    request_id: str = Field(alias="requestId")
    model: str
    content: str
    finish_reason: str | None = Field(default=None, alias="finishReason")
    latency_ms: float = Field(alias="latencyMs")
    usage: UsageResponse


@router.get("/config", response_model=ModelConfigView)
def get_model_config() -> ModelConfigView:
    try:
        return model_config_view(load_model_config())
    except ModelConfigError as error:
        http_error = model_config_http_error(error)
    raise http_error from None


@router.get("/providers", response_model=list[ProviderSpec])
def list_model_providers() -> tuple[ProviderSpec, ...]:
    return PROVIDER_SPECS


@router.post("/chat", response_model=ChatCompletionResponse)
def create_chat_completion(request: ChatCompletionRequest) -> ChatCompletionResponse:
    del request
    raise HTTPException(
        status_code=403,
        detail={
            "code": "model.taskAuditRequired",
            "message": (
                "Direct model chat is disabled. Use a task-bound Agent request so every "
                "model call is linked to the Privacy Ledger and token budget."
            ),
        },
    )


def model_config_http_error(error: ModelConfigError) -> HTTPException:
    return HTTPException(
        status_code=400,
        detail={"code": error.code, "message": error.message},
    )


def model_gateway_http_error(error: ModelGatewayError) -> HTTPException:
    return HTTPException(
        status_code=error.http_status,
        detail={"code": error.code, "message": error.message},
    )


def model_provider_http_error(error: ModelProviderError) -> HTTPException:
    return HTTPException(
        status_code=error.http_status,
        detail={"code": error.code, "message": error.message},
    )
