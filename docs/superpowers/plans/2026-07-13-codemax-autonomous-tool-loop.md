# CodeMax Autonomous Tool Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `test-driven-development` for every behavior change and keep this checklist updated task by task. Do not commit unless the user explicitly requests it.

**Goal:** 将 REL-P0-009 交付为可运行、可恢复、可审计的 Codex 式自主编程主链，让模型根据真实工具结果持续搜索、读取、编辑、运行、修复并完成交付。

**Architecture:** 新任务使用 `workflowVersion=3`。Python Agent 保存模型消息、轮次、预算和单个待执行工具请求；Rust/Tauri 校验并执行真实文件、命令、Git、审批和交付副作用，再将结构化 `toolResult` 回灌 Python。历史版本继续走 TodoPlan/EditingPlan Graph，避免破坏已有 checkpoint。

**Tech Stack:** Python 3.12、Pydantic、FastAPI、LangGraph 兼容层、OpenAI-compatible Chat Completions、Rust/Tauri、rusqlite、pytest、Cargo test。

---

## 文件结构

- Create `agent/app/tools/protocol.py`: 严格工具定义、工具调用、工具结果和消息协议。
- Create `agent/app/tools/registry.py`: Runtime 可用工具 Schema 注册表，不执行真实副作用。
- Create `agent/app/agent_loop.py`: `workflowVersion=3` 单步自主循环与终态判定。
- Create `agent/app/system_prompt.py`: 安装态安全的基础提示词加载和动态上下文拼装。
- Modify `agent/app/providers/openai_compatible.py`: Tool Calling 请求序列化与响应解析。
- Modify `agent/app/model_gateway.py`: tools/tool_choice/tool_calls 全链透传与校验。
- Modify `agent/app/graph/state.py`: 持久消息、待处理请求、工具结果、轮次和预算字段。
- Modify `agent/app/graph/workflow.py`: 版本 3 路由到自主循环，版本 1/2 保持原图。
- Modify `agent/app/api/tasks.py`: 接受通用工具结果并推进下一轮。
- Modify `apps/desktop/src-tauri/src/commands/agent.rs`: 泛化 validation cycle 为通用工具请求执行循环。
- Modify `apps/desktop/src-tauri/src/lib.rs`: 注册新增或重命名后的 Tauri command（保持旧命令兼容）。
- Test `agent/tests/test_model_gateway.py`: Provider/Gateway Tool Calling 契约。
- Test `agent/tests/test_autonomous_agent_loop.py`: 多轮决策、预算、重复调用、恢复和终态。
- Test `apps/desktop/src-tauri/src/commands/agent.rs`: toolRequest 执行、幂等、审批和回灌。
- Test `agent/tests/test_autonomous_agent_e2e.py`: 陌生仓库搜索、读取、编辑、失败修复和最终交付。

### Task 1: 定义 Tool Calling Provider/Gateway 协议

- [ ] 在 `agent/tests/test_model_gateway.py` 新增失败测试，期望 `gateway.chat(..., tools=[...], tool_choice=auto)` 将工具定义传给 Transport，并保留拦截器可见性。
- [ ] 运行 `python -m pytest agent/tests/test_model_gateway.py -k tool -q`，确认因缺少 `tools` 参数失败，而不是夹具或导入错误。
- [ ] 在 `agent/app/providers/openai_compatible.py` 增加不可变协议类型：

```python
@dataclass(frozen=True, slots=True)
class ModelToolCall:
    id: str
    name: str
    arguments: str

@dataclass(frozen=True, slots=True)
class ModelMessage:
    role: str
    content: str = "
    tool_call_id: str | None = None
    tool_calls: tuple[ModelToolCall, ...] = ()
```

- [ ] 扩展 `ModelChatResult`、`ModelGatewayRequest`、`ModelGatewayResult` 和 `ModelGatewayTransport.chat`，携带 `tools`、`tool_choice` 与 `tool_calls`。
- [ ] 运行定向测试确认绿灯，并运行现有 `test_model_gateway.py` 全文件防回归。

### Task 2: 实现 OpenAI-compatible 工具请求和响应解析

- [ ] 新增失败测试：assistant `tool_calls` 中缺失 id、function name 或 JSON arguments 字符串时返回稳定 `model.invalidResponse`。
- [ ] 新增失败测试：`role=tool` 消息序列化时必须包含 `tool_call_id`，assistant 工具消息必须包含 `tool_calls`。
- [ ] 在 `OpenAICompatibleTransport.chat` 中仅在非空时提交 `tools`/`tool_choice`，使用结构化消息序列化函数替代固定 `{role, content}`。
- [ ] 在 `parse_chat_response` 中解析 `message.tool_calls[*].function.name/arguments`，允许工具调用轮次 `content=None`。
- [ ] 运行 `python -m pytest agent/tests/test_model_gateway.py -q`，确认旧 JSON response_format 行为不变。

### Task 3: 建立严格工具协议和注册表

- [ ] 创建 `agent/tests/test_tool_registry.py` 红灯测试，断言每个工具名称唯一、参数 Schema 为 strict object、禁止额外字段，并包含只读与副作用分类。
- [ ] 创建 `agent/app/tools/protocol.py`，定义 `ToolDefinition`、`ToolCall`、`ToolRequest`、`ToolResult`、`ToolOutcome`。
- [ ] 创建 `agent/app/tools/registry.py`，首批注册：`list_files`、`search_text`、`read_file`、`apply_file_edits`、`run_command`、`git_status`、`git_diff`、`update_todos`、`request_approval`、`complete_task`。
- [ ] 工具定义只描述 Schema、风险和执行域；Python 不在这里直接写文件或启动命令。
- [ ] 运行 `python -m pytest agent/tests/test_tool_registry.py -q`。

### Task 4: 扩展持久 AgentState

- [ ] 创建 `agent/tests/test_autonomous_agent_loop.py` 红灯测试，加载旧 checkpoint 时新字段使用安全默认值，版本 3 checkpoint 可往返保存完整调用 ID 和脱敏工具结果。
- [ ] 在 `AgentState` 增加 `messages`、`pendingToolRequest`、`lastToolResult`、`agentRound`、`maxAgentRounds`、`consecutiveDuplicateCalls`、`maxDuplicateCalls`、`tokenBudget`、`consumedTokens`、`completion`。
- [ ] 对工具 stdout/stderr 和文件内容设置持久化大小上限；敏感值经过既有 privacy helper 后才能进入 checkpoint。
- [ ] `create_initial_state` 在存在模型配置时创建 `workflowVersion=3`，但显式恢复的旧版本不自动升级。
- [ ] 运行状态与既有模型编辑测试，确认历史 payload 仍可解析。

### Task 5: 实现单步自主循环

- [ ] 红灯测试模拟三次模型响应：`search_text` -> `read_file` -> `complete_task`，每次只有收到匹配调用 ID 的真实结果后才允许下一轮。
- [ ] 红灯测试覆盖未知工具、非法 JSON 参数、调用 ID 不匹配、重复调用上限、轮次上限、Token 超限、取消和模型直接文本结束。
- [ ] 创建 `agent/app/agent_loop.py`，核心入口为：

```python
def advance_autonomous_agent(state: AgentState) -> AgentState:
    if state.pending_tool_request is not None:
        return state
    if terminal(state):
        return state
    response = gateway.chat(
        messages=build_messages(state),
        tools=registry.schemas(),
        tool_choice=auto,
    )
    return apply_model_decision(state, response)
```

- [ ] 工具调用生成 `pendingToolRequest` 并暂停；`complete_task` 生成显式交付终态；普通文本且无工具调用时进入 `needsIntervention`，不能被当作成功。
- [ ] 每轮累计模型 usage，超预算时保存原因并停止继续请求模型。

### Task 6: 接入产品级系统提示词

- [ ] 新增红灯测试，断言模型首条 system message 包含产品基础提示词和动态 `repository_root/worktree/allowed_tools`，且不包含开发机固定盘符或密钥。
- [ ] 创建 `agent/app/system_prompt.py`，优先从打包资源目录加载 `codemax-programming-assistant.md`，开发态从仓库根解析；缺失时抛稳定配置错误。
- [ ] 将动态 Runtime 上下文作为独立 system message 追加，避免修改基础提示词文件和污染缓存。
- [ ] 更新 Agent 打包/资源清单，确保安装态包含提示词。
- [ ] 运行提示词加载测试和现有隐私测试。

### Task 7: API 与旧 Graph 兼容路由

- [ ] 新增 API 红灯测试：版本 3 advance 返回 `toolRequest`；提交匹配 `toolResult` 后推进下一轮；版本 1/2 仍返回 validationRequest/旧状态。
- [ ] 在 `run_agent_graph` 或上层统一入口按 `workflow_version` 路由：版本 3 调 `advance_autonomous_agent`，旧版本调 `compiled_graph()`。
- [ ] 扩展 `agent/app/api/tasks.py` 的 advance 请求体，接受通用 `toolResult` 并做调用 ID、任务 ID、状态和重复提交校验。
- [ ] 保留旧 validation result 字段和 endpoint，避免桌面旧客户端/历史任务失效。
- [ ] 运行 API、repair loop、model-driven editing 全量 Python 测试。

### Task 8: Rust/Tauri 通用工具执行与安全边界

- [ ] 在 `commands::agent::tests` 先写红灯：Python state 含 `toolRequest` 时，Rust 分发只读工具并把同一 `callId` 的 `toolResult` 回传；重复请求不得重复执行副作用。
- [ ] 将 `run_agent_validation_cycle` 内部循环抽为通用 `run_agent_tool_cycle_inner`，旧 command 继续委托新内核。
- [ ] 为每个工具使用已有路径规范化、Worktree 限制、命令策略、审批、隐私扫描、事件落盘和取消 helper；不得在 dispatcher 复制一套较弱校验。
- [ ] `apply_file_edits` 使用结构化编辑事务；`run_command` 复用 command run 持久化；Git 工具只读；高风险请求先返回等待审批状态。
- [ ] 工具输出设置字节上限并保存 artifact 引用，回灌模型的结果包含截断标记和可审计路径，不把超大日志塞进 state。
- [ ] 运行 Rust 定向测试和全量 `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`。

### Task 9: 陌生仓库 E2E 与发布门禁

- [ ] 创建临时陌生仓库夹具，放入一个需要先搜索定位、首次测试失败、修改后通过的缺陷；模型使用脚本化响应模拟真实工具选择变化。
- [ ] 断言调用顺序由工具结果驱动而非固定：搜索结果决定读取文件，失败输出决定再次编辑和复验。
- [ ] 断言最终状态包含成功验证、Diff 摘要、工具证据、Token 用量和交付说明；取消/审批/崩溃恢复场景可从 checkpoint 继续。
- [ ] 运行：

```powershell
python -m pytest agent/tests -q
python -m ruff check agent/app agent/tests
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run check
npm run build:desktop
```

- [ ] 将真实测试结果和遗留限制追加到 `progress.md`、`findings.md` 与 `docs/距离正式上线缺陷清单.md` 的 REL-P0-009 关闭证据；没有安装态 E2E 证据时不得标记关闭。

## 自检

- 需求覆盖：协议、Provider、Gateway、注册表、状态、循环、提示词、Rust 执行、审批、取消、恢复、E2E 均有对应任务。
- 兼容策略：明确保留 workflowVersion 1/2 和旧 validation 字段。
- 安全边界：所有副作用仍由 Rust 强制校验和持久化。
- 资源约束：工具输出有大小上限，超大内容转 artifact；模型 usage 每轮累计。
- 非目标：本计划不重做 UI，不改变用户已确认的 Mac Codex UI 方案，不执行远程 Git 操作。
