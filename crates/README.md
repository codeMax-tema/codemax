# Shared Rust Crates

共享 Rust 逻辑会放在这里。MVP 初期可以先放在 Tauri `src-tauri` 内，只有当命令执行、Git、存储、安全策略等逻辑需要复用或拆分时，再迁移成独立 crate。

## 候选 crate

- `codemax-core`：任务状态、错误类型、DTO。
- `codemax-git`：仓库校验、worktree、diff、merge。
- `codemax-exec`：命令执行、日志捕获、取消。
- `codemax-storage`：SQLite 访问、产物索引、清理策略。
- `codemax-safety`：风险扫描、审批门禁、路径越权检查。

