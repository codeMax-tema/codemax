# S0-T06 代码风格规范

## TypeScript / React

- 使用 TypeScript strict 配置。
- 使用 Prettier 统一格式，根目录 `.prettierrc` 已给出默认规则。
- 用户可见文本必须走 i18n key。
- 页面组件优先小而清晰，复杂业务状态放到 store 或 service 层。
- Tauri invoke 由 API client 封装，页面不直接散落调用。

## Rust

- 使用 `rustfmt`，根目录 `rustfmt.toml` 已给出默认规则。
- 错误类型要能映射为前端可读错误，不直接暴露敏感路径或密钥。
- 文件、进程、Git 操作先做路径和风险校验。
- 命令执行必须记录 cwd、命令、退出码、耗时和日志路径。

## Python

- Python 版本基线为 3.11+。
- 使用 Ruff + Black，根目录 `pyproject.toml` 已给出默认规则。
- Agent 状态和 API DTO 使用类型标注。
- 不保存模型不可见内部推理过程，只保存用户可见消息、摘要、决策和结果。

## 测试约定

- 行为变化优先写测试。
- Rust 核心服务、Python Agent 节点、前端关键状态和组件都要有对应测试。
- 验证命令输出必须可追踪，不能只记录“成功”或“失败”。

