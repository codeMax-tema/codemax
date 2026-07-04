# S0 项目准备与规范

S0 目标是完成正式编码前的工程基线，让后续 S1-S13 可以按同一套边界推进。

## 任务映射

| S0 任务 | 产出文件 | 状态 |
| --- | --- | --- |
| S0-T01 确认 MVP 范围 | `mvp-scope.md` | done |
| S0-T02 确认核心用户流程 | `core-user-flow.md` | done |
| S0-T03 确认高风险操作范围 | `risk-operations.md` | done |
| S0-T04 确认默认技术栈 | `tech-stack.md` | done |
| S0-T05 制定目录结构规范 | `directory-structure.md` | done |
| S0-T06 制定代码风格规范 | `code-style.md` | done |
| S0-T07 制定提交信息规范 | `commit-convention.md` | done |
| S0-T08 制定环境变量规范 | `environment.md` | done |

## 用户额外要求落点

- UI：见 `ui-i18n-baseline.md`，默认简约，多 UI 可切换，真正进入 UI 设计前需要询问用户意见。
- 国际化：所有用户可见文本后续进入 i18n 资源，不在组件内硬编码。
- 本地占用：见 `environment.md` 与 `directory-structure.md`，存储路径可选且透明。

