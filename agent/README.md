# Agent Engine

Python 3.11+ Agent 引擎会放在这里，后续接入 FastAPI、LangGraph、模型供应商适配、上下文加载和状态持久化。

S0 阶段不创建虚拟环境、不安装依赖，只固定目录职责。

## 预期边界

- `app/`：FastAPI 服务、API DTO、Agent 状态机。
- `app/graph/`：LangGraph 节点和状态定义。
- `app/providers/`：OpenAI-compatible、Claude、DeepSeek 等模型适配。
- `app/memory/`：最近消息、滚动摘要、长期记忆加载与清理。
- `tests/`：Agent 单元测试。

