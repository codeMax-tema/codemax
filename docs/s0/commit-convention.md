# S0-T07 Commit Message 规范

仓库操作由用户处理；本规范用于未来 Agent 生成建议提交信息。

## 格式

```text
type(scope): summary

body
```

## 类型

| type | 用途 |
| --- | --- |
| `feat` | 新功能 |
| `fix` | 缺陷修复 |
| `docs` | 文档 |
| `style` | 纯格式或样式调整 |
| `refactor` | 不改变行为的重构 |
| `test` | 测试 |
| `chore` | 构建、配置、维护 |
| `security` | 安全策略或密钥处理 |

## scope 建议

- `desktop`
- `agent`
- `storage`
- `git`
- `exec`
- `safety`
- `memory`
- `docs`

## Agent 建议提交信息要求

- summary 不超过 72 字符。
- body 说明关键修改、验证命令和剩余风险。
- 涉及审批或强制覆盖门禁时必须写明原因。

