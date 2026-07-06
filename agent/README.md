# Agent Engine

Python 3.11+ Agent 引擎服务。当前阶段提供 FastAPI 本地服务、健康检查、Agent 任务基础协议和可独立启动入口。

## 本地启动

```powershell
python -m venv .venv
.\.venv\Scripts\python -m pip install -e .
.\.venv\Scripts\python -m app.main
```

默认监听 `127.0.0.1:8765`，可通过 `CODEMAX_AGENT_HOST` 和 `CODEMAX_AGENT_PORT` 覆盖。

健康检查：

```powershell
Invoke-RestMethod http://127.0.0.1:8765/health
```

## 模型配置

S5-E02 支持 `openai-compatible`、`claude`、`deepseek` 三类可调用配置入口，均通过 OpenAI-compatible chat transport 调用。Claude 类模型需要配置兼容 OpenAI Chat Completions 的 Base URL。

```powershell
$env:CODEMAX_MODEL_PROVIDER="openai-compatible"
$env:CODEMAX_MODEL_BASE_URL="https://api.example.test/v1"
$env:CODEMAX_MODEL_NAME="example-model"
$env:CODEMAX_MODEL_API_KEY="..."
```

查看当前配置状态：

```powershell
Invoke-RestMethod http://127.0.0.1:8765/api/v1/models/config
```

预留占位 provider 统一登记在 `agent/app/providers/config.py::PROVIDER_SPECS`，当前不会发起真实调用：

| provider | 位置 | 状态 |
| --- | --- | --- |
| `ds` | `PROVIDER_SPECS[ds]` | placeholder |
| `bailian` | `PROVIDER_SPECS[bailian]` | placeholder |
| `volcengine` | `PROVIDER_SPECS[volcengine]` | placeholder |
| `glm` | `PROVIDER_SPECS[glm]` | placeholder |
| `gemini` | `PROVIDER_SPECS[gemini]` | placeholder |
| `openai-gpt` | `PROVIDER_SPECS[openai-gpt]` | placeholder |
| `openai-claude` | `PROVIDER_SPECS[openai-claude]` | placeholder |
| `anthropic` | `PROVIDER_SPECS[anthropic]` | placeholder |
| `relay` | `PROVIDER_SPECS[relay]` | placeholder |

## LangGraph 状态机

S5-E03 的状态机入口在 `app/graph/workflow.py`，节点在 `app/graph/nodes.py`，状态结构在 `app/graph/state.py`。任务状态会 checkpoint 到 `CODEMAX_AGENT_CHECKPOINT_DIR`；未配置时写入用户目录下的 `.codemax/agent/checkpoints`。

当前 API 流程：

```powershell
# 创建并持久化初始状态
Invoke-RestMethod -Method Post http://127.0.0.1:8765/api/v1/tasks -ContentType application/json -Body '{...}'

# 推进 LangGraph：生成 Todo、写入 Worktree 编辑计划、生成 validationRequest
Invoke-RestMethod -Method Post http://127.0.0.1:8765/api/v1/tasks/{taskId}/advance -ContentType application/json -Body '{}'

# Rust 执行 validationRequest 后回填结果，状态机会进入 completed 或 failed 并生成 repairPlan
Invoke-RestMethod -Method Post http://127.0.0.1:8765/api/v1/tasks/{taskId}/validation-result -ContentType application/json -Body '{...}'
```

## 对话记忆与上下文

S5-E04 的记忆服务在 `app/memory/service.py`，API 在 `app/api/memory.py`。默认最近消息窗口为 50 条，可用 `CODEMAX_KEEP_RECENT_MESSAGES` 覆盖；记忆文件默认写入用户目录 `.codemax/agent/memory`，可用 `CODEMAX_AGENT_MEMORY_DIR` 覆盖。

可用接口：

```powershell
# 保存用户可见消息，并自动提取偏好、仓库命令、方案选择、审批决策
Invoke-RestMethod -Method Post http://127.0.0.1:8765/api/v1/memory/messages -ContentType application/json -Body '{...}'

# 加载最近消息、滚动摘要和相关长期记忆
Invoke-RestMethod -Method Post http://127.0.0.1:8765/api/v1/memory/context -ContentType application/json -Body '{...}'

# 检索相关长期记忆
Invoke-RestMethod http://127.0.0.1:8765/api/v1/memory/search
```

安全规则：记忆服务只保存用户可见消息、摘要、决策和结果；`analysis`、`reasoning`、`internal`、`chain_of_thought` 等内部推理角色会被拒绝。

## 预期边界

- `app/`：FastAPI 服务、API DTO、Agent 状态机。
- `app/graph/`：LangGraph 节点和状态定义。
- `app/providers/`：OpenAI-compatible、Claude、DeepSeek 等模型适配。
- `app/memory/`：最近消息、滚动摘要、长期记忆加载与清理。
- `tests/`：Agent 单元测试。

