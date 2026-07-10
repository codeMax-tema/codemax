"""Model provider adapters."""

from app.providers.client import build_chat_client
from app.providers.config import (
    ModelConfig,
    ModelConfigError,
    ModelConfigView,
    ModelProvider,
    ProviderSpec,
    load_model_config,
    model_config_view,
)
from app.providers.errors import ModelProviderError
from app.providers.openai_compatible import (
    ModelChatResult,
    ModelMessage,
    ModelUsage,
    OpenAICompatibleTransport,
)

__all__ = [
    "ModelChatResult",
    "ModelConfig",
    "ModelConfigError",
    "ModelConfigView",
    "ModelMessage",
    "ModelProvider",
    "ModelProviderError",
    "ModelUsage",
    "OpenAICompatibleTransport",
    "ProviderSpec",
    "build_chat_client",
    "load_model_config",
    "model_config_view",
]
