from app.providers.config import ModelConfig, ModelConfigError, ModelProvider, provider_spec
from app.providers.openai_compatible import OpenAICompatibleChatClient


def build_chat_client(config: ModelConfig) -> OpenAICompatibleChatClient:
    spec = provider_spec(config.provider)
    if not spec.implemented:
        raise ModelConfigError(
            "model.providerPlaceholder",
            f"{spec.label} is a placeholder at {spec.code_location}. {spec.note}",
        )

    if config.provider in {
        ModelProvider.OPENAI_COMPATIBLE,
        ModelProvider.CLAUDE,
        ModelProvider.DEEPSEEK,
    }:
        return OpenAICompatibleChatClient(config)

    raise ModelConfigError(
        "model.providerUnsupported",
        f"Unsupported model provider: {config.provider}",
    )
