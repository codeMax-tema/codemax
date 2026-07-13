from __future__ import annotations

import pytest

from app.providers import ModelToolCall
from app.tools.registry import ToolProtocolError, builtin_tool_registry


def test_builtin_registry_exposes_unique_strict_function_schemas() -> None:
    registry = builtin_tool_registry()

    tools = registry.openai_tools()
    names = [tool["function"]["name"] for tool in tools]

    assert names == [
        "list_files",
        "search_text",
        "read_file",
        "apply_file_edits",
        "run_command",
        "git_status",
        "git_diff",
        "update_todos",
        "request_approval",
        "complete_task",
    ]
    assert len(names) == len(set(names))
    for tool in tools:
        assert tool["type"] == "function"
        function = tool["function"]
        assert function["strict"] is True
        parameters = function["parameters"]
        assert parameters["type"] == "object"
        assert parameters["additionalProperties"] is False


def test_registry_normalizes_known_model_tool_call() -> None:
    registry = builtin_tool_registry()

    call = registry.parse_model_tool_call(
        ModelToolCall(
            id="call-search-1",
            name="search_text",
            arguments='{"query":"AgentState","path":"agent/app"}',
        )
    )

    assert call.id == "call-search-1"
    assert call.name == "search_text"
    assert call.arguments == {"query": "AgentState", "path": "agent/app"}
    assert call.definition.execution_domain == "rust_runtime"
    assert call.definition.risk_level == "read_only"


@pytest.mark.parametrize(
    "tool_call",
    [
        ModelToolCall(id="call-unknown", name="shell", arguments='{"command":"dir"}'),
        ModelToolCall(id="call-extra", name="read_file", arguments='{"path":"README.md","extra":true}'),
        ModelToolCall(id="call-missing", name="read_file", arguments="{}"),
    ],
)
def test_registry_rejects_unknown_or_schema_invalid_calls(
    tool_call: ModelToolCall,
) -> None:
    registry = builtin_tool_registry()

    with pytest.raises(ToolProtocolError) as raised:
        registry.parse_model_tool_call(tool_call)

    assert raised.value.code in {"tool.unknown", "tool.invalidArguments"}
    assert "dir" not in str(raised.value)
    assert "README.md" not in str(raised.value)
