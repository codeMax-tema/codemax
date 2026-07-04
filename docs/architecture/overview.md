# 整体架构

## 模块布局

```text
apps/desktop/
  src/          # React 前端
  src-tauri/    # Rust + Tauri 本地后端

agent/          # Python Agent 服务
contracts/      # IPC 和本地服务契约
database/       # SQLite migration
config/         # 默认策略配置
```

## 运行关系

1. React 前端负责仓库选择、任务看板、审批、设置、Diff 和报告展示。
2. Rust/Tauri 后端负责本地文件、Git Worktree、命令执行、日志捕获、SQLite、风险扫描和合入。
3. Python Agent 负责任务规划、模型调用、代码修改策略、错误分析、滚动摘要和长期记忆提取。
4. SQLite 只保存结构化索引和轻量内容，大日志、Diff、截图和上下文文件保存在可配置产物目录。

## 调用方向

```text
React UI
  -> Tauri invoke
  -> Rust command/service
  -> SQLite / file system / Git / process
  -> local HTTP
  -> Python Agent
```

前端不直接读写本地文件，不直接调用 Python Agent，也不直接执行 Git 或命令。所有高风险操作先进入 Rust 安全层。

