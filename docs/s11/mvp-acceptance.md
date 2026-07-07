# S11 MVP 联调与验收

## 目标

S11 验证 CodeMax MVP 的本地闭环：选择仓库、创建任务与 worktree、Agent 修改、验证失败、自动修复、生成 Diff 与交付报告、人工确认后本地合入，并覆盖审批、记忆和存储清理边界。

本阶段不新增 UI 设计，不改变自动推送策略；所有验证都在本地临时目录或内存 SQLite 中运行。

## 一键复跑

```powershell
npm run check:s11
```

该脚本依次运行：

```powershell
cd agent
python tests\test_s11_mvp_acceptance.py

cd ..
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml s11 -- --nocapture
```

Python 验收采用脚本式测试，避免为了 S11 单项检查额外创建虚拟环境或下载 pytest 依赖。Rust 验收使用 Cargo 测试和临时 Git 仓库，结束时清理测试目录。

## 覆盖映射

| 任务 | 覆盖位置 | 验收点 |
| --- | --- | --- |
| S11-T01 demo repo | `apps/desktop/src-tauri/src/commands/s11_acceptance.rs` | 创建临时 Git demo 仓库，包含失败的 `validate.py` |
| S11-T02 创建任务流 | Rust S11 主链路 | 写入 Task 记录，创建 task worktree 和任务分支 |
| S11-T03 Agent 编辑 | `agent/tests/test_s11_mvp_acceptance.py` 与 Rust 主链路 | Python 节点写入 Agent edit plan；Rust 主链路模拟确定性修复 |
| S11-T04 验证失败 | Python 与 Rust 主链路 | 首次验证退出码为 1 并记录失败 run |
| S11-T05 自动修复 | Python Agent 验收 | 解析 `CODEMAX_REPAIR` 后把 `return False` 修复为 `return True` |
| S11-T06 Diff 展示流程 | Rust 主链路 | `generate_task_diff_inner` 生成最终 diff 文件和 artifact |
| S11-T07 合入流程 | Rust 主链路 | `prepare_task_merge_inner` 通过后 `merge_task_inner` 本地合入 |
| S11-T08 仓库选择测试 | Rust 边界测试 | 无效 Git 目录被拒绝，Git 仓库被接受 |
| S11-T09 任务/worktree 测试 | Rust 主链路 | task、worktree path、branch name 持久化 |
| S11-T10 审批测试 | Rust 边界测试 | approved、rejected、revise 三类决策均可记录 |
| S11-T11 自修复测试 | Python Agent 验收 | 失败后进入 repair round，再次验证通过并完成 |
| S11-T12 合入成功/冲突测试 | Rust S10 与 S11 验收 | S11 覆盖成功合入；S10 既有测试覆盖冲突失败 |
| S11-T13 记忆测试 | Rust 边界测试 | 最近消息窗口保留 50 条，长期记忆可写入、查询、删除 |
| S11-T14 存储清理测试 | Rust 主链路 | 清理 temporary artifact file 后保留 diff 和 merge record |

## 交付边界

- 验证报告只按同一 `command + cwd` 的最新一次运行统计，避免已修复任务因为历史失败 run 被误判为失败。
- Diff、delivery report、merge record 作为 permanent artifact file 保留；临时 context artifact 可由 CleanupGuard 清理。
- S11 验收不调用模型、不访问网络、不启动桌面 UI，不写入真实用户仓库。
- 合入只执行本地 merge，不自动 push 远程。
