from enum import StrEnum

from pydantic import BaseModel, ConfigDict, Field

from app.core.config import AgentSettings, load_settings


class ModelProvider(StrEnum):
    OPENAI_COMPATIBLE = "openai-compatible"
    CLAUDE = "claude"
    DEEPSEEK = "deepseek"
    DS = "ds"
    BAILIAN = "bailian"
    VOLCENGINE = "volcengine"
    GLM = "glm"
    GEMINI = "gemini"
    OPENAI_GPT = "openai-gpt"
    OPENAI_CLAUDE = "openai-claude"
    ANTHROPIC = "anthropic"
    RELAY = "relay"


class ProviderSpec(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    provider: ModelProvider
    label: str
    status: str
    implemented: bool
    default_base_url: str = Field(alias="defaultBaseUrl")
    default_model_name: str = Field(default="", alias="defaultModelName")
    requires_base_url: bool = Field(default=False, alias="requiresBaseUrl")
    transport: str
    code_location: str = Field(alias="codeLocation")
    env_location: str = Field(alias="envLocation")
    note: str


class ModelConfig(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    provider: ModelProvider
    base_url: str = Field(default="", alias="baseUrl")
    api_key: str = Field(default="", alias="apiKey")
    model_name: str = Field(default="", alias="modelName")
    timeout_seconds: float = Field(default=60.0, alias="timeoutSeconds", gt=0)

    @property
    def api_key_configured(self) -> bool:
        return bool(self.api_key.strip())

    @property
    def configured(self) -> bool:
        spec = provider_spec(self.provider)
        if not spec.implemented:
            return False

        has_required_base_url = bool(self.base_url.strip()) or not spec.requires_base_url

        return self.api_key_configured and bool(self.model_name.strip()) and has_required_base_url


class ModelConfigView(BaseModel):
    model_config = ConfigDict(populate_by_name=True)

    provider: ModelProvider
    base_url: str = Field(alias="baseUrl")
    model_name: str = Field(alias="modelName")
    timeout_seconds: float = Field(alias="timeoutSeconds")
    api_key_configured: bool = Field(alias="apiKeyConfigured")
    configured: bool
    status: str
    implemented: bool
    code_location: str = Field(alias="codeLocation")
    env_location: str = Field(alias="envLocation")
    note: str


class ModelConfigError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


PROVIDER_SPECS: tuple[ProviderSpec, ...] = (
    ProviderSpec(
        provider=ModelProvider.OPENAI_COMPATIBLE,
        label="OpenAI-compatible",
        status="ready",
        implemented=True,
        defaultBaseUrl="",
        requiresBaseUrl=False,
        transport="openai-compatible",
        codeLocation="agent/app/providers/openai_compatible.py::OpenAICompatibleChatClient",
        envLocation="CODEMAX_MODEL_PROVIDER=openai-compatible",
        note="当前可用入口，适配 OpenAI Chat Completions 兼容接口。",
    ),
    ProviderSpec(
        provider=ModelProvider.CLAUDE,
        label="Claude-compatible",
        status="ready",
        implemented=True,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="openai-compatible",
        codeLocation="agent/app/providers/openai_compatible.py::OpenAICompatibleChatClient",
        envLocation="CODEMAX_MODEL_PROVIDER=claude",
        note="当前按 OpenAI-compatible Claude 网关使用，需要配置兼容 Base URL。",
    ),
    ProviderSpec(
        provider=ModelProvider.DEEPSEEK,
        label="DeepSeek",
        status="ready",
        implemented=True,
        defaultBaseUrl="https://api.deepseek.com",
        defaultModelName="deepseek-chat",
        requiresBaseUrl=False,
        transport="openai-compatible",
        codeLocation="agent/app/providers/openai_compatible.py::OpenAICompatibleChatClient",
        envLocation="CODEMAX_MODEL_PROVIDER=deepseek",
        note="当前按 DeepSeek OpenAI-compatible 接口使用。",
    ),
    ProviderSpec(
        provider=ModelProvider.DS,
        label="DS placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[ds]",
        envLocation="CODEMAX_MODEL_PROVIDER=ds",
        note="占位：预留 DS 入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.BAILIAN,
        label="Alibaba Bailian placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[bailian]",
        envLocation="CODEMAX_MODEL_PROVIDER=bailian",
        note="占位：预留百炼入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.VOLCENGINE,
        label="Volcengine Ark placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[volcengine]",
        envLocation="CODEMAX_MODEL_PROVIDER=volcengine",
        note="占位：预留火山/方舟入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.GLM,
        label="GLM placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[glm]",
        envLocation="CODEMAX_MODEL_PROVIDER=glm",
        note="占位：预留 GLM/智谱入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.GEMINI,
        label="Gemini placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[gemini]",
        envLocation="CODEMAX_MODEL_PROVIDER=gemini",
        note="占位：预留 Gemini 原生入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.OPENAI_GPT,
        label="OpenAI GPT placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=False,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[openai-gpt]",
        envLocation="CODEMAX_MODEL_PROVIDER=openai-gpt",
        note="占位：预留 OpenAI GPT 专用入口，当前请使用 openai-compatible。",
    ),
    ProviderSpec(
        provider=ModelProvider.OPENAI_CLAUDE,
        label="OpenAI-compatible Claude placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[openai-claude]",
        envLocation="CODEMAX_MODEL_PROVIDER=openai-claude",
        note="占位：预留 OpenAI-compatible Claude 分类入口，当前请使用 claude。",
    ),
    ProviderSpec(
        provider=ModelProvider.ANTHROPIC,
        label="Anthropic placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=False,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[anthropic]",
        envLocation="CODEMAX_MODEL_PROVIDER=anthropic",
        note="占位：预留 Anthropic 原生 Messages API 入口，当前不发起真实模型调用。",
    ),
    ProviderSpec(
        provider=ModelProvider.RELAY,
        label="Relay gateway placeholder",
        status="placeholder",
        implemented=False,
        defaultBaseUrl="",
        requiresBaseUrl=True,
        transport="placeholder",
        codeLocation="agent/app/providers/config.py::PROVIDER_SPECS[relay]",
        envLocation="CODEMAX_MODEL_PROVIDER=relay",
        note="占位：预留中转站/聚合网关入口，当前不发起真实模型调用。",
    ),
)


def load_model_config(settings: AgentSettings | None = None) -> ModelConfig:
    settings = settings or load_settings()
    provider = parse_provider(settings.model_provider)
    spec = provider_spec(provider)
    base_url = settings.model_base_url.strip() or spec.default_base_url
    model_name = settings.model_name.strip() or spec.default_model_name

    return ModelConfig(
        provider=provider,
        baseUrl=base_url,
        apiKey=settings.model_api_key,
        modelName=model_name,
        timeoutSeconds=settings.model_timeout_seconds,
    )


def model_config_view(config: ModelConfig) -> ModelConfigView:
    spec = provider_spec(config.provider)

    return ModelConfigView(
        provider=config.provider,
        baseUrl=config.base_url,
        modelName=config.model_name,
        timeoutSeconds=config.timeout_seconds,
        apiKeyConfigured=config.api_key_configured,
        configured=config.configured,
        status=spec.status,
        implemented=spec.implemented,
        codeLocation=spec.code_location,
        envLocation=spec.env_location,
        note=spec.note,
    )


def provider_spec(provider: ModelProvider) -> ProviderSpec:
    for spec in PROVIDER_SPECS:
        if spec.provider == provider:
            return spec

    raise ModelConfigError(
        "model.providerUnsupported",
        f"Unsupported model provider: {provider}",
    )


def parse_provider(value: str) -> ModelProvider:
    try:
        provider_value = value.strip() or ModelProvider.OPENAI_COMPATIBLE.value
        return ModelProvider(provider_value)
    except ValueError as error:
        supported = ", ".join(provider.value for provider in ModelProvider)
        raise ModelConfigError(
            "model.providerUnsupported",
            f"Unsupported model provider '{value}'. Supported providers: {supported}.",
        ) from error


def validate_model_config(config: ModelConfig) -> None:
    spec = provider_spec(config.provider)

    if not spec.implemented:
        raise ModelConfigError(
            "model.providerPlaceholder",
            f"{spec.label} is a placeholder at {spec.code_location}. {spec.note}",
        )

    if spec.requires_base_url and not config.base_url.strip():
        raise ModelConfigError(
            "model.baseUrlMissing",
            f"{spec.label} requires CODEMAX_MODEL_BASE_URL for the OpenAI-compatible endpoint.",
        )

    if not config.api_key_configured:
        raise ModelConfigError(
            "model.apiKeyMissing",
            "Model API key is required. Set CODEMAX_MODEL_API_KEY before calling the model.",
        )

    if not config.model_name.strip():
        raise ModelConfigError(
            "model.modelNameMissing",
            "Model name is required. Set CODEMAX_MODEL_NAME before calling the model.",
        )
