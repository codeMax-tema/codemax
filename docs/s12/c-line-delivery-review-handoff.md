# 2026-07-09 成员C交付审查链路交付文档

> 交付人：Codex
> 范围：成员C（交付审查、门禁、评分、证据、Rules/Hooks、Model Arena）
> 项目：CodeMax 本地桌面端编程智能体工作台

## 1. 本次目标

把 `最终计划.md` 中成员C的 C-01 至 C-10 从“证据展示”推进为真实可追溯的交付决策链路：合并前统一查看 Diff、验证、审批、隐私账本、运行契约、Token 预算、质量门禁、交付评分、风险、规则命中、Hook 运行与扩权审批、模型方案选择，并让默认合并门禁以这些真实记录为依据。

2026-07-09 追补重点：把此前“部分完成”的 C-01/C-02/C-06/C-08/C-10 做深。交付审查页新增隐私账本、运行契约、Token 预算和 Proof Pack 文件完整性；Proof Pack 写出最终计划要求的固定证据结构；Task Capsule 不再只是 manifest 复制，而是可复盘摘要；Hook、规则、Model Arena 进入证据文件和合并阻断。

## 2. 上下文与边界

- 已读取/参考：`最终计划.md`、`README.md`、`开发准则.md`、`docs/s12/e01-e05-handoff-zh.md`、`docs/s12/e01-e05-enhancements.md`、`docs/superpowers/specs/2026-07-08-s12-e01-e05-design.md`。
- 涉及模块：SQLite migration、Tauri/Rust `s12_evidence` 命令、IPC schema、前端 API client、任务详情交付审查区、架构/前端/Rust 测试、S12 交付文档。
- 未触碰高风险边界：未自动创建 Git worktree、未推送远程、未创建 PR、未执行真实合并、未写入任何真实密钥。
- 敏感信息处理：新增表只保存结构化状态、原因、路径和索引，不保存大日志或敏感明文。

## 3. 现状与根因

原有 S12-E05 已具备 Proof Pack、Quality Gate、Delivery Score、Risk Radar 和 Task Capsule 的基础能力，但 `Rules / Hooks / Model Arena` 主要是派生或占位状态，没有独立落库、IPC 命令和扩权审批链路。上一轮补齐了 C-07 至 C-10 的基础持久化；本轮继续补齐“部分完成”项：Proof Pack 证据结构不完整、Task Capsule 复盘信息偏薄、交付审查页缺少隐私/契约/预算摘要、合并阻断没有充分吸收 B 线证据。

## 4. 执行内容

| 模块/文件 | 动作 | 说明 |
| --- | --- | --- |
| `database/migrations/0007_c_line_delivery_review.sql` | 新增 | 增加 `rule_registry`、`rule_hits`、`hook_approvals`、`hook_runs`、`model_arena_decisions`。 |
| `apps/desktop/src-tauri/src/storage/mod.rs` | 修改 | 接入 0007 迁移，并把新表纳入存储迁移测试。 |
| `apps/desktop/src-tauri/src/commands/s12_evidence.rs` | 修改 | 新增规则命中、Hook 运行、Hook 扩权审批、Model Arena 决策命令；交付审查状态聚合新表、隐私/契约/预算摘要和完整 Proof Pack 文件结构，并阻断默认合并。 |
| `contracts/ipc.schema.json` | 修改 | 暴露 C 线新增 IPC 契约，显式声明 `proofPackFiles`、`privacyLedgerSummary`、`runContractSummary`、`tokenBudgetSummary`。 |
| `apps/desktop/src/api/tauriClient.ts`、`apps/desktop/src/types/domain.ts` | 修改 | 补齐前端调用类型和成员 C 新增审查状态类型。 |
| `apps/desktop/src/features/tasks/TaskOverviewPage.tsx`、`apps/desktop/src/styles/global.css`、`apps/desktop/src/i18n/locales/*.json` | 修改 | 统一交付审查页新增 Privacy、Run Contract、Token Budget、Proof Files、Rules、Hooks、Model Arena 面板。 |
| `tests/architecture/verify-architecture.mjs`、`tests/frontend/verify-s6-ui.mjs`、`scripts/check-d-line-release-smoke.mjs` | 修改 | 将成员C新增能力、Proof Pack 文件类型和 UI key 纳入源码契约和发布 smoke 描述。 |

## 4.1 Proof Pack 固定证据结构

本轮生成器会写出并登记以下文件：

- `task.json`
- `run-contract.json`
- `privacy-ledger.json`
- `todos.json`
- `commands.json`
- `validation-report.json`
- `diff.patch`
- `quality-gate.json`
- `delivery-score.json`
- `approvals.json`
- `risk-report.json`
- `merge-record.json`
- `summary.md`

同时额外写出 `manifest.json`、`task-capsule.json`、`context-sources.json`、`rules-hooks.json`、`model-arena.json`，用于复盘上下文来源、规则/Hook 生命周期和模型方案选择。

## 5. 验证结果

| 验证项 | 命令/路径 | 结果 |
| --- | --- | --- |
| Rust 格式化 | `cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml` | 通过 |
| 成员C Rust 单测 | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s12_evidence -- --nocapture` | 10 passed |
| 源码契约 | `npm run check` | 通过：architecture、frontend、release smoke contract |
| Tauri 检查 | `npm run check:tauri` | 通过 |
| 桌面前端构建 | `npm run build:desktop` | 通过，Vite 仅提示 Monaco chunk 体积较大 |
| Diff 空白检查 | `git diff --check` | 通过，仅有 Windows LF/CRLF 提醒 |
| Python Agent 测试尝试 | `python -m pytest agent\tests` / `python -m unittest discover -s agent\tests` | 当前环境缺 `pytest`、`pydantic`，且默认导入路径未挂 `agent`；本轮未改 Python 代码 |

## 6. 风险与遗留

- 已解决：C-01 交付审查页可聚合隐私、契约、预算、Proof Files；C-02 Proof Pack 固定结构已写出并进入 `artifact_files`；C-06 Task Capsule 具备复盘摘要和关键决策；C-07 至 C-10 有真实表、IPC、聚合状态、前端审查入口和单测覆盖。
- 边界：本轮未做真实多模型远程调用竞赛；当前 Model Arena 记录的是方案选择与对比元数据。
- 后续建议：安装包环境中补一条真实任务 smoke，验证 Proof Pack 导出的文件目录能被用户从交付审查页定位。

## 7. 结论

成员C交付链路已从 Proof/Gate/Score/Risk 扩展为包含 Privacy Ledger、Run Contract、Token Budget、完整 Proof Pack、Task Capsule、Rules、Hooks、Hook 扩权审批和 Model Arena 决策的合并前审查系统。默认合并门禁现在能够依据这些真实记录阻断或放行。
