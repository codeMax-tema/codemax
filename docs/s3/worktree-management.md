# S3-E02 Worktree 管理

## 目录规则

任务 Worktree 默认创建在应用数据目录的 `worktrees/` 根目录下：

```text
app-data/
  worktrees/
    task-001/
```

目录名来自任务 ID。若任务 ID 已经是安全的 ASCII 标识，例如 `task-001`，目录名保持不变；若包含空格、中文或特殊字符，系统会生成可读 slug 并追加稳定短哈希，保证路径唯一且仍可追踪到原始任务。

任务分支采用 `agent/<task-id>` 规则，例如：

```text
agent/task-001
```

该规则满足每个任务拥有独立目录和独立分支，且路径、分支名、任务 ID 会写入 `tasks.worktree_path` 与 `tasks.branch_name`。

## 后端接口

| 命令 | 用途 |
| --- | --- |
| `create_task_branch` | 为指定仓库和任务 ID 创建任务分支；如果分支已存在则返回现有分支名 |
| `create_task_worktree` | 根据任务表中的仓库路径，在默认 Worktree 根目录创建 Git Worktree，并持久化路径和分支 |
| `get_task_worktree_status` | 读取任务 Worktree 中新增、修改、删除的文件列表 |
| `cleanup_task_worktree` | 用户确认后通过 `git worktree remove` 清理任务工作区 |

## 存储与清理约束

- Worktree 使用 Git 原生机制，不复制完整仓库。
- SQLite 只保存路径、分支名和轻量元数据，不保存仓库副本。
- Worktree 创建成功后若元数据写入失败，系统会立即回滚刚创建的 Worktree，避免产生任务表不可追踪的孤儿目录。
- 清理接口必须收到 `confirmed=true` 才会执行。
- 清理通过 Git 原生命令执行；若 Worktree 有未提交变更导致 Git 拒绝删除，接口返回明确错误，不静默丢弃用户改动。
- 清理成功后会同步清空 `tasks.worktree_path` 与 `tasks.branch_name`；如果记录中的目录已不存在，确认清理后也会清空陈旧元数据。
