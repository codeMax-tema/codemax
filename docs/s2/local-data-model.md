# S2 本地数据与任务模型

## 技术选择

S2 选择 `rusqlite` 作为 Rust SQLite 访问库，并启用 bundled SQLite。

原因：

- 桌面端本地数据访问以短事务和结构化索引为主，同步 API 足够直接。
- 依赖面比异步 ORM 更小，便于控制内存和运行时复杂度。
- bundled SQLite 能减少 Windows 用户本机缺少 SQLite 动态库导致的启动失败。

## 启动与迁移

桌面应用启动时通过 `ManagedStorage::initialize` 完成本地数据初始化：

1. 创建 app data 根目录。
2. 创建 `tasks/` 和 `worktrees/` 根目录。
3. 打开 `app.db`。
4. 启用 SQLite foreign keys。
5. 创建 `schema_migrations` 并应用 `0001_initial`。
6. 初始化默认存储策略。

## 数据访问层

`apps/desktop/src-tauri/src/storage/mod.rs` 提供 S2 数据访问入口：

- `TaskRepository`
- `TodoRepository`
- `CommandRunRepository`
- `ApprovalRepository`
- `ArtifactRepository`
- `ModelConfigRepository`
- `AppSettingsRepository`
- `MemoryRepository`
- `StoragePolicyRepository`

页面和后台业务后续应通过这些 Repository/Service 读写数据，不直接散落 SQL。

## 文件产物目录

SQLite 只保存结构化数据、摘要、状态和路径引用。大文件产物保存到文件系统：

```text
app-data/
  app.db
  tasks/
    task-001/
      logs/
      artifacts/
      screenshots/
      context/
      diff.patch
      report.json
  worktrees/
    task-001/
```

`StorageRoots` 负责生成目录和任务产物路径，`ArtifactFile` 只记录路径、大小、压缩状态和保留级别。

## 保留与清理

默认策略：

- 最近消息保留 50 条。
- 原始日志保留 30 天。
- 截图保留 30 天。
- 临时上下文保留 7 天。
- 最终 Diff 和审批记录长期保留。

`CleanupGuard` 在清理前检查最终 Diff 和审计记录策略，避免把后续审查需要的关键证据删掉。

## 验证

Rust storage 单元测试覆盖：

- S2 schema 迁移和默认策略初始化。
- Task/Todo/CommandRun/Approval/Artifact/ModelConfig/AppSettings 数据读写。
- 对话最近消息窗口和长期记忆删除。
- 产物目录规则和大文件路径索引。
- 清理前置条件阻断。
