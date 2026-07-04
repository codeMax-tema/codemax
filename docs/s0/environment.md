# S0-T08 环境变量规范

根目录 `.env.example` 是模板，不允许写入真实密钥。

## 存储与路径

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `CODEMAX_APP_DATA_DIR` | OS app-data | 应用数据根目录，用户可在设置页更改 |
| `CODEMAX_WORKTREE_ROOT` | app-data/worktrees | 任务 worktree 根目录 |
| `CODEMAX_ARTIFACT_ROOT` | app-data/tasks | 日志、Diff、报告、截图等产物根目录 |
| `CODEMAX_DATABASE_URL` | `sqlite://app-data/app.db` | SQLite 数据库地址 |

## 保留策略

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `CODEMAX_KEEP_RECENT_MESSAGES` | 50 | 每个对话保留最近原文消息数 |
| `CODEMAX_RAW_LOG_RETENTION_DAYS` | 30 | 原始日志保留天数 |
| `CODEMAX_SCREENSHOT_RETENTION_DAYS` | 30 | 截图保留天数 |
| `CODEMAX_TEMP_CONTEXT_RETENTION_DAYS` | 7 | 临时上下文保留天数 |
| `CODEMAX_MAX_REPAIR_ROUNDS` | 5 | 自动修复最大轮次 |

## UI 与国际化

| 变量 | 默认 | 说明 |
| --- | --- | --- |
| `CODEMAX_DEFAULT_LOCALE` | `zh-CN` | 默认语言 |
| `CODEMAX_FALLBACK_LOCALE` | `en-US` | 回退语言 |
| `CODEMAX_DEFAULT_THEME` | `minimal` | 默认 UI 风格 |

## 模型供应商

| 变量 | 说明 |
| --- | --- |
| `CODEMAX_MODEL_PROVIDER` | 模型供应商标识 |
| `CODEMAX_MODEL_BASE_URL` | OpenAI-compatible Base URL |
| `CODEMAX_MODEL_NAME` | 默认模型名 |
| `CODEMAX_MODEL_API_KEY` | 本地密钥，后续优先进入系统凭据或加密存储 |

## 安全要求

- `.env` 和 `.env.*` 默认被 `.gitignore` 忽略。
- 日志、错误提示、证据包和交付报告不得明文输出 API Key。
- 用户需要能在设置页看到当前存储位置、占用统计和清理策略。

