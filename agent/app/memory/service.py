from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class MemoryWindow:
    recent_message_limit: int = 50


def clamp_recent_message_limit(value: int) -> int:
    if value < 1:
        return 1
    if value > 200:
        return 200
    return value

