# S8 Diff、测试报告与交付物

## 范围

S8 覆盖任务交付或合入前的最终审查能力。它把任务 Worktree 的代码改动、验证命令记录和 Agent 交付说明整理成可持久化、可重新打开的交付物。

已实现内容：

- 根据任务保存的 Worktree 和目标基准分支生成任务 Diff。
- 将最终 Patch 保存到 `app-data/tasks/<taskId>/diff.patch`。
- Diff 元数据写入 `artifacts` 和 `artifact_files`，SQLite 不保存大段 Patch 正文。
- 在任务总览页展示变更文件、增删行统计和只读 Monaco Diff 视图。
- 支持按文件树切换查看多文件 Diff。
- 大 Diff 文件默认折叠，避免渲染造成明显卡顿。
- 从 `command_runs` 汇总验证命令结果。
- 生成结构化 `report.json`，包含命令数量、状态、运行详情、变更文件、Diff 路径、交付说明路径和风险摘要。
- 生成 `artifacts/delivery.md`，包含问题、修改点、文件、验证、风险和建议 Commit Message。
- 将测试报告和交付说明作为永久 Artifact 文件记录。

## Diff 生成

桌面端命令实现在 `apps/desktop/src-tauri/src/commands/diff.rs`。

输入参数：

- `taskId`：必填，任务 ID。
- `baseRef`：可选，Git 对比基准。如果不传，则使用仓库当前分支作为对比目标。

运行行为：

- 从本地存储读取任务记录。
- 要求任务已保存 `worktree_path`。
- 调用 `apps/desktop/src-tauri/src/git/mod.rs` 中的 `git::task_diff`。
- 对已跟踪文件执行 `git diff --binary --find-renames <baseRef> --`。
- 为未跟踪文件补充 Patch，确保新建文件也能进入审查。
- 按文件拆分 Patch，返回每个文件的状态、增删行和 Patch 内容。
- 将完整 Patch 写入任务 Artifact 目录。
- 写入带 `diff_path` 的 Artifact 记录，并新增类型为 `diff` 的永久 `artifact_files` 记录。

## 交付报告

桌面端命令实现在 `apps/desktop/src-tauri/src/commands/delivery.rs`。

输入参数：

- `taskId`：必填，任务 ID。

运行行为：

- 读取任务、命令运行记录和现有 Artifact 记录。
- 优先复用最近一个包含 `diff_path` 的 Artifact。
- 如果已存在 `app-data/tasks/<taskId>/diff.patch`，则作为兜底 Diff 路径。
- 从最近的 Diff Artifact 元数据中读取变更文件列表。
- 将每条命令运行记录转换为验证摘要，包含命令、cwd、状态、退出码、耗时和创建时间。
- 计算 `overallStatus`：`passed`、`failed` 或 `notRun`。
- 生成简短测试摘要和风险说明。
- 将机器可读报告写入 `app-data/tasks/<taskId>/report.json`。
- 将人工可读交付说明写入 `app-data/tasks/<taskId>/artifacts/delivery.md`。
- 写入一条交付 Artifact，并新增 `test_report` 和 `delivery_summary` 两类永久 Artifact 文件记录。

报告 JSON 字段包括：

- `taskId`
- `artifactId`
- `taskTitle`
- `generatedAt`
- `overallStatus`
- `summary`
- `commandCount`
- `passedCount`
- `failedCount`
- `changedFiles`
- `diffPath`
- `deliveryPath`
- `runs`
- `risk`

## IPC

生成 Diff：

```ts
generateTaskDiff({
  taskId: 'task-001',
  baseRef: 'main',
});
```

生成测试报告和交付说明：

```ts
generateTaskDelivery({
  taskId: 'task-001',
});
```

返回的交付数据示例：

```ts
{
  taskId: 'task-001',
  artifactId: 'delivery-task-001-...',
  reportPath: 'app-data/tasks/task-001/report.json',
  deliveryPath: 'app-data/tasks/task-001/artifacts/delivery.md',
  diffPath: 'app-data/tasks/task-001/diff.patch',
  summary: '## 问题...',
  commitMessage: 'feat(desktop): add task delivery report...',
  report: {
    overallStatus: 'passed',
    commandCount: 3,
    passedCount: 3,
    failedCount: 0,
    runs: []
  }
}
```

## 前端审查界面

`apps/desktop/src/features/tasks/TaskOverviewPage.tsx` 在任务总览执行区域承载 S8 能力。

界面行为：

- Diff 面板展示基准分支、变更文件数量、Artifact 路径和总增删行统计。
- 文件树支持在不同变更文件之间切换。
- 普通文本 Patch 使用 Monaco `DiffEditor` 以只读双栏模式展示。
- 二进制 Patch 或无法解析为 original/modified 的 Patch 回退为预格式化 Patch 预览。
- 大文件 Diff 默认折叠，用户确认后再展开。
- 交付面板展示报告路径、交付说明路径、Artifact ID、验证状态、命令数量、验证运行列表、Agent 总结和建议 Commit Message。
- 没有真实任务输出时，页面使用演示数据维持空状态预览。

## 存储原则

S8 延续 S2 的低占用存储策略：

- SQLite 保存 Artifact 索引、路径、摘要、Commit Message、状态元数据和变更文件元数据。
- Patch 正文、交付说明等较大文件保存在任务 Artifact 根目录下。
- 最终 Diff 和交付 Artifact 使用永久保留，因为后续合入审查、证据包和清理保护都依赖它们。
- Worktree 清理前必须确认最终 Diff 已通过 Artifact 记录保留。

## 与其他阶段的关系

- 依赖 S3 提供任务 Worktree 路径和分支隔离。
- 依赖 S4 提供验证命令运行记录和日志路径。
- 依赖 S5/S7 产生 Agent 执行过程和自动修复后的最终验证状态。
- 为 S9/S10 提供审批或合入前可审查的 Diff 和验证结果。
- 为后续证据包和交付评分能力保留报告、说明、Diff、命令和风险证据。
