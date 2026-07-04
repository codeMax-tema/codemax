from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class OpenAICompatibleConfig:
    base_url: str
    api_key: str
    model: str


def build_authorization_header(config: OpenAICompatibleConfig) -> dict[str, str]:
    return {"Authorization": f"Bearer {config.api_key}"}

