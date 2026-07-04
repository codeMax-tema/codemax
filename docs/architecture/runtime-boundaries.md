# 运行边界

## 桌面前后端

`apps/desktop` 是桌面应用单元，前端和 Rust/Tauri 后端放在一起：

- 前端位于 `apps/desktop/src`。
- Rust 后端位于 `apps/desktop/src-tauri`。
- IPC 契约由 `contracts/ipc.schema.json` 和 `src/api/tauriClient.ts` 共同约束。

## Python Agent

`agent` 是独立本地服务：

- Rust 负责拉起和健康检查。
- Rust 通过本地 HTTP 调用 Agent。
- Agent 不直接写用户主工作区，只接收任务 Worktree 路径。

## Worktree 和存储

- 每个任务使用独立 Git Worktree。
- Worktree 根目录通过 `CODEMAX_WORKTREE_ROOT` 配置。
- 产物根目录通过 `CODEMAX_ARTIFACT_ROOT` 配置。
- SQLite 只保存路径引用、摘要、状态和审批记录。

## 安全边界

- 主工作区默认只读，合入阶段除外。
- 删除、依赖变更、Schema 变更、危险命令和合入必须审批。
- API Key 只通过环境变量、系统凭据或加密存储引用，不写入普通日志。

