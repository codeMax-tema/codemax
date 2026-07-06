from dataclasses import dataclass
from os import environ


@dataclass(frozen=True, slots=True)
class AgentSettings:
    host: str
    port: int
    log_level: str
    memory_dir: str
    keep_recent_messages: int
    model_provider: str
    model_base_url: str
    model_name: str
    model_api_key: str
    model_timeout_seconds: float


def load_settings() -> AgentSettings:
    return AgentSettings(
        host=environ.get("CODEMAX_AGENT_HOST", "127.0.0.1"),
        port=int(environ.get("CODEMAX_AGENT_PORT", "8765")),
        log_level=environ.get("CODEMAX_AGENT_LOG_LEVEL", "info"),
        memory_dir=environ.get("CODEMAX_AGENT_MEMORY_DIR", ""),
        keep_recent_messages=int(environ.get("CODEMAX_KEEP_RECENT_MESSAGES", "50")),
        model_provider=environ.get("CODEMAX_MODEL_PROVIDER", "openai-compatible"),
        model_base_url=environ.get("CODEMAX_MODEL_BASE_URL", ""),
        model_name=environ.get("CODEMAX_MODEL_NAME", ""),
        model_api_key=environ.get("CODEMAX_MODEL_API_KEY", ""),
        model_timeout_seconds=float(environ.get("CODEMAX_MODEL_TIMEOUT_SECONDS", "60")),
    )
