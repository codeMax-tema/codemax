from dataclasses import dataclass
from os import environ


@dataclass(frozen=True, slots=True)
class AgentSettings:
    host: str
    port: int
    model_provider: str
    model_base_url: str
    model_name: str


def load_settings() -> AgentSettings:
    return AgentSettings(
        host=environ.get("CODEMAX_AGENT_HOST", "127.0.0.1"),
        port=int(environ.get("CODEMAX_AGENT_PORT", "8765")),
        model_provider=environ.get("CODEMAX_MODEL_PROVIDER", "openai-compatible"),
        model_base_url=environ.get("CODEMAX_MODEL_BASE_URL", ""),
        model_name=environ.get("CODEMAX_MODEL_NAME", ""),
    )

