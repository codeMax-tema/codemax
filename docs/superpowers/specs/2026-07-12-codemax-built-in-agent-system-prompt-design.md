# CodeMax 内置编程智能体系统提示词设计

## 1. 目标

将 `codemax-programming-assistant.md` 重写为 CodeMax 产品内置的基础系统提示词。

它应当像 Cursor 的内置 Agent Prompt 一样持续约束智能体行为，同时让 CodeMax 具备 Codex 式自主编程能力：

1. 在授权的任务 Worktree 中真实读取、搜索和修改文件。
2. 自主执行命令、分析结果、修复失败并重新验证。
3. 默认主动完成任务闭环，而不是只给建议或一次性代码片段。
4. 始终让用户知道当前进展、关键决策、验证结果和剩余风险。
5. 与 CodeMax 的隐私、运行契约、记忆、证据和质量门禁能力协同工作。

## 2. 定位与边界

### 2.1 系统提示词负责

1. 定义 CodeMax 的身份、目标、工程判断和交流方式。
2. 指导模型如何选择和使用 Runtime 提供的工具。
3. 规定理解、规划、编辑、验证、修复、审查和交付流程。
4. 规定不伪造结果、不覆盖用户修改、不越权操作等行为原则。
5. 指导模型生成可审计的进度、决策、风险和交付说明。

### 2.2 Runtime 负责

以下能力不能只依赖模型遵守提示词，必须由 Runtime 强制执行：

1. Worktree 和允许路径隔离。
2. 命令、网络和文件权限校验。
3. 高风险操作审批。
4. 敏感信息扫描、脱敏和阻断。
5. 工具参数 Schema 校验。
6. 状态机、事件、日志和 Privacy Ledger 落盘。
7. Token 预算计量与溢出处理。
8. Quality Gate、Proof Pack 和合并权限判定。

### 2.3 与现有节点提示词的关系

`docs/agent/system-prompt.md` 是当前一次性 TodoPlan / EditingPlan 节点的结构化输出提示词，不代表 CodeMax 最终产品级 Agent 的完整行为。

新版 `codemax-programming-assistant.md` 描述最终的 Codex 式工具循环。Runtime 可以逐步升级接入，不要求本次同时改造执行代码。

## 3. Agent 工作模型

CodeMax 使用多轮工具循环：

```text
理解用户目标与规则
→ 检查任务环境和现有改动
→ 建立可验证 Todo
→ 按需读取和搜索代码
→ 修改任务 Worktree
→ 执行相关验证
→ 分析真实失败并自动修复
→ 复验和审查 Diff
→ 生成可审计交付结果
```

模型负责自主选择下一步工具；Runtime 负责执行、返回结果并强制安全边界。

工具不可用或权限不足时，Agent 应寻找允许的替代路径；确实无法继续时再进入人工介入，不能编造工具结果。

## 4. 提示词结构

新版系统提示词包含：

1. Identity and Mission
2. Instruction Priority
3. Operating Contract
4. Core Engineering Principles
5. Autonomous Tool Loop
6. Task Modes
7. Repository and Worktree Safety
8. Editing and Validation Discipline
9. Failure Recovery
10. Privacy, Memory and User Control
11. Auditability and Delivery
12. Multi-Task Coordination
13. Communication
14. Dynamic Runtime Context
15. Runtime Enforcement Boundary

## 5. 关键行为

### 5.1 默认主动执行

除非用户明确只要求分析、解释、评审或方案讨论，否则 Agent 应完成真实修改和验证闭环。

### 5.2 渐进式探索

Agent 不默认读取整个仓库。它应从规则、入口文件、搜索结果、依赖关系和失败信息开始，按任务需要扩展上下文。

### 5.3 真实修改

Agent 使用 Runtime 提供的文件工具修改 Worktree。结构化 JSON 只用于工具参数、事件和数据契约，不限制整轮任务只能输出一次 EditingPlan。

### 5.4 质量门禁分层

1. 任务范围检查必须通过。
2. 不得新增仓库基线错误。
3. 完整仓库和发布级检查由 Run Contract 或发布流程决定。
4. Token 不足不能作为虚报通过或绕过必要验证的理由。

### 5.5 用户控制

涉及需求高影响歧义、UI 视觉决策、高风险操作、权限扩张、合并和质量门禁覆盖时，应请求用户确认。

低风险且可恢复的工程判断由 Agent 自主完成，避免频繁打断用户。

## 6. 规则优先级

出现冲突时按以下顺序处理：

1. Runtime 强制安全和隐私边界。
2. 当前任务 Run Contract。
3. 用户当前明确要求。
4. 仓库规则和项目文档。
5. 当前任务计划与已确认设计。
6. 用户确认的长期记忆和偏好。
7. 历史经验与默认工程惯例。

任何下级规则都不能扩大上级规则授予的权限。

## 7. 动态注入

基础提示词不硬编码机器路径、工具名称、模型或验证命令。Runtime 每个任务动态注入：

```text
product_name
task_id
repository_root
worktree_path
task_branch
target_branch
operating_system
shell
current_date
timezone
output_language
permission_level
allowed_paths
allowed_commands
network_policy
validation_policy
validation_commands
max_repair_rounds
token_budget
remaining_budget
available_tools
active_profile
memory_scope
run_contract
```

## 8. 验收标准

新版提示词应满足：

1. 明确 Agent 能通过工具真实修改文件和执行命令。
2. 不再声明模型只能生成一次性 JSON 计划。
3. 不包含固定 drive letter、固定工具调用次数或无法感知的预算百分比。
4. 不把 Runtime 必须强制执行的安全机制伪装成单纯提示词保证。
5. 与 `最终计划.md` 的用户视角、真实数据、隐私、审计和交付目标一致。
6. 消除 Token 降级与 Quality Gate、合并顺序与 Runtime 职责之间的冲突。
7. 控制基础提示词长度，删除语言框架清单和大量重复案例。
8. 支持中文和英文输出，并服从用户配置语言。

