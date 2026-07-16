from __future__ import annotations

from dataclasses import dataclass, field
from typing import Literal, TypeAlias

JsonValue: TypeAlias = str | int | float | bool | None | list["JsonValue"] | dict[str, "JsonValue"]

ToolRiskLevel = Literal["read_only", "workspace_write", "command", "high"]
ToolExecutionDomain = Literal["rust_runtime"]
ToolResultStatus = Literal[
    "succeeded",
    "failed",
    "rejected",
    "cancelled",
    "waiting_approval",
]


@dataclass(frozen=True, slots=True)
class ToolDefinition:
    name: str
    description: str
    parameters: dict[str, object] = field(repr=False)
    risk_level: ToolRiskLevel = "read_only"
    execution_domain: ToolExecutionDomain = "rust_runtime"

    def openai_schema(self) -> dict[str, object]:
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
                "strict": True,
            },
        }


@dataclass(frozen=True, slots=True)
class ToolCall:
    id: str
    name: str
    arguments: dict[str, object] = field(repr=False)
    definition: ToolDefinition


@dataclass(frozen=True, slots=True)
class ToolRequest:
    call_id: str
    tool_name: str
    arguments: dict[str, object] = field(repr=False)
    reason: str = ""
    risk_level: ToolRiskLevel = "read_only"
    context_source: str = "model"


@dataclass(frozen=True, slots=True)
class ToolResult:
    call_id: str
    tool_name: str
    status: ToolResultStatus
    output: dict[str, JsonValue] = field(default_factory=dict, repr=False)
    error_code: str | None = None
    error_message: str | None = field(default=None, repr=False)
    artifact_refs: tuple[str, ...] = ()
    truncated: bool = False
