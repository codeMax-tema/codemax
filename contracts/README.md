# Contracts

这里保存前端、Rust/Tauri 本地后端和 Python Agent 之间共享的契约。

原则：

- 前端通过 `src/api` 调用 Rust Tauri commands。
- Rust 通过本地 HTTP 调用 Python Agent。
- `agent-api.schema.json` 约束 Rust 与 Python Agent 的本地 HTTP 协议。
- 大日志、Diff、截图和上下文文件只通过路径引用进入 SQLite。
- 契约变更需要同步 TypeScript 类型、Rust DTO、Python schema 和迁移文档。

