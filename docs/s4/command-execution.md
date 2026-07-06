# S4 命令执行与日志系统

## 范围

S4 第一阶段落地本地命令执行和日志捕获能力，服务后续验证命令、测试报告和任务详情实时日志面板。

已实现内容：

- `CommandExecutor` 基于 `tokio::process` 异步执行命令。
- 命令请求包含 `taskId`、`runId`、`command`、`cwd`、`env`、`timeoutMs`。
- stdout/stderr 分流捕获，并通过 `codemax://command-output` 增量推送。
- stdout/stderr 原始日志写入 `app-data/tasks/<taskId>/logs/`，SQLite 只保存路径。
- 支持超时终止和 `cancel_task_command` 主动取消。
- 执行结束后写入 `command_runs`，记录状态、退出码、耗时和日志路径。
- `cwd` 必须位于任务 Worktree 内，避免命令默认跑到主工作区或用户目录。
- 日志写入前会按敏感环境变量值做基础脱敏，避免 API Key/Token 明文落盘。
- 提供 `read_task_command_log` 按 stdout/stderr、offset 和字节上限分页读取日志。
- 提供 `summarize_task_command_log` 从日志尾部提取关键错误摘要，供测试报告和失败修复使用。
- 命令结束后超过阈值的大日志会压缩为 `.gz`，结果路径指向压缩后的文件。
- 提供 `cleanup_expired_task_logs` 按 `StoragePolicy.raw_log_retention_days` 清理过期原始日志。

## IPC

命令调用：

```ts
executeTaskCommand({
  taskId: 'task-001',
  command: 'npm test',
  cwd: 'D:/app-data/worktrees/task-001',
  timeoutMs: 120000,
});
```

取消调用：

```ts
cancelTaskCommand('cmd-run-id');
```

分页读取日志：

```ts
readTaskCommandLog({
  taskId: 'task-001',
  runId: 'cmd-run-id',
  stream: 'stderr',
  offsetBytes: 0,
  maxBytes: 65536,
});
```

提取错误摘要：

```ts
summarizeTaskCommandLog({
  taskId: 'task-001',
  runId: 'cmd-run-id',
  maxLines: 20,
});
```

清理过期日志：

```ts
cleanupExpiredTaskLogs();
```

实时日志事件：

- `codemax://command-output`
- `codemax://command-finished`

## 存储原则

延续 S2 低占用策略：SQLite 不保存大日志正文，只保存路径、退出码、状态和耗时。日志文件按任务隔离，读取时必须校验日志路径仍在任务产物根目录内。清理只删除过期原始日志，不删除最终 Diff、审批记录、合入记录或命令运行索引。
