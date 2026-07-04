# S0-T04 默认技术栈决策

## 桌面端

| 能力 | 技术 | 决策 |
| --- | --- | --- |
| 桌面壳体 | Tauri v2 | 低体积、强本地能力 |
| 前端框架 | React 18 + TypeScript | 适合复杂任务台和状态驱动 UI |
| 构建工具 | Vite | 快速开发和桌面端集成成熟 |
| 样式 | Tailwind CSS + shadcn/ui | 便于构建一致、可维护的工作台 UI |
| Diff | Monaco Editor Diff Mode | 只读 Diff、代码高亮和大文件策略基础 |

## 本地后台

| 能力 | 技术 | 决策 |
| --- | --- | --- |
| 系统能力 | Rust + Tauri Commands | 文件、Git、进程、安全策略由本地后台托管 |
| 异步执行 | Tokio | 命令执行、日志流、任务状态推送 |
| 命令执行 | tokio::process，必要时 portable-pty | 支持 stdout/stderr 捕获和取消 |
| 数据库 | SQLite | 本地优先，低运维成本 |
| 大产物 | 文件系统 + gzip 或 zstd | 大日志、Diff、截图不进入数据库 |

## Agent 引擎

| 能力 | 技术 | 决策 |
| --- | --- | --- |
| Agent 服务 | Python 3.11+ + FastAPI | 方便模型生态和本地 HTTP 通信 |
| 状态机 | LangGraph | 支持规划、执行、验证、修复、审批中断 |
| 模型接口 | OpenAI-compatible API 优先 | 兼容 OpenAI、Claude、DeepSeek 和私有服务 |
| 代码解析 | Tree-sitter | 后续用于精准上下文检索 |

## S0 环境确认

本机已具备 Node.js、npm、Rust、Cargo、Python 3.11。S0 不安装依赖，S1 再创建具体工程。

