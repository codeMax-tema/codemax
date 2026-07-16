# CodeMax V3 自主多轮工具调用主链设计

**日期：** 2026-07-16
**范围：** REL-P0-008、REL-P0-009 的默认编程 Agent 主链；为 REL-P0-002、REL-P0-003、REL-P0-005、REL-P0-006 的 Runtime 统一边界提供接入点。
**不在本规格范围：** 新 UI 视觉设计、安装态完整 E2E 执行、签名与发布工程。后续涉及恢复处置 UI 时必须单独征求用户的 UI 设计意见。

## 1. 目标与验收口径

CodeMax 的新编程任务必须以模型驱动的自主工具循环作为默认执行主链：

```text
模型选择下一项 Runtime 工具
→ Rust Runtime 校验、持久化并执行
→ 脱敏、截断后的真实结果回灌模型
→ 模型基于该结果继续选择读取、编辑、验证、修复、审批或结束
```

V3 不得将 `TodoPlan → EditingPlan → Validation → Repair` 的固定节点图作为新任务能力上限。旧图必须保留给已有 V1/V2 checkpoint 的完整兼容恢复；只有 checkpoint 损坏、必要上下文缺失或继续执行会造成危险副作用时，才允许转入人工处置。

验收必须证明：

1. 新编程任务默认创建 V3 状态，而不是 V1/V2 固定图状态。
2. 模型发出的工具请求均由 Runtime 的严格 Schema、Run Contract、路径隔离、命令策略、审批、隐私、预算和审计边界处理。
3. Python Agent 不直接对用户工作区产生副作用；文件编辑只能由 Rust 的可恢复文件事务提交。
4. 每个工具调用持久化稳定 `callId`、请求摘要、结果摘要、状态、耗时、关联事务和上下文来源。
5. 取消、超时、非法工具、非法参数、预算/轮次超限、重复循环与审批拒绝均进入真实终态或人工介入，不能伪报完成。

## 2. 方案选择

选择“新增 V3 自主工具循环 + 保留 V1/V2 兼容恢复”的渐进方案。

- 不删除既有 LangGraph 固定图；已存在或进行中的 V1/V2 任务必须按其原 workflow version 恢复原有完整流程，而非降级为只读。
- 新建 `programming` 任务默认使用 V3；旧任务根据已持久化 workflow version 继续走兼容逻辑。
- V3 Runtime 工具调用不直接复用 Python 文件编辑器；Python 只编排模型消息和待执行工具请求，Rust 是唯一副作用执行者。

不选择直接删除旧图，原因是其会同时破坏历史任务恢复、审批中断和已持久化任务状态。

## 3. V3 状态模型

在现有 Agent state 基础上明确以下持久化字段与不变量：

- `workflowVersion = 3`：新编程任务默认值。
- `agentMessages`：按顺序保存 user、assistant、tool 消息；只保存脱敏后的模型上下文。
- `agentRound`：每次成功完成“模型决策”后递增；由 Run Contract/任务配置限制最大值。
- `pendingToolRequest`：一次仅允许一个未终结的 Runtime 工具请求，包含 `callId`、工具名、严格参数、请求摘要、风险级别和上下文来源。
- `lastToolResult`：最近一次 Runtime 结果，含状态、脱敏输出、产物引用和截断标志。
- `consumedTokens` 与预算状态：每次模型请求后更新，超限时不再发出新的模型请求。
- `loopFingerprint` / 重复读取计数：用于检测同一工具与相同参数的无进展循环。

状态转移：

```text
running_model
→ waiting_runtime
→ running_model
→ waiting_approval | completed | cancelled | failed | needs_intervention
```

`waiting_runtime`、`waiting_approval`、`needs_intervention` 必须可在进程重启后恢复；危险操作不会静默重放。

## 4. 模型与工具协议

### 4.1 模型调用

V3 通过现有模型网关发送当前可用工具定义和消息历史。模型响应只有两种合法类型：

1. 最终交付：产生结构化 `complete_task` 结果。
2. 工具调用：产生一项或多项严格 Schema 的 Tool Call；Runtime 逐项串行执行并回灌结果。

未知工具、参数无法通过 Schema、重复 `callId`、空调用或协议不完整均返回结构化 `tool_protocol_error`，该结果作为 tool message 回灌模型；超过可恢复阈值后进入 `needs_intervention`。

### 4.2 Runtime 权威工具集合

V3 仅向模型注入当前任务实际可用的工具：

- 只读：目录枚举、文本搜索、读取聚焦文件、Git 状态、Git Diff；
- 可控副作用：结构化文件编辑、命令执行、Todo 更新；
- 高风险：审批请求；
- 终态：完成任务。

Rust Runtime 是工具执行者。`apply_file_edits` 必须调用可恢复文件事务；`run_command` 必须调用现有命令策略与审批消费；Git 工具不得修改用户原始目录；任何工具均不得绕过任务工作区与 Run Contract。

### 4.3 工具结果回灌

Runtime 返回的每个结果必须包括：

- `callId`、`toolName`、状态、开始/结束或耗时；
- 脱敏后的结果摘要；
- 产物引用、截断说明、关联文件事务或命令记录；
- 等待审批、取消、失败或人工介入原因。

Python 只把该结构化、脱敏结果转换为 `tool` message 并追加 checkpoint；不信任模型自行声称的工具执行结果。

## 5. Runtime 执行与持久化边界

### 5.1 Python Agent 职责

- 读取任务状态与 checkpoint；
- 调用模型网关；
- 校验模型 Tool Call 是否符合 Agent 协议；
- 持久化待执行工具请求；
- 接收 Runtime 工具结果并构造下一轮模型上下文；
- 判定循环、预算、取消与明确终态。

Python 不直接写用户文件、不直接执行用户工作区命令、不直接操作 Git。

### 5.2 Rust Runtime 职责

- 根据 task、Run Contract、当前状态和授权限制过滤可用工具；
- 再次验证 callId、参数、路径、审批、预算和幂等键；
- 实际执行只读、事务性编辑、命令、Git 与 Todo 操作；
- 在成功发布事件前持久化工具结果；
- 生成可用于 Privacy Ledger、Proof Pack 和恢复的产物引用。

所有副作用工具都必须可以从任务、callId 和关联事务反查。

## 6. 错误、取消与恢复

- 模型请求超时、供应商协议错误、无效工具参数和 Runtime 拒绝均转换为结构化 tool result，不允许直接跳过。
- 用户取消会阻止后续模型轮次；正在执行的可取消工具转换为 `cancelled`，不可取消的危险操作进入明确人工处置状态。
- 预算、最大轮次、重复调用阈值触发时保存 checkpoint 并进入 `needs_intervention`。
- 重启后：
  - `waiting_runtime` 根据 callId 的 Runtime 持久化状态继续获取结果或转人工处理；
  - `waiting_approval` 保持等待，不重新创建审批；
  - 已消费的 callId 不得再次执行；
  - 文件事务与命令恢复遵循现有安全恢复策略。

## 7. 兼容策略

- 仅新建编程任务默认 `workflowVersion = 3`；不得在恢复时静默升级既有 V1/V2 任务。
- 既有 V1/V2 checkpoint 必须继续执行对应的完整旧图恢复通道：planning、editing、validating、repairing、waiting approval、delivering 与 merging 均按原版本的安全恢复策略处理。
- 仅当 checkpoint 损坏、必要运行时上下文缺失、旧状态无法解析，或恢复会重放删除、覆盖、外部命令、推送、合并等危险动作时，才进入 `needs_intervention` 并保留诊断原因；不得因为版本较旧而直接只读降级。
- V3 不依赖固定 `TodoPlan` 或 `EditingPlan`；必要时这些结构只能作为 `update_todos` 或 `apply_file_edits` 工具参数，不得控制完整任务流程。

## 8. 测试策略与完成定义

### 8.1 Python 测试

- 脚本化模型连续产生：`search_text → read_file → apply_file_edits → run_command → complete_task`；每轮断言模型收到了上一轮 Runtime 真实结果。
- 非法工具、非法参数、重复 callId、循环检测、预算/轮次超限、取消、审批拒绝和供应商错误均有测试。
- 新任务默认 V3；旧 V1/V2 checkpoint 在每个可恢复阶段均可继续原有完整流程；损坏或危险重放场景转为可诊断的人工处置。

### 8.2 Rust 测试

- Runtime 工具 dispatcher 覆盖每个工具的 task/worktree/Run Contract/审批/幂等验证。
- 文件编辑断言走 `execute_transaction`，命令断言走既有策略与授权消费。
- 工具结果、事件、关联事务和重启后的 callId 去重可查询。

### 8.3 契约与端到端测试

- Python 请求 / Rust 结果使用严格共享 Schema。
- 真实本地测试仓库至少覆盖搜索、跨文件读取、一次编辑、验证失败后的下一轮调整和最终交付。
- 后续 P0-004 安装态 E2E 必须保留模型消息摘要、工具轨迹、Diff、验证、隐私账本和 Proof Pack 的可复核产物。

## 9. 实施边界

本阶段只实现 V3 默认主链及其 Runtime 协议、持久化、测试与兼容迁移。不将以下事项伪装为本阶段完成：

- 彻底关闭 P0-001 的所有 Windows handle-pinned 读取缺口；
- 完成 P0-004 干净 Windows 安装态 E2E；
- 设计恢复处置 UI；
- 完成签名、升级、卸载与供应链发布门禁。

这些工作会在 V3 主链具备真实工具轨迹后，以独立规格和验收证据推进。
