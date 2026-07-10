# CodeMax A/B/C/D 全线完善实施计划

> 目标：按风险优先的纵向闭环完成 A、B、C、D 四条任务线，使 CodeMax 从部分骨架收口为可安装、可恢复、可审计的真实桌面编程智能体。

**设计依据：** `docs/superpowers/specs/2026-07-10-codemax-abcd-completion-design.md`

**技术栈：** React 18、TypeScript、Zustand、Tauri 2、Rust、rusqlite、Python 3.11、FastAPI、LangGraph、OpenAI-compatible API、Playwright。

**执行原则：**

- 保护当前工作区已有修改，不回退、不覆盖其他人的成果。
- 每项行为改动先补失败测试，再实现最小通过代码。
- 共享契约先于跨语言实现冻结。
- 所有默认数据来自真实任务，禁止 Fixture 或静态占位进入生产路径。
- 用户可见文案必须同时进入中文和英文资源。
- 每个阶段结束都运行窄测试和相关全量回归。

---

## 阶段 0：保护现场与恢复可编译基线

### 任务 0.1：建立当前改动清单与提交边界

**文件：**

- 新增：`docs/release/abcd-worktree-baseline.md`
- 检查：当前所有已修改与未跟踪文件

**步骤：**

1. 记录当前 `HEAD`、工作区文件清单和各文件所属 A/B/C/D 范围。
2. 标记 `database/migrations/0008_agent_telemetry.sql` 与引用它的 Rust 修改为同一交付单元。
3. 记录当前可通过项：架构契约、前端契约、前端构建、发布契约。
4. 记录当前失败项：Rust `InvalidPurpose` 非穷尽匹配、Python 缺少 dev 依赖。
5. 后续每个提交只包含一个纵向任务所需文件。

**验收：** 工作区基线文档可用于判断后续修改是否误带或遗漏现有成果。

### 任务 0.2：修复 Rust 编译阻断并纳入 0008 迁移

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/exec.rs`
- 检查并纳入：`apps/desktop/src-tauri/src/exec/mod.rs`
- 检查并纳入：`apps/desktop/src-tauri/src/storage/mod.rs`
- 检查并纳入：`database/migrations/0008_agent_telemetry.sql`

**测试先行：**

1. 在 `commands/exec.rs` 的错误映射测试中增加 `InvalidPurpose` 场景。
2. 运行：

   ```powershell
   cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml invalid_command_purpose
   ```

3. 确认测试或编译因缺少错误映射失败。

**实现：**

1. 为所有 `CommandExecutionError` 映射补齐 `InvalidPurpose` 分支。
2. 使用稳定错误码，不把原始命令或敏感输入泄露到 UI。
3. 确认 0008 迁移可从空库执行，也可从 0007 增量升级。
4. 增加迁移版本与关键列断言。

**验证：**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml storage::tests -- --nocapture
npm run check:tauri
```

---

## 阶段 1：冻结共享契约

### 任务 1.1：建立严格 IPC Schema 契约测试

**文件：**

- 修改：`contracts/ipc.schema.json`
- 新增：`tests/contracts/verify-ipc-contract.mjs`
- 修改：`package.json`
- 修改：`tests/architecture/verify-architecture.mjs`

**测试先行：**

1. 编写契约测试，比较：
   - Rust `generate_handler!` 注册命令。
   - TypeScript `invoke()` 命令名。
   - JSON Schema command 枚举。
2. 断言 `RepositorySummary`、`CreateTaskRequest`、`TaskDetail`、Run Contract、Privacy、Delivery、Memory 和 D 线对象禁止无约束空壳。
3. 运行 `node tests/contracts/verify-ipc-contract.mjs`，确认当前命令枚举与字段漂移导致失败。

**实现：**

1. 补齐 `branch: string | null`、`isGitRepository`、任务模式、推理强度、权限和网络策略。
2. 明确 `targetBranch`、`taskBranch`、`worktreePath` 的持久化语义。
3. 为 B/C/D 共享对象定义必需字段、枚举和可空规则。
4. 根 `npm run check` 纳入 `check:contracts`。

**验证：**

```powershell
npm run check:contracts
npm run check:architecture
```

### 任务 1.2：生成或校验三端类型

**文件：**

- 修改：`apps/desktop/src/types/domain.ts`
- 修改：`apps/desktop/src/api/tauriClient.ts`
- 修改：`agent/app/api/models.py`
- 新增：`agent/tests/test_ipc_contract.py`

**步骤：**

1. 增加 JSON Schema 示例对象验证测试。
2. 修正 Rust/TS/Python 命名、枚举、可空字段和时间格式。
3. 对请求/响应增加序列化 round-trip 测试。
4. 禁止前端客户端调用未进入 Schema 的命令。

**验证：**

```powershell
py -m pytest agent/tests/test_ipc_contract.py -q
npm run build:desktop
```

---

## 阶段 2：真实任务工作区与恢复

### 任务 2.1：持久化任务工作区策略与目标分支

**文件：**

- 新增：`database/migrations/0009_task_workspace_contract.sql`
- 修改：`apps/desktop/src-tauri/src/storage/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/tasks.rs`
- 修改：`apps/desktop/src-tauri/src/commands/merge.rs`
- 修改：`apps/desktop/src/types/domain.ts`

**测试先行：**

1. Git 任务创建后断言数据库持久化 `task_branch`、`target_branch`、`workspace_kind`、`source_path` 和 `worktree_path`。
2. 创建任务后切换原仓库分支，断言 merge preview 仍使用冻结目标分支。
3. 篡改 source branch 后断言合并被阻断。
4. 从 0008 升级到 0009，断言旧任务可读取。

**实现：**

1. 在任务创建时冻结目标分支。
2. 合并只消费任务持久化分支，不重新推断当前分支。
3. merge preview 与 merge action 使用同一校验函数。
4. 修正 `available` 与 `canMerge` 矛盾状态。

**验证：**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml target_branch -- --nocapture
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml merge -- --nocapture
```

### 任务 2.2：实现日常/编程模式与非 Git 隔离副本

**文件：**

- 修改：`apps/desktop/src-tauri/src/git/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/repository.rs`
- 修改：`apps/desktop/src-tauri/src/commands/tasks.rs`
- 修改：`apps/desktop/src/features/tasks/NewTaskDialog.tsx`
- 修改：`apps/desktop/src/state/appStore.ts`
- 修改：`apps/desktop/src/i18n/locales/zh-CN.json`
- 修改：`apps/desktop/src/i18n/locales/en-US.json`

**测试先行：**

1. 仓库子目录应识别到 Git 根目录。
2. 日常模式非 Git 目录：同意初始化 Git 后创建 worktree。
3. 日常模式拒绝初始化 Git 后创建隔离副本。
4. 编程模式非 Git 目录默认创建隔离副本。
5. 编程模式只有显式授权时可直接修改原目录。
6. 复制前空间不足时不创建半成品目录。

**实现：**

1. 增加 `daily` / `coding` 工作模式和工作区策略。
2. 隔离副本忽略 `.git`、构建缓存、依赖目录和用户可配置排除项。
3. 复制前计算预计大小与目标空间。
4. UI 展示源路径、工作路径、预计占用、恢复与清理说明。

**验证：**

```powershell
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml workspace_kind -- --nocapture
npm run build:desktop
```

### 任务 2.3：任务创建事务补偿与双任务隔离

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/tasks.rs`
- 修改：`apps/desktop/src-tauri/src/commands/agent.rs`
- 修改：`apps/desktop/src/features/tasks/NewTaskDialog.tsx`
- 修改：`scripts/check-a-line.mjs`

**测试先行：**

1. Agent session 创建失败后，不保留任务、分支、worktree 或副本。
2. 数据库写入失败后清理文件系统产物。
3. 同仓库创建两个任务，断言 branch、worktree、日志和 Diff 完全隔离。
4. 应用重启后两个任务均可恢复。

**实现：**

1. 将任务创建拆为 prepare、persist、start-agent、commit 四步。
2. 每步注册补偿动作，失败时逆序回滚。
3. 保存可恢复的创建失败原因，不伪装为成功。

**验证：**

```powershell
npm run check:a-line
```

---

## 阶段 3：真实模型驱动 Agent

### 任务 3.1：建立可测试的统一模型网关

**文件：**

- 新增：`agent/app/model_gateway.py`
- 修改：`agent/app/providers/openai_compatible.py`
- 修改：`agent/app/api/models.py`
- 新增：`agent/tests/test_model_gateway.py`

**测试先行：**

1. 使用本地 mock transport 验证请求模型、消息与结构化输出。
2. 供应商失败、超时、无 usage 和无效响应均返回稳定错误。
3. 网关记录请求 ID、模型、延迟和 usage，不记录 API Key。
4. 直接 provider 调用在生产入口不可达。

**实现：**

1. 抽象 `ModelGateway` 与可注入 transport。
2. `/models/chat` 与 Agent graph 统一调用网关。
3. 为后续 Privacy、Token Budget 和 Model Arena 预留拦截器。

**验证：**

```powershell
py -m pytest agent/tests/test_model_gateway.py -q
```

### 任务 3.2：模型生成 Todo 与结构化编辑计划

**文件：**

- 修改：`agent/app/graph/nodes.py`
- 修改：`agent/app/graph/state.py`
- 新增：`agent/app/editing/models.py`
- 新增：`agent/app/editing/apply.py`
- 新增：`agent/tests/test_model_driven_editing.py`

**测试先行：**

1. 给定任务和仓库上下文，mock 模型返回不同 Todo，断言不使用固定模板。
2. 结构化编辑只允许工作区内相对路径。
3. 路径穿越、二进制覆盖和工作区外修改被拒绝并生成审批或失败状态。
4. 应用编辑后 Diff 非空且内容符合模型计划。

**实现：**

1. 定义 Todo 与文件编辑的严格 Pydantic 模型。
2. `plan_node` 调用模型网关生成 Todo。
3. `edit_node` 应用结构化 create/update/delete 操作。
4. 写文件前接入安全检查和后续 Hook 接口。

**验证：**

```powershell
py -m pytest agent/tests/test_model_driven_editing.py -q
```

### 任务 3.3：真实验证日志驱动自动修复

**文件：**

- 修改：`agent/app/graph/nodes.py`
- 修改：`agent/app/graph/state.py`
- 修改：`apps/desktop/src-tauri/src/commands/agent.rs`
- 修改：`apps/desktop/src-tauri/src/commands/exec.rs`
- 修改：`agent/tests/test_s11_mvp_acceptance.py`
- 新增：`agent/tests/test_model_repair_loop.py`

**测试先行：**

1. 验证失败日志不包含 `CODEMAX_REPAIR`，模型仍能产生正确修复。
2. 修复后重新验证并进入完成状态。
3. 达到上限进入 `needsIntervention`。
4. 每轮分析、编辑和验证事件同步到 Rust 时间线。

**实现：**

1. 用结构化 repair response 替代日志中特制指令。
2. 保留旧解析器仅用于历史兼容，不进入新任务默认路径。
3. 修复轮次、失败摘要和修改文件写入持久化事件。

**验证：**

```powershell
py -m pytest agent/tests/test_model_repair_loop.py agent/tests/test_s11_mvp_acceptance.py -q
npm run check:s11
```

### 任务 3.4：任务详情重启恢复

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/tasks.rs`
- 修改：`apps/desktop/src-tauri/src/commands/diff.rs`
- 修改：`apps/desktop/src/features/tasks/TaskOverviewPage.tsx`
- 新增：`tests/frontend/task-recovery.spec.ts`

**测试先行：**

1. 创建任务、生成 Diff 和日志后重建前端状态。
2. 断言 Diff、Todo、事件、验证轮次和合并记录仍可查看。
3. 大日志只加载首段，继续滚动时增量读取。

**实现：**

1. 任务详情 API 返回 Diff 索引而不是依赖组件内存。
2. 前端以持久化详情为主，实时事件只做增量更新。

---

## 阶段 4：隐私、契约、预算与记忆

### 任务 4.1：扩展敏感扫描与统一脱敏

**文件：**

- 修改：`apps/desktop/src-tauri/src/privacy/mod.rs`
- 新增：`apps/desktop/src-tauri/src/privacy/patterns.rs`
- 修改：`apps/desktop/src-tauri/src/commands/privacy.rs`
- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`

**测试先行：**

1. 增加 JWT、Bearer、URL 凭据、Gitee/GitLab/GitHub Token、数据库 URL、证书和私钥攻击样本。
2. 覆盖分块日志边界、Diff、任务描述、审批评论和 Hook 消息。
3. 断言扫描不把普通包含 `token` 字样的业务文本整行误删。

**实现：**

1. 建立统一 `SensitiveDataScanner`。
2. 日志、模型、Privacy Ledger 和 Proof Pack 复用同一扫描服务。
3. 返回脱敏类型与位置摘要，不返回原秘密。

### 任务 4.2：模型网关接入 Privacy Ledger 与真实 usage

**文件：**

- 修改：`agent/app/model_gateway.py`
- 修改：`agent/app/api/models.py`
- 修改：`apps/desktop/src-tauri/src/privacy/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/privacy.rs`
- 新增：`agent/tests/test_private_model_gateway.py`

**测试先行：**

1. 请求包含秘密时在发送前脱敏或阻断。
2. 供应商接收的 payload 不含原秘密。
3. 真实 usage 写入 Token Budget；无 usage 时标记 `estimated`。
4. Privacy Ledger 记录实际模型、供应商、上下文来源和脱敏动作。

**实现：**

1. Rust 向 Agent 提供任务级隐私/预算上下文。
2. Python 网关回传发送摘要与 usage。
3. Rust 持久化账本和预算记录。

### 任务 4.3：执行完整 Run Contract

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/exec.rs`
- 修改：`apps/desktop/src-tauri/src/safety/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/approvals.rs`
- 修改：`apps/desktop/src-tauri/src/commands/tasks.rs`

**测试先行：**

1. 表驱动测试覆盖允许/拒绝命令、路径、网络、权限、单次预算和总预算。
2. 普通但不在 allowlist 的命令也必须按契约处理。
3. 审批通过后只允许指定动作与指定范围，不形成永久扩权。
4. 已拒绝动作可在用户修改契约后重新申请。

**实现：**

1. 每个执行入口统一调用 `ContractEvaluator`。
2. 生成结构化 breach record 与 approval request。
3. UI 只展示 Rust 返回的权威决策。

### 任务 4.4：接入 Context Budgeter

**文件：**

- 修改：`agent/app/context/retriever.py`
- 新增：`agent/app/context/budgeter.py`
- 修改：`agent/app/graph/nodes.py`
- 新增：`agent/tests/test_context_budgeter.py`

**测试先行：**

1. 默认不读取整仓。
2. 按最近消息、摘要、记忆、片段、工具结果和完整文件分层。
3. 超预算按契约降级或请求审批。
4. 记录每个上下文项来源和估算 token。

### 任务 4.5：统一长期记忆、偏好和画像

**文件：**

- 新增：`database/migrations/0010_memory_profile_closure.sql`
- 修改：`apps/desktop/src-tauri/src/commands/privacy.rs`
- 修改：`apps/desktop/src-tauri/src/storage/mod.rs`
- 修改：`agent/app/memory/service.py`
- 修改：`agent/app/api/tasks.py`
- 修改：`apps/desktop/src/features/settings/SettingsPage.tsx`
- 修改：`agent/tests/test_memory_preference_guard.py`

**测试先行：**

1. UI 删除/停用记忆后，Python Agent 下一任务不再读取。
2. 候选偏好不会自动写入；接受、编辑后接受、忽略、拒绝和不再提示均可追溯。
3. 六个预置画像影响模型、权限、验证、预算、Gate、语言和记忆范围。
4. 任务详情显示实际使用的画像、偏好和记忆。

**实现：**

1. SQLite 成为长期记忆唯一来源。
2. Python `memory.json` 仅保留可迁移的临时会话数据，完成迁移后停止作为长期源。
3. 增加记忆编辑、停用、来源、作用域和最近使用时间。

---

## 阶段 5：不可变证据与交付门禁

### 任务 5.1：版本化 Proof Pack 与新鲜度

**文件：**

- 新增：`database/migrations/0011_proof_pack_versions.sql`
- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`
- 修改：`apps/desktop/src-tauri/src/commands/merge.rs`

**测试先行：**

1. 同任务生成两次 Proof Pack，目录和 ID 不相同，旧文件不被覆盖。
2. 清单包含文件哈希与输入快照哈希。
3. Diff、验证、审批、隐私或契约变化后，旧包标记 `stale`。
4. 合并后生成包含最终 `merge-record.json` 的新版本。
5. 统一敏感扫描确保所有文件无攻击样本明文。

**实现：**

1. 使用 `proof-pack/<version-id>` 目录。
2. 数据库记录 generation、supersedes、freshness 与 manifest hash。
3. UI 显示最新、陈旧和最终版本。

### 任务 5.2：统一 Gate、审批、Risk 生命周期

**文件：**

- 新增：`database/migrations/0012_delivery_decision_lifecycle.sql`
- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`
- 修改：`apps/desktop/src-tauri/src/commands/merge.rs`

**测试先行：**

1. 失败后成功的同一验证命令以当前有效轮次为准。
2. 被拒绝审批在修订后可形成新审批，不永久阻断。
3. 已处理 Risk 和已批准 override 不重复阻断。
4. Gate 输入变化后旧结果失效。
5. 没有新鲜 Proof Pack 时禁止合并。

**实现：**

1. 为 Gate、Risk、Approval、Breach 增加 pending/resolved/overridden/superseded 状态。
2. 建立一个权威 `DeliveryDecisionEvaluator`。
3. merge preview 与 merge action 复用同一快照。

### 任务 5.3：完善 Delivery Score 与 Task Capsule

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`
- 修改：`apps/desktop/src/features/tasks/TaskOverviewPage.tsx`

**测试先行：**

1. Score 覆盖验证、Diff、修复、风险、审批、契约、隐私和用户反馈。
2. 输入变化后旧 Score 不再优先展示。
3. Task Capsule 指向不可变证据版本，临时数据清理后仍可打开。

### 任务 5.4：实现真实 Rules 与 Hooks 引擎

**文件：**

- 新增：`apps/desktop/src-tauri/src/rules/mod.rs`
- 新增：`apps/desktop/src-tauri/src/hooks/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/exec.rs`
- 修改：`apps/desktop/src-tauri/src/commands/agent.rs`
- 修改：`apps/desktop/src-tauri/src/commands/merge.rs`
- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`

**测试先行：**

1. Bug 修复、补测试、安全检查、发布前检查四类内置规则真实命中并写事件。
2. 命令前、写文件前、验证后和合并前 Hook 均可阻断。
3. Hook 执行命令或扩权时必须引用状态为 approved 的审批。
4. Hook 超时、失败和取消不会静默放行。

### 任务 5.5：实现真实 Model Arena

**文件：**

- 修改：`agent/app/proposals.py`
- 新增：`agent/app/arena.py`
- 修改：`agent/app/model_gateway.py`
- 修改：`apps/desktop/src-tauri/src/commands/s12_evidence.rs`
- 修改：`apps/desktop/src/features/tasks/NewTaskDialog.tsx`
- 新增：`agent/tests/test_model_arena.py`

**测试先行：**

1. 两个模型配置各产生真实方案，不使用固定模板。
2. 方案包含影响范围、风险、预计 token/成本和验证策略。
3. 未选择方案时 Agent 不进入编辑阶段。
4. 选择记录进入事件和 Proof Pack。

---

## 阶段 6：Mac Minimal UI、主题与性能

### 任务 6.1：首页、侧栏与工作模式收口

**文件：**

- 修改：`apps/desktop/src/app/App.tsx`
- 修改：`apps/desktop/src/features/home/HomePage.tsx`
- 修改：`apps/desktop/src/features/search/SearchPage.tsx`
- 修改：`apps/desktop/src/features/skills/SkillsPage.tsx`
- 修改：`apps/desktop/src/styles/global.css`
- 修改：`apps/desktop/src/i18n/locales/zh-CN.json`
- 修改：`apps/desktop/src/i18n/locales/en-US.json`

**测试先行：**

1. 首页只包含 composer 必需入口，不加载审计面板或 Monaco。
2. 日常/编程模式可见、可持久化，并影响创建流程。
3. 所有图标按钮有本地化 `aria-label`。
4. 中英文长文案在桌面与窄屏不溢出。

**实现：**

1. 保持用户确认的纯 Mac Minimal。
2. Plus、模型/强度、项目和发送均实现完整交互。
3. 移除硬编码 `设`、`Close` 和英文输入占位符。
4. 搜索错误使用稳定本地化错误，不直接显示底层原文。

### 任务 6.2：主题、高对比、紧凑和减少动态效果

**文件：**

- 修改：`apps/desktop/src/state/appStore.ts`
- 修改：`apps/desktop/src/features/settings/SettingsPage.tsx`
- 修改：`apps/desktop/src/styles/global.css`

**测试先行：**

1. 浅色、深色、高对比可持久化。
2. 紧凑模式可与浅/深主题组合。
3. `prefers-reduced-motion` 下 slider 和启动动画无扫描、脉冲或弹性。
4. 状态不只靠颜色表达。

### 任务 6.3：任务线程、检查器和统一交付审查

**文件：**

- 修改：`apps/desktop/src/features/tasks/TaskOverviewPage.tsx`
- 修改：`apps/desktop/src/features/approvals/ApprovalsPage.tsx`
- 修改：`apps/desktop/src/styles/global.css`

**测试先行：**

1. 主线程展示真实 Todo、事件、命令、验证和修复。
2. 检查器默认折叠，按需加载隐私、契约、预算和证据。
3. 交付审查是合并前唯一入口。
4. Gate 失败时操作不可用，override 必须输入原因。
5. Proof Pack 可打开实际目录或导出位置。

### 任务 6.4：路由级懒加载与首屏资源优化

**文件：**

- 修改：`apps/desktop/src/app/App.tsx`
- 修改：`apps/desktop/src/app/routes.ts`
- 修改：`apps/desktop/vite.config.ts`
- 修改：`apps/desktop/src/features/tasks/TaskOverviewPage.tsx`

**测试先行：**

1. 首页构建 chunk 不包含 Monaco 主模块。
2. 首次打开任务 Diff 时才加载 Monaco。
3. 路由切换有稳定尺寸 loading 状态，不引发布局跳动。

**验收目标：** 主入口 gzip 明显低于当前约 1 MB；构建不再只有单个约 4 MB 主 JS。

### 任务 6.5：Playwright 视觉与可访问性验收

**文件：**

- 新增：`playwright.config.ts`
- 新增：`tests/e2e/ui-mac-minimal.spec.ts`
- 新增：`tests/e2e/ui-themes.spec.ts`
- 修改：`package.json`

**验证视口：**

- 1440x900
- 1280x720
- 390x844

**断言：**

- 页面非空。
- 首页、设置、任务、交付审查无重叠和裁切。
- 最长中英文文本不溢出。
- 键盘可操作 slider、tabs、对话框和菜单。
- 深色、高对比和紧凑模式均可读。

---

## 阶段 7：存储迁移、Agent 打包与发布

### 任务 7.1：实现存储位置与迁移向导后端

**文件：**

- 新增：`apps/desktop/src-tauri/src/storage/migration.rs`
- 修改：`apps/desktop/src-tauri/src/storage/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/app.rs`
- 修改：`apps/desktop/src-tauri/src/lib.rs`

**测试先行：**

1. 预检目标空间、权限和路径冲突。
2. 移动模式完整迁移数据库、产物和任务索引。
3. 保留模式让旧任务继续指向旧位置，新任务进入新位置。
4. 中途失败回滚配置和已移动内容。
5. 永久证据校验哈希保持不变。

### 任务 7.2：实现存储迁移 UI

**文件：**

- 修改：`apps/desktop/src/api/tauriClient.ts`
- 修改：`apps/desktop/src/types/domain.ts`
- 修改：`apps/desktop/src/features/settings/SettingsPage.tsx`
- 修改：`apps/desktop/src/i18n/locales/zh-CN.json`
- 修改：`apps/desktop/src/i18n/locales/en-US.json`

**步骤：**

1. 显示当前位置、目标位置、预计移动量和可用空间。
2. 提供移动或保留旧数据选择。
3. 迁移前确认影响范围。
4. 显示进度、失败原因、回滚结果和历史位置。

### 任务 7.3：真实模型连接测试

**文件：**

- 修改：`apps/desktop/src-tauri/src/commands/models.rs`
- 修改：`apps/desktop/src/features/settings/SettingsPage.tsx`

**测试先行：**

1. 本地 mock server 接收最小 OpenAI-compatible 请求。
2. 认证失败、模型不存在、超时和无效响应映射为稳定错误。
3. 返回 provider、model、host 和 latency，不返回 Key 或响应敏感正文。

### 任务 7.4：Python Agent 随包分发与启动

**文件：**

- 修改：`agent/pyproject.toml`
- 新增：`scripts/build-agent-runtime.ps1`
- 修改：`apps/desktop/src-tauri/tauri.conf.json`
- 修改：`apps/desktop/src-tauri/src/agent/mod.rs`
- 修改：`apps/desktop/src-tauri/src/commands/agent.rs`

**测试先行：**

1. 打包目录不存在时启动自检返回明确 blocked。
2. sidecar 启动后健康检查通过。
3. Agent 异常退出可重启并保留任务检查点。
4. 安装态不依赖源码目录或用户预装 Python。

**实现选择：** 优先构建独立 Agent 可执行产物作为 Tauri sidecar；若体积或兼容性不满足，再采用内置精简 Python 运行时。两种方式都必须记录安装体积与空闲内存。

### 任务 7.5：图标、安装器与启动自检

**文件：**

- 更新：`apps/desktop/src-tauri/icons/*`
- 修改：`apps/desktop/src-tauri/tauri.conf.json`
- 修改：`apps/desktop/src-tauri/src/commands/app.rs`
- 新增：`scripts/generate-icons.ps1`

**步骤：**

1. 从 `ico/CodeMax.png` 生成 Windows/Tauri 所需尺寸。
2. 窗口、任务栏、MSI 和 NSIS 使用同一品牌源。
3. 启动自检覆盖数据库、存储、模型、Agent 和资源。
4. 减少动态效果或动画缺失时使用静态图标。

### 任务 7.6：安装态 E2E 与发布文档

**文件：**

- 新增：`tests/e2e/installed-app-smoke.ps1`
- 修改：`scripts/check-d-line-release-smoke.mjs`
- 修改：`tests/release/verify-d-line-release-smoke.mjs`
- 新增：`docs/user-guide/installation.md`
- 新增：`docs/user-guide/configuration.md`
- 新增：`docs/user-guide/privacy-memory-storage.md`
- 新增：`docs/release/release-notes.md`
- 新增：`docs/release/known-issues.md`

**安装态流程：**

1. 安装并首次启动。
2. 完成启动自检。
3. 配置并测试模型。
4. 选择临时真实 Git 仓库。
5. 创建任务和独立 worktree。
6. Agent 生成 Todo、修改代码、验证并自动修复。
7. 审批高风险动作。
8. 打开交付审查并生成 Proof Pack。
9. Gate 通过后合并。
10. 重启应用并恢复历史。
11. 查看占用并清理临时数据，确认永久证据仍存在。

**验收：** release smoke 中所有章节必须为 `passed`，不得保留 `pending_integration` 或 `pending_package`。

---

## 阶段 8：最终回归与上线门禁

### 任务 8.1：全量自动化回归

```powershell
npm run check
npm run build:desktop
npm run check:tauri
py -m pytest agent/tests -q
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml -- --nocapture
npm run test:e2e
```

记录：

- 通过/失败数量。
- 总耗时。
- 前端 chunk 大小。
- Debug/Release 可执行文件与安装器大小。

### 任务 8.2：资源与存储基线

**文件：**

- 新增：`docs/release/resource-baseline.md`
- 新增：`scripts/measure-runtime.ps1`

**测量项：**

- 冷启动与暖启动时间。
- 空闲内存。
- 执行任务时峰值内存。
- Agent sidecar 内存。
- 日志增量加载耗时。
- 10 GB 工作区的存储扫描耗时。
- 隔离副本预计与实际占用差异。

### 任务 8.3：最终上线门禁审计

逐项核对 `最终计划.md` 的 A-01～A-08、B-01～B-10、C-01～C-10、D-01～D-12，并生成：

- `output/release-smoke/latest/release-smoke-report.json`
- `output/release-smoke/latest/release-smoke-report.md`
- 安装包路径和 SHA-256。
- 主链、隐私、画像、交付和打包 smoke 记录。
- 已知问题清单。

只有全部上线门禁通过后，才能标记 A/B/C/D 完成。

---

## 提交建议

每个任务使用独立提交，推荐顺序：

1. `fix(rust): restore tauri compile baseline`
2. `feat(contract): freeze cross-runtime ipc schema`
3. `feat(tasks): enforce isolated workspace strategies`
4. `feat(agent): add model-driven editing and repair`
5. `feat(privacy): enforce private budgeted model gateway`
6. `feat(memory): unify memory preferences and profiles`
7. `feat(delivery): version proof packs and decision gates`
8. `feat(hooks): enforce rules hooks and model arena`
9. `feat(ui): complete mac minimal task experience`
10. `feat(storage): add selectable storage migration`
11. `feat(release): bundle agent and installed-app smoke`

每次提交前运行该任务的窄测试；每个阶段结束运行相关全量回归。
