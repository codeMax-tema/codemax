# S0-T05 目录结构规范

## 根目录

```text
.
  apps/
    desktop/
      src/
      src-tauri/
  agent/
    README.md
  crates/
    README.md
  database/
    migrations/
      README.md
  config/
    README.md
  docs/
    s0/
  scripts/
    README.md
  .env.example
  package.json
  pyproject.toml
  rustfmt.toml
```

## 边界说明

| 路径 | 职责 |
| --- | --- |
| `apps/desktop` | Tauri v2 桌面应用，`src` 放 React 前端，`src-tauri` 放 Rust 本地后端 |
| `agent` | Python Agent 服务，包含 FastAPI、LangGraph 和模型适配 |
| `crates` | 可复用 Rust crate，按需要从 `src-tauri` 拆分 |
| `database/migrations` | SQLite migration |
| `config` | 默认配置、命令黑白名单模板 |
| `docs` | 产品、架构、规范、验收文档 |
| `scripts` | 开发、校验、打包、smoke test 辅助脚本 |

## 本地数据目录

运行时数据不放入源码目录。默认结构如下，位置可由用户设置：

```text
app-data/
  app.db
  tasks/
    task-001/
      logs/
      artifacts/
      diff.patch
      report.json
      screenshots/
      context/
  worktrees/
    task-001/
```

## 约束

- Worktree 使用 Git 原生机制，不默认复制完整仓库。
- SQLite 不保存大日志、大截图、大 Diff 和完整仓库副本。
- 清理临时文件前必须保留最终 Diff、审批记录、合入记录和摘要索引。
