# S12-E01 至 S12-E05 交付说明

## 目标

本轮完成 S12-E01 至 S12-E05 的推荐方案落地：多任务调度、主流语言上下文解析、前端截图验证、方案选择器、Proof Pack 与质量门禁等差异化能力。

## 用户侧变化

- S12-E01 多任务并行：Python Agent 创建任务时进入轻量调度器，任务可处于 running 或 queued，完成/失败后释放运行槽位。
- S12-E02 上下文增强：语言注册表覆盖 TypeScript、JavaScript、Python、Java、Go、Rust、C/C++、C#、PHP、Ruby、Kotlin、Swift、Dart、Scala、Lua、Elixir、Haskell、Zig、Solidity、Julia、Clojure 等主流语言，并保留 Tree-sitter 可用时的模式标记与 fallback 结构提取。
- S12-E03 截图验证：截图服务不再写 0 字节占位图；只有 Playwright 真实产出非空截图才返回 captured，否则返回 browserUnavailable 或 captureFailed。
- S12-E04 方案选择器：Agent 创建任务时生成多方案状态，前端任务页展示方案卡片、截图、证据包、质量门禁、交付评分和风险雷达。
- S12-E05 交付证据：新增 Proof Pack、Delivery Score、Risk Radar、Task Capsule、质量门禁记录与覆盖原因命令，并接入合入前检查。

## 关键修复

- 新增 `database/migrations/0002_s12_evidence.sql`，已运行过 `0001_initial` 的旧 SQLite 库也会补建 S12 表。
- `generate_task_proof_pack` 返回结构已对齐前端 `GeneratedTaskProofPack`，同时保留 `manifestPath`、`summaryPath`、`capsulePath` 等真实证据路径。
- IPC 契约补齐 `generate_task_proof_pack`、`record_quality_gate_result`、`override_quality_gate`。
- `ContextRetriever` 优先使用 `git ls-files`，并有 `max_scan_files` 上限，避免默认读取整个仓库。
- 质量门禁失败默认阻断合入；用户覆盖必须写入 reason 后才会解除 blocker。

## 验证结果

- `python agent/tests/test_s12_scheduler_context.py`：6 tests OK。
- `python agent/tests/test_s12_screenshot_proposals.py`：4 tests OK。
- `python agent/tests/test_s11_mvp_acceptance.py`：passed。
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s12 -- --nocapture`：5 passed。
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s12_evidence -- --nocapture`：5 passed。
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`：passed。
- `npm run check`：architecture + frontend contracts passed。
- `npm run build:desktop`：passed。Vite 仅提示 Monaco 相关 chunk 体积较大。

## 边界

- 本轮按推荐方案采用紧凑增量 UI，没有重做整体视觉风格。
- Playwright 截图依赖本机浏览器运行环境；缺失或页面不可达时会明确返回失败状态。
- S12-E06 之后的隐私账本、运行契约、Token 预算、Hooks Studio 等仍属于后续里程碑。
