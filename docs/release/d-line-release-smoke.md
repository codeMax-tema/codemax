# D 线发布 Smoke 与打包验收说明

本文档定义 D 线的 `release_smoke_report` 入口，用于把源码检查、桌面构建、Tauri 检查、安装包产物和最终 smoke 缺口统一记录下来。

## 命令

- `npm run check:d-line-release`
  - 运行 `npm run check`、`npm run build:desktop`、`npm run check:tauri`。
  - 生成 `output/release-smoke/latest/release-smoke-report.json` 和 `.md`。
  - 不打安装包，适合日常快速自检。
- `npm run release:smoke:package`
  - 在上述检查基础上运行 Tauri 打包。
  - 报告会扫描 `apps/desktop/src-tauri/target/release/bundle` 下的安装包产物。

## 报告字段

- `release_smoke_report`：报告 schema 名称。
- `packaging_artifacts`：桌面 dist、调试 EXE、官方图标、安装包等产物路径和大小。
- `main_chain_smoke`：主链验收状态。
- `privacy_smoke`：隐私账本验收状态。
- `profile_memory_smoke`：画像与记忆验收状态。
- `delivery_review_smoke`：交付审查验收状态。
- `packaging_smoke`：打包与桌面构建验收状态。

## 当前边界

在 A/B/C 最新代码未统一推送前，D 线报告会把主链、隐私、画像记忆、交付审查标记为 `pending_integration`。这不是假通过，而是明确告诉上线验收还有哪些链路需要等集成后补齐。

报告会对常见 API Key、Bearer token 和已知私人令牌做脱敏处理，不应包含敏感明文。
