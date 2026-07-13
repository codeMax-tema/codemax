from __future__ import annotations

import json
from dataclasses import dataclass
from functools import cache

from app.providers import ModelToolCall
from app.tools.protocol import ToolCall, ToolDefinition


class ToolProtocolError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message

    def __str__(self) -> str:
        return self.message


@dataclass(frozen=True, slots=True)
class ToolRegistry:
    definitions: tuple[ToolDefinition, ...]

    def __post_init__(self) -> None:
        names = [definition.name for definition in self.definitions]
        if len(names) != len(set(names)):
            raise ValueError("Tool names must be unique.")

    def openai_tools(self) -> list[dict[str, object]]:
        return [definition.openai_schema() for definition in self.definitions]

    def parse_model_tool_call(self, tool_call: ModelToolCall) -> ToolCall:
        definition = next(
            (
                candidate
                for candidate in self.definitions
                if candidate.name == tool_call.name
            ),
            None,
        )
        if definition is None:
            raise ToolProtocolError("tool.unknown", "Model requested an unknown tool.")

        try:
            arguments = json.loads(tool_call.arguments)
        except (json.JSONDecodeError, TypeError, ValueError):
            arguments = None
        if not isinstance(arguments, dict) or not arguments_match_schema(
            arguments,
            definition.parameters,
        ):
            raise ToolProtocolError(
                "tool.invalidArguments",
                "Model supplied invalid tool arguments.",
            )
        return ToolCall(
            id=tool_call.id,
            name=tool_call.name,
            arguments=arguments,
            definition=definition,
        )


@cache
def builtin_tool_registry() -> ToolRegistry:
    return ToolRegistry(definitions=builtin_tool_definitions())


def strict_object(
    properties: dict[str, object],
    required: tuple[str, ...] = (),
) -> dict[str, object]:
    return {
        "type": "object",
        "properties": properties,
        "required": list(required),
        "additionalProperties": False,
    }


def builtin_tool_definitions() -> tuple[ToolDefinition, ...]:
    file_edit = strict_object(
        {
            "path": {"type": "string", "minLength": 1},
            "operation": {"type": "string", "enum": ["create", "update", "delete"]},
            "content": {"type": "string"},
            "summary": {"type": "string", "minLength": 1},
            "expectedSha256": {"type": "string"},
        },
        ("path", "operation", "summary"),
    )
    todo_item = strict_object(
        {
            "id": {"type": "string", "minLength": 1},
            "title": {"type": "string", "minLength": 1},
            "description": {"type": "string"},
            "status": {
                "type": "string",
                "enum": ["pending", "in_progress", "completed", "failed", "skipped"],
            },
        },
        ("id", "title", "status"),
    )
    return (
        ToolDefinition(
            name="list_files",
            description="List files and directories within the authorized workspace.",
            parameters=strict_object(
                {
                    "path": {"type": "string"},
                    "depth": {"type": "integer", "minimum": 1, "maximum": 8},
                }
            ),
        ),
        ToolDefinition(
            name="search_text",
            description="Search text within files in the authorized workspace.",
            parameters=strict_object(
                {
                    "query": {"type": "string", "minLength": 1},
                    "path": {"type": "string"},
                    "glob": {"type": "string"},
                    "maxResults": {"type": "integer", "minimum": 1, "maximum": 500},
                },
                ("query",),
            ),
        ),
        ToolDefinition(
            name="read_file",
            description="Read a focused range from a file in the authorized workspace.",
            parameters=strict_object(
                {
                    "path": {"type": "string", "minLength": 1},
                    "startLine": {"type": "integer", "minimum": 1},
                    "lineCount": {"type": "integer", "minimum": 1, "maximum": 2000},
                },
                ("path",),
            ),
        ),
        ToolDefinition(
            name="apply_file_edits",
            description="Apply a structured, transactional set of workspace file edits.",
            parameters=strict_object(
                {"edits": {"type": "array", "minItems": 1, "items": file_edit}},
                ("edits",),
            ),
            risk_level="workspace_write",
        ),
        ToolDefinition(
            name="run_command",
            description="Run an authorized command in the task workspace.",
            parameters=strict_object(
                {
                    "command": {"type": "string", "minLength": 1},
                    "cwd": {"type": "string"},
                    "reason": {"type": "string", "minLength": 1},
                    "purpose": {"type": "string", "enum": ["task", "validation"]},
                },
                ("command", "reason", "purpose"),
            ),
            risk_level="command",
        ),
        ToolDefinition(
            name="git_status",
            description="Read the current Git status without modifying the repository.",
            parameters=strict_object({}),
        ),
        ToolDefinition(
            name="git_diff",
            description="Read a scoped Git diff without modifying the repository.",
            parameters=strict_object(
                {
                    "base": {"type": "string"},
                    "path": {"type": "string"},
                    "statOnly": {"type": "boolean"},
                }
            ),
        ),
        ToolDefinition(
            name="update_todos",
            description="Replace the task todo list with an auditable updated list.",
            parameters=strict_object(
                {"todos": {"type": "array", "items": todo_item}},
                ("todos",),
            ),
            risk_level="workspace_write",
        ),
        ToolDefinition(
            name="request_approval",
            description="Request explicit user approval for a blocked high-risk action.",
            parameters=strict_object(
                {
                    "approvalType": {"type": "string", "minLength": 1},
                    "content": {"type": "string", "minLength": 1},
                    "reason": {"type": "string", "minLength": 1},
                },
                ("approvalType", "content", "reason"),
            ),
            risk_level="high",
        ),
        ToolDefinition(
            name="complete_task",
            description="Finish the task with a truthful delivery summary and evidence.",
            parameters=strict_object(
                {
                    "summary": {"type": "string", "minLength": 1},
                    "validationSummary": {"type": "string"},
                    "changedFiles": {"type": "array", "items": {"type": "string"}},
                    "remainingRisks": {"type": "array", "items": {"type": "string"}},
                },
                ("summary", "changedFiles", "remainingRisks"),
            ),
            risk_level="workspace_write",
        ),
    )


def arguments_match_schema(value: object, schema: dict[str, object]) -> bool:
    expected_type = schema.get("type")
    if expected_type == "object":
        if not isinstance(value, dict):
            return False
        properties = schema.get("properties", {})
        if not isinstance(properties, dict):
            return False
        required = schema.get("required", [])
        if not isinstance(required, list) or any(key not in value for key in required):
            return False
        if schema.get("additionalProperties") is False and any(
            key not in properties for key in value
        ):
            return False
        return all(
            isinstance(properties.get(key), dict)
            and arguments_match_schema(item, properties[key])
            for key, item in value.items()
        )
    if expected_type == "array":
        if not isinstance(value, list):
            return False
        min_items = schema.get("minItems")
        if isinstance(min_items, int) and len(value) < min_items:
            return False
        item_schema = schema.get("items")
        return isinstance(item_schema, dict) and all(
            arguments_match_schema(item, item_schema) for item in value
        )
    if expected_type == "string":
        if not isinstance(value, str):
            return False
        min_length = schema.get("minLength")
        if isinstance(min_length, int) and len(value) < min_length:
            return False
        allowed = schema.get("enum")
        return not isinstance(allowed, list) or value in allowed
    if expected_type == "integer":
        if not isinstance(value, int) or isinstance(value, bool):
            return False
        minimum = schema.get("minimum")
        maximum = schema.get("maximum")
        if isinstance(minimum, int) and value < minimum:
            return False
        return not isinstance(maximum, int) or value <= maximum
    if expected_type == "boolean":
        return isinstance(value, bool)
    return False
