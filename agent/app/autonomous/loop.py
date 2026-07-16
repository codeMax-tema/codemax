from __future__ import annotations

import json
from collections.abc import Iterable
from typing import Protocol

from app.graph.state import (
    AgentCompletion,
    AgentMessage,
    AgentPhase,
    AgentState,
    AgentToolCall,
    AgentToolRequest,
    AgentToolResult,
    ToolRequestStatus,
    ToolResultStatus,
    append_log,
)
from app.model_gateway import ModelGatewayError, ModelGatewayResult, build_model_gateway
from app.providers import ModelMessage, ModelToolCall
from app.tools import ToolProtocolError, builtin_tool_registry
from app.tools.protocol import ToolCall, ToolResult


class AutonomousGateway(Protocol):
    def chat(
        self,
        messages: list[ModelMessage],
        *,
        tools: list[dict[str, object]] | None = None,
        tool_choice: str | dict[str, object] | None = None,
    ) -> ModelGatewayResult: ...


def advance_autonomous_turn(
    state: AgentState,
    *,
    gateway: AutonomousGateway | None = None,
) -> AgentState:
    """Run one V3 model decision without executing any Runtime tool."""
    if state.workflow_version < 3 or state.phase in _paused_or_terminal_phases():
        return state
    if state.pending_tool_request is not None:
        return state.model_copy(update={"phase": AgentPhase.WAITING_RUNTIME})
    if state.agent_round >= state.max_agent_rounds:
        return _needs_intervention(state, "The maximum autonomous model rounds was reached.")
    if state.token_budget is not None and state.consumed_tokens >= state.token_budget:
        return _needs_intervention(state, "The autonomous token budget was reached.")

    current = _ensure_user_message(state)
    selected_gateway = gateway or build_model_gateway()
    registry = builtin_tool_registry()
    try:
        result = selected_gateway.chat(
            _model_messages(current.agent_messages),
            tools=registry.openai_tools(),
            tool_choice="auto",
        )
    except ModelGatewayError as error:
        return _needs_intervention(
            current,
            f"The model gateway could not produce a Runtime tool request ({error.code}).",
        )

    return _record_model_decision(current, result)


def apply_runtime_tool_result(
    state: AgentState,
    result: ToolResult,
    *,
    gateway: AutonomousGateway | None = None,
) -> AgentState:
    """Record an authoritative Runtime result and select the next V3 action."""
    if state.phase is AgentPhase.WAITING_APPROVAL:
        if _is_idempotent_approval_replay(state, result):
            return state
        return _needs_intervention(
            state,
            "Runtime returned a non-idempotent result while approval is pending.",
        )
    if state.phase is not AgentPhase.WAITING_RUNTIME:
        return _needs_intervention(
            state,
            "Runtime tool results are accepted only while waiting for Runtime.",
        )

    pending = state.pending_tool_request
    if pending is None:
        return _needs_intervention(
            state, "Runtime returned a tool result without a pending request."
        )
    if pending.call_id in state.executed_tool_call_ids:
        return _needs_intervention(state, "Runtime replayed an already consumed tool call.")
    if result.call_id != pending.call_id or result.tool_name != pending.tool_name:
        return _needs_intervention(state, "Runtime tool result does not match the pending request.")

    runtime_result = AgentToolResult(
        callId=result.call_id,
        toolName=result.tool_name,
        status=result.status,
        output=result.output,
        errorCode=result.error_code,
        errorMessage=result.error_message,
        artifactRefs=list(result.artifact_refs),
        truncated=result.truncated,
    )
    loop_fingerprint = _loop_fingerprint(
        pending.tool_name,
        pending.arguments,
        runtime_result.status,
    )
    current = state.model_copy(
        update={
            "agent_messages": [
                *state.agent_messages,
                AgentMessage(
                    role="tool",
                    content=_tool_result_content(runtime_result),
                    toolCallId=runtime_result.call_id,
                ),
            ],
            "last_tool_result": runtime_result,
            "executed_tool_call_ids": _append_once(
                state.executed_tool_call_ids,
                runtime_result.call_id,
            ),
            "pending_tool_request": None,
            "phase": AgentPhase.CREATED,
            "loop_fingerprint": loop_fingerprint,
            "consecutive_duplicate_calls": (
                state.consecutive_duplicate_calls
                if state.loop_fingerprint == loop_fingerprint
                else 0
            ),
        }
    )

    if runtime_result.status is ToolResultStatus.WAITING_APPROVAL:
        return current.model_copy(
            update={
                "phase": AgentPhase.WAITING_APPROVAL,
                "requires_approval": True,
                "pending_tool_request": pending.model_copy(
                    update={"status": ToolRequestStatus.WAITING_APPROVAL}
                ),
            }
        )
    if runtime_result.status is ToolResultStatus.CANCELLED:
        return current.model_copy(update={"phase": AgentPhase.CANCELLED})
    if pending.tool_name == "complete_task" and runtime_result.status is ToolResultStatus.SUCCEEDED:
        return current.model_copy(
            update={
                "phase": AgentPhase.COMPLETED,
                "completion": _completion_from_result(runtime_result, pending),
            }
        )

    next_request = _next_serial_request(current)
    if next_request is not None:
        return current.model_copy(
            update={
                "phase": AgentPhase.WAITING_RUNTIME,
                "pending_tool_request": next_request,
            }
        )
    return advance_autonomous_turn(current, gateway=gateway)


def _record_model_decision(
    state: AgentState,
    result: ModelGatewayResult,
) -> AgentState:
    assistant = AgentMessage(
        role="assistant",
        content=result.content,
        toolCalls=[_agent_tool_call(call) for call in result.tool_calls],
    )
    current = state.model_copy(
        update={
            "agent_messages": [*state.agent_messages, assistant],
            "agent_round": state.agent_round + 1,
            "consumed_tokens": state.consumed_tokens + (result.usage.total_tokens or 0),
        }
    )
    if current.token_budget is not None and current.consumed_tokens > current.token_budget:
        return _needs_intervention(current, "The autonomous token budget was exceeded.")
    if not result.tool_calls:
        return _protocol_error(
            current,
            f"model-round-{current.agent_round}",
            "tool.emptyCall",
            "Model response did not include a Runtime tool call.",
        )

    registry = builtin_tool_registry()
    parsed_calls: list[ToolCall] = []
    seen_ids = _model_call_ids(state.agent_messages)
    for raw_call in result.tool_calls:
        error = _validate_model_call(raw_call, seen_ids)
        if error is not None:
            return _protocol_error(
                current, raw_call.id or f"model-round-{current.agent_round}", *error
            )
        try:
            parsed_call = registry.parse_model_tool_call(raw_call)
        except ToolProtocolError as protocol_error:
            return _protocol_error(
                current,
                raw_call.id,
                protocol_error.code,
                protocol_error.message,
            )
        parsed_calls.append(parsed_call)
        seen_ids.add(raw_call.id)

    first_call = parsed_calls[0]
    duplicate_count = _duplicate_count(state, first_call)
    current = current.model_copy(update={"consecutive_duplicate_calls": duplicate_count})
    if duplicate_count >= current.max_duplicate_calls:
        return _needs_intervention(
            current,
            "The model repeated an identical Runtime request without progress.",
        )
    return current.model_copy(
        update={
            "phase": AgentPhase.WAITING_RUNTIME,
            "pending_tool_request": _tool_request(first_call),
        }
    )


def _next_serial_request(state: AgentState) -> AgentToolRequest | None:
    result_ids = {
        message.tool_call_id
        for message in state.agent_messages
        if message.role == "tool" and message.tool_call_id
    }
    definitions = {
        definition.name: definition for definition in builtin_tool_registry().definitions
    }
    for message in reversed(state.agent_messages):
        if message.role != "assistant" or not message.tool_calls:
            continue
        for call in message.tool_calls:
            if call.id in result_ids:
                continue
            definition = definitions.get(call.name)
            if definition is None:
                return None
            return AgentToolRequest(
                callId=call.id,
                toolName=call.name,
                arguments=call.arguments,
                riskLevel=definition.risk_level,
                status=ToolRequestStatus.WAITING_RUNTIME,
            )
        return None
    return None


def _validate_model_call(
    call: ModelToolCall,
    seen_ids: set[str],
) -> tuple[str, str] | None:
    if not isinstance(call.id, str) or not call.id.strip():
        return ("tool.invalidCallId", "Model supplied an empty tool call identifier.")
    if call.id in seen_ids:
        return ("tool.duplicateCallId", "Model reused a tool call identifier.")
    if not isinstance(call.name, str) or not call.name.strip():
        return ("tool.invalidName", "Model supplied an invalid tool name.")
    return None


def _tool_request(call: ToolCall) -> AgentToolRequest:
    return AgentToolRequest(
        callId=call.id,
        toolName=call.name,
        arguments=call.arguments,
        reason=f"Model requested {call.name}.",
        riskLevel=call.definition.risk_level,
        contextSource="model",
        status=ToolRequestStatus.WAITING_RUNTIME,
    )


def _protocol_error(
    state: AgentState,
    call_id: str,
    code: str,
    message: str,
) -> AgentState:
    protocol_result = AgentToolResult(
        callId=call_id,
        toolName="tool_protocol_error",
        status=ToolResultStatus.FAILED,
        errorCode=code,
        errorMessage=message,
    )
    protocol_state = state.model_copy(
        update={
            "last_tool_result": protocol_result,
            "agent_messages": [
                *state.agent_messages,
                AgentMessage(
                    role="tool",
                    content=_tool_result_content(protocol_result),
                    toolCallId=call_id,
                ),
            ],
        }
    )
    return _needs_intervention(protocol_state, "Model returned an invalid Runtime tool call.")


def _ensure_user_message(state: AgentState) -> AgentState:
    if state.agent_messages:
        return state
    content = state.description.strip() or state.title
    return state.model_copy(update={"agent_messages": [AgentMessage(role="user", content=content)]})


def _model_messages(messages: Iterable[AgentMessage]) -> list[ModelMessage]:
    return [
        ModelMessage(
            role=message.role,
            content=message.content,
            tool_call_id=message.tool_call_id,
            tool_calls=tuple(
                ModelToolCall(
                    id=tool_call.id,
                    name=tool_call.name,
                    arguments=json.dumps(
                        tool_call.arguments,
                        ensure_ascii=False,
                        separators=(",", ":"),
                        sort_keys=True,
                    ),
                )
                for tool_call in message.tool_calls
            ),
        )
        for message in messages
    ]


def _agent_tool_call(call: ModelToolCall) -> AgentToolCall:
    try:
        arguments = json.loads(call.arguments)
    except (TypeError, ValueError, json.JSONDecodeError):
        arguments = {}
    return AgentToolCall(
        id=call.id if isinstance(call.id, str) else "",
        name=call.name if isinstance(call.name, str) else "",
        arguments=arguments if isinstance(arguments, dict) else {},
    )


def _tool_result_content(result: AgentToolResult) -> str:
    return json.dumps(
        result.model_dump(mode="json", by_alias=True, exclude_none=True),
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )


def _duplicate_count(state: AgentState, call: ToolCall) -> int:
    status = state.last_tool_result.status if state.last_tool_result is not None else None
    fingerprint = _loop_fingerprint(call.name, call.arguments, status)
    if fingerprint == state.loop_fingerprint:
        return state.consecutive_duplicate_calls + 1
    return 0


def _loop_fingerprint(
    tool_name: str,
    arguments: dict[str, object],
    runtime_result_status: ToolResultStatus | None,
) -> str:
    canonical_arguments = json.dumps(
        arguments,
        ensure_ascii=False,
        separators=(",", ":"),
        sort_keys=True,
    )
    return json.dumps(
        (
            tool_name,
            canonical_arguments,
            runtime_result_status.value if runtime_result_status is not None else None,
        ),
        ensure_ascii=False,
        separators=(",", ":"),
    )


def _model_call_ids(messages: Iterable[AgentMessage]) -> set[str]:
    return {call.id for message in messages for call in message.tool_calls if call.id}


def _append_once(values: list[str], value: str) -> list[str]:
    return values if value in values else [*values, value]


def _is_idempotent_approval_replay(state: AgentState, result: ToolResult) -> bool:
    pending = state.pending_tool_request
    last = state.last_tool_result
    return (
        pending is not None
        and pending.status is ToolRequestStatus.WAITING_APPROVAL
        and last is not None
        and result.call_id == pending.call_id == last.call_id
        and result.tool_name == pending.tool_name == last.tool_name
        and result.status == last.status
        and result.output == last.output
        and result.error_code == last.error_code
        and result.error_message == last.error_message
        and tuple(result.artifact_refs) == tuple(last.artifact_refs)
        and result.truncated == last.truncated
        and result.call_id in state.executed_tool_call_ids
    )


def _completion_from_result(
    result: AgentToolResult,
    pending: AgentToolRequest,
) -> AgentCompletion:
    output = result.output
    summary = output.get("summary", pending.arguments.get("summary", ""))
    validation_summary = output.get("validationSummary", "")
    changed_files = output.get("changedFiles", [])
    remaining_risks = output.get("remainingRisks", [])
    return AgentCompletion(
        summary=summary if isinstance(summary, str) else "",
        validationSummary=validation_summary if isinstance(validation_summary, str) else "",
        changedFiles=[item for item in changed_files if isinstance(item, str)]
        if isinstance(changed_files, list)
        else [],
        remainingRisks=[item for item in remaining_risks if isinstance(item, str)]
        if isinstance(remaining_risks, list)
        else [],
    )


def _needs_intervention(state: AgentState, reason: str) -> AgentState:
    return append_log(
        state.model_copy(update={"phase": AgentPhase.NEEDS_INTERVENTION}),
        reason,
        level="error",
    )


def _paused_or_terminal_phases() -> set[AgentPhase]:
    return {
        AgentPhase.WAITING_RUNTIME,
        AgentPhase.WAITING_APPROVAL,
        AgentPhase.NEEDS_INTERVENTION,
        AgentPhase.COMPLETED,
        AgentPhase.CANCELLED,
        AgentPhase.FAILED,
    }
