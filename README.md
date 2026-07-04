# Agent 编程任务调度台

本项目是面向 AI 编程 Agent 的本地桌面端任务管理与执行调度工具。

当前状态：S0 项目准备与规范。此阶段只建立轻量底座、目录约定、工程规范和环境变量基线，不安装依赖、不创建 Git worktree、不生成大体积运行产物。

## S0 产出

- MVP 范围清单：`docs/s0/mvp-scope.md`
- 核心用户流程：`docs/s0/core-user-flow.md`
- 高风险操作清单：`docs/s0/risk-operations.md`
- 默认技术栈决策：`docs/s0/tech-stack.md`
- 目录结构规范：`docs/s0/directory-structure.md`
- 代码风格规范：`docs/s0/code-style.md`
- 提交信息规范：`docs/s0/commit-convention.md`
- 环境变量规范：`docs/s0/environment.md`
- UI 与国际化基线：`docs/s0/ui-i18n-baseline.md`

## 工程边界

- 仓库和远程相关操作由用户自行处理。
- MVP 不自动推送远程仓库，不自动创建 PR，不绕过审批合入。
- 大日志、Diff、截图和临时上下文不进入 SQLite，只保存到可配置文件系统目录，并在数据库中保存索引路径。

## 架构布局

```text
apps/desktop/
  src/          # React 前端
  src-tauri/    # Rust + Tauri 本地后端

agent/          # Python Agent 服务
contracts/      # 前端、Rust、Agent 之间的契约
database/       # SQLite migration
config/         # 默认策略和命令风险配置
docs/           # 架构、规范、验收文档
tests/          # 架构契约和后续自动化测试
```

桌面前后端放在同一个 `apps/desktop` 包里，方便 Tauri IPC、打包和版本同步；Python Agent 保持独立，避免虚拟环境、模型依赖和桌面壳体耦合。
