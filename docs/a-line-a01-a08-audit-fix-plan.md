# A 线 A-01 到 A-08 审查修复清单

检查日期：2026-07-08  
范围：任务主链与 Agent 基线，覆盖 A-01 到 A-08  
结论：当前不能宣称 A-01 到 A-08 全部完成。底层存储、IPC、worktree、Diff、Delivery、Merge 骨架已建立，现有检查通过，但用户可验收的真实主链仍有关键缺口。

## 1. 已通过验证

本次检查已执行：

```powershell
npm run check
npm run build:desktop
npm run check:tauri
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run check:s11
cd agent; python -m pytest tests
```

结果：

- `npm run check` 通过。
- `npm run build:desktop` 通过。
- `npm run check:tauri` 通过。
- Rust 全量测试通过，68 passed。
- S11 检查通过。
- Python Agent 测试在 `agent` 目录下通过，37 passed。

注意：

- 从仓库根目录直接运行 `python -m pytest agent/tests` 会因为 `app` 包路径未加入 `PYTHONPATH` 失败。这是调用路径问题，不是当前 A 线功能失败。
- gstack `/review` 的 bash 前置流程因本机无可用 bash/WSL 未能执行，但已按其 checklist 做人工审查。
- Greptile 因当前 remote 不是 `gh` 可识别 GitHub host，已按规则跳过。

## 2. 总体达标状态

| 任务 | 状态 | 判断 |
| --- | --- | --- |
| A-01 去除任务默认 demo/fixture 路径 | 基本达标 | 前端生产路径未见 `taskFixtures`、`demoDiff`、`demoDelivery` 默认数据。 |
| A-02 真实任务创建链路 | 部分达标 | 能生成 task/worktree/branch/session，但 DB 写入缺事务，UI 未注册 Agent 任务。 |
| A-03 任务列表真实化 | 部分达标 | 列表来自 DB，但状态筛选缺 `failed`、`cancelled`，状态契约有漂移。 |
| A-04 任务详情真实化 | 部分达标 | 详情按 `task_id` 聚合真实数据，但 stdout/stderr 日志内容未在 UI 加载。 |
| A-05 Agent 状态事件接入 | 部分达标 | 后端事件表和部分事件写入存在，但用户主链没有启动 Agent 的 UI 入口。 |
| A-06 自动验证与修复展示 | 未达标 | 只记录失败轮次，没有完整记录修复后再验证成功的轮次。 |
| A-07 合并基础链路联通 | 部分达标 | merge 记录和冲突记录存在，但验证 gate 数据源会误把普通命令当验证命令。 |
| A-08 多任务隔离 | 部分达标 | worktree 隔离方向正确，但创建失败可能留下 DB/FS 不一致记录。 |

## 3. P0 必修项

### P0-1 任务 UI 没有接上 Agent 执行和验证循环

证据：

- `apps/desktop/src/features/tasks/NewTaskDialog.tsx:100` 只调用 `createTaskRecord`。
- `apps/desktop/src/features/tasks/TaskOverviewPage.tsx:36` 只导入 `getTaskDetail`、`generateTaskDiff`、`generateTaskDelivery`、`generateTaskProofPack`、`mergeTask`、`prepareTaskMerge`。
- `apps/desktop/src/api/tauriClient.ts:173` 已有 `createAgentTask`，但 UI 未使用。
- `apps/desktop/src/api/tauriClient.ts:216` 已有 `runAgentValidationCycle`，但 UI 未使用。

影响：

- A-05 和 A-06 无法通过用户路径验收。
- 新建任务后只完成了本地 task/worktree/session 记录，没有真正进入 Agent 任务执行链路。
- Todo、Agent 阶段、修复轮次、验证请求都依赖手动或后端内部调用，用户看不到完整自动流程。

建议修复：

1. 新建任务成功后，使用返回的 `TaskSummary` 调用 `createAgentTask`。
2. 将 `taskId`、`repositoryPath`、`worktreePath`、`title`、`description`、`modelId`、`validationCommand` 传给 Agent。
3. 在任务详情页提供明确的 Agent 启动/继续/验证循环入口，调用 `runAgentValidationCycle`。
4. 每次 Agent 状态同步后刷新 `getTaskDetail(taskId)`。
5. UI 上展示 Agent 当前阶段、最近验证请求、最大修复轮次、当前修复轮次。

验收标准：

- 选择真实 Git 仓库后，新建任务会创建 task、branch、worktree，并注册 Agent task。
- 点击运行后，任务状态从 `queued/planning` 进入 `editing/validating/repairing/readyToMerge`。
- 任务详情的 Todo、timeline、commands、validation rounds 均来自同一个真实 `task_id`。

## 4. P1 必修项

### P1-1 任务详情没有读取真实 stdout/stderr 日志内容

证据：

- `apps/desktop/src/api/tauriClient.ts:106` 有 `readTaskCommandLog`。
- `apps/desktop/src/features/tasks/TaskOverviewPage.tsx:1127` 的 `CommandRunCard` 只显示命令、cwd、状态和 exit code。
- 未发现 `TaskOverviewPage.tsx` 调用 `readTaskCommandLog`。

影响：

- A-04 的“日志按真实 task_id 加载”未完成。
- A-06 的失败分析无法从用户界面追溯到真实 stdout/stderr。

建议修复：

1. 为每个 `CommandRunCard` 增加展开状态。
2. 展开时调用 `readTaskCommandLog({ taskId, runId, stream: 'stdout' })` 和 `readTaskCommandLog({ taskId, runId, stream: 'stderr' })`。
3. 支持分页加载：使用 `offsetBytes` 和 `nextOffsetBytes`。
4. 清晰展示日志路径、是否压缩、是否 EOF。
5. 保持后端已有脱敏结果，不在 UI 中展示原始密钥。

验收标准：

- 任务详情中每条命令可展开查看 stdout/stderr。
- 大日志能增量加载。
- 日志来自 `task_id + run_id`，切换任务不会串数据。

### P1-2 验证轮次只记录失败，不记录修复后成功轮次

证据：

- `apps/desktop/src-tauri/src/commands/exec.rs:186` 只有验证失败时调用 `record_validation_failure`。
- `apps/desktop/src-tauri/src/commands/exec.rs:652` 写入 `validation_rounds` 时状态固定为 `failed`。
- 未发现成功验证写入 `validation_rounds` 的逻辑。

影响：

- A-06 要求“每轮失败、分析、修复、再验证可追溯”，当前只能看到失败轮次。
- 修复后通过验证不能和前一轮失败形成闭环。

建议修复：

1. 将 `validation_rounds` 的记录从“失败专用”改成“每次验证命令都记录”。
2. 字段建议：
   - `status`: `failed`、`passed`、`cancelled`、`timedOut`
   - `analysis`: 失败摘要或通过摘要
   - `repair_summary`: 对应修复轮次说明
   - `validation_summary`: 命令、cwd、日志路径、exit code
3. `repair.started` 和 `repair.finished` 事件要能关联同一轮 `round_index` 或 `repair_round`。
4. 最终通过验证时，保留前面的失败轮次，不覆盖。

验收标准：

- 一个“失败 -> 修复 -> 再验证通过”的任务至少能看到两条验证记录。
- 失败记录包含失败摘要和日志路径。
- 通过记录包含再验证命令和通过结果。
- UI 中按轮次顺序展示完整过程。

### P1-3 Delivery 和 Merge 会把普通命令误判为验证命令

证据：

- `apps/desktop/src-tauri/src/commands/exec.rs:736` 只有 `run_id` 以 `validation-` 开头才识别为验证命令。
- `apps/desktop/src-tauri/src/commands/delivery.rs:224` 对所有 `command_runs` 做汇总。
- `apps/desktop/src-tauri/src/commands/merge.rs:321` 也对所有 `command_runs` 做验证判断。

影响：

- 一个普通 `echo ok` 或非验证命令通过后，可能导致 delivery/merge 误认为验证通过。
- A-07 的 merge gate 可信度不足。

建议修复：

1. 在 `command_runs` 增加结构化字段，例如 `kind` 或 `purpose`，值至少包含 `validation`、`edit`、`diagnostic`。
2. 短期可先用 `run_id.starts_with("validation-")` 过滤，但建议落库字段，避免依赖命名约定。
3. Delivery 和 Merge 只使用验证命令计算 `latest_validation_status`。
4. UI 命令列表可以显示全部命令，但验证摘要只统计 validation 命令。

验收标准：

- 非验证命令不会让 `latest_validation_status` 变成 `passed`。
- merge precheck 必须存在至少一条通过的验证命令。
- failed/cancelled/timedOut 验证命令会阻止默认合并。

### P1-4 真实任务创建不是事务写入

证据：

- `apps/desktop/src-tauri/src/commands/tasks.rs:291` 调用 `persist_created_task`。
- `apps/desktop/src-tauri/src/commands/tasks.rs:457` 先插入 task，再插入 agent session、artifact file、events、todos。
- 失败时 `create_task_record` 只回滚文件系统和 worktree，没有 DB transaction。

影响：

- 中途失败可能留下 task 已存在、worktree/artifact 已删除或 session 不完整的幽灵任务。
- A-02 和 A-08 的“数据库和文件系统真实记录一致”“多任务不污染”存在风险。

建议修复：

1. 在 `persist_created_task` 内使用 `rusqlite` transaction。
2. 所有 task、agent session、artifact file、events、todos 的写入在同一事务中提交。
3. transaction 失败后回滚 DB，再回滚 worktree 和 artifact 目录。
4. 增加测试：模拟 todo/event/artifact 写入失败，确认不会留下 task 记录。

验收标准：

- 任一持久化步骤失败后，DB 不存在半成品 task。
- 文件系统和 DB 不会互相指向不存在的路径。
- 重试创建任务不会被幽灵记录阻塞。

## 5. P2 修复项

### P2-1 状态契约不完整，筛选缺少 failed/cancelled

证据：

- `apps/desktop/src/app/App.tsx:43` 的 `taskStatusFilters` 缺少 `failed`、`cancelled`。
- `apps/desktop/src/types/domain.ts:1` 仍包含 `created`、`analyzing`、`running`、`waitingApproval`、`completed`、`merging` 等旧状态。

影响：

- A-03 的状态筛选不完整。
- A 线共享契约和前端类型不一致，B/C/D 联调容易出现状态解释不一致。

建议修复：

1. 前端 `TaskStatus` 收敛到 A 线契约：
   - `queued`
   - `planning`
   - `editing`
   - `validating`
   - `repairing`
   - `awaitingApproval`
   - `awaitingReview`
   - `readyToMerge`
   - `merged`
   - `needsIntervention`
   - `failed`
   - `cancelled`
2. 如果需要兼容 Agent 内部 phase，单独保留 `AgentTaskPhase`，不要混入 `TaskStatus`。
3. `taskStatusFilters` 增加 `failed`、`cancelled`。
4. 检查 i18n 状态 key 已存在，目前 `status.failed` 和 `status.cancelled` 已有。

验收标准：

- UI 可筛选所有 A 线状态。
- 任务状态与 Agent phase 类型分离。
- 后端返回未知状态时 UI 有清晰 fallback，但不主动引入旧状态。

### P2-2 IPC schema 漏掉 validationCommand

证据：

- `apps/desktop/src-tauri/src/commands/tasks.rs:33` 支持 `validation_command`。
- `apps/desktop/src/api/tauriClient.ts:250` 支持 `validationCommand`。
- `contracts/ipc.schema.json:598` 的 `CreateTaskRecordRequest` 未声明 `validationCommand`。

影响：

- 共享契约不完整，B/C/D 或测试工具按 schema 生成客户端时会丢字段。

建议修复：

1. 在 `contracts/ipc.schema.json` 的 `CreateTaskRecordRequest` 增加：

```json
"validationCommand": {
  "type": ["string", "null"]
}
```

2. 如果 schema 有生成流程，重新生成或同步对应类型。
3. 增加 architecture 检查，确保 Rust request、TS request、IPC schema 字段一致。

验收标准：

- schema、TS、Rust 三者字段一致。
- 新建任务时 validation command 可以从 UI 传到 DB/agent session 文件。

### P2-3 现有 S11 验收不是 UI 主链验收

证据：

- `apps/desktop/src-tauri/src/commands/s11_acceptance.rs:309` 手动用 `TaskRepository` 创建 task。
- `apps/desktop/src-tauri/src/commands/s11_acceptance.rs:39` 手动调用 `git::create_task_worktree`。
- `scripts/check-s11.mjs:27` 跑的是 Python demo acceptance 和 Rust s11 测试。
- `tests/frontend/verify-s6-ui.mjs` 是静态字符串检查，不验证真实 IPC/UI 操作。

影响：

- 当前测试能证明底层模块能拼起来，但不能证明真实桌面用户路径可用。
- A-01 到 A-08 的“用户创建真实任务闭环”缺少自动验收保护。

建议修复：

1. 增加一个 A 线 smoke 测试，直接调用 `create_task_record` command 内层或 Tauri IPC 层，而不是手动插 DB。
2. 覆盖以下步骤：
   - 创建临时真实 Git repo。
   - 调用真实创建任务链路。
   - 确认 DB task、agent session 文件、worktree 都存在。
   - 调用 Agent 验证循环或命令验证。
   - 生成 Diff、Delivery。
   - prepare merge、merge。
   - 重开 storage 后能恢复任务详情。
3. 增加前端层测试或 Playwright smoke，覆盖 UI 新建任务、列表展示、详情加载、状态刷新。

验收标准：

- 不依赖手动插入固定 `TASK_ID`。
- 失败时能定位到 UI、IPC、DB、worktree 哪一层断了。
- 能证明“用户可执行的主链”而不仅是模块可用。

## 6. 建议修复顺序

1. 先修 P0-1：把 UI 新建任务接到 Agent task 和验证循环，这是 A-05/A-06 的入口。
2. 修 P1-4：创建任务持久化事务，避免后续测试制造幽灵数据。
3. 修 P1-3：命令分类，先让验证 gate 可信。
4. 修 P1-2：记录成功验证轮次，补齐自动修复闭环。
5. 修 P1-1：任务详情读取 stdout/stderr 日志。
6. 修 P2-1/P2-2：状态契约和 IPC schema 收敛。
7. 最后补 P2-3：把 A 线 smoke 测试升级成真实主链验收。

## 7. 修复后必须重新跑的命令

```powershell
npm run check
npm run build:desktop
npm run check:tauri
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
npm run check:s11
cd agent; python -m pytest tests
```

建议新增并运行：

```powershell
npm run check:a-line
```

`check:a-line` 可覆盖：

- 真实 task 创建。
- worktree/branch 创建。
- Agent task 注册。
- 验证失败、修复、再验证通过。
- 日志读取。
- Diff/Delivery/Proof Pack。
- merge success/conflict 记录。
- 重启后任务详情恢复。

## 8. 不可宣称完成的条件

任一项存在时，不建议标记 A-01 到 A-08 完成：

- 新建任务后不能从 UI 启动 Agent。
- 任务详情看不到真实 stdout/stderr 日志。
- 修复后再验证通过没有轮次记录。
- Delivery/Merge 仍把普通命令当验证命令。
- 创建任务失败会留下半成品 DB 记录。
- 状态筛选缺少 A 线契约状态。
- IPC schema 与 Rust/TS 字段不一致。
- 验收测试仍只覆盖 demo repo 或手动插库路径。

