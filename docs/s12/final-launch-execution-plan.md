# CodeMax 最终上线执行版文档（四人并行）

## 1. 文档用途

这份文档是给研发同事直接执行的版本呀。  
默认前提是产品目标、上线范围和验收口径以 [final-launch-prd.md](/D:/codemax/docs/s12/final-launch-prd.md) 为准；本文档只负责把它翻译成可分工、可排期、可联调、可验收的执行清单哦。

---

## 2. 执行总原则

1. 不再新增大而散的想法，所有开发以 `v1.0 可打包上线` 为唯一目标呀。
2. 所有任务必须围绕真数据、真流程、真审查、真打包来做。
3. 不允许继续保留 demo、fixture、假任务入口作为默认路径。
4. 每个人先守住自己的边界，再通过联调里程碑合流。
5. 所有 P0 项必须完成并通过统一验收后，才允许打包上线哦。

---

## 3. 四人分工总览

| 负责人 | 工作流 | 核心目标 |
| --- | --- | --- |
| A | 任务主链产品化 | 把任务入口、列表、详情、状态全部做成真实闭环 |
| B | Agent 运行态与审批 | 把 Agent 过程做成可看、可控、可追责 |
| C | 交付审查与亮点能力 | 把交付页做成真正的上线决策入口 |
| D | 设置、稳定性、打包与验收 | 把配置、存储、记忆、打包和上线验证收口 |

---

## 4. A 负责人执行清单：任务主链产品化

### A-1 开发目标

把“仓库选择 -> 创建任务 -> 创建 worktree -> 进入任务详情 -> 恢复历史任务”全部做成真实产品链路呀。

### A-2 开发项

| 编号 | 开发项 | 优先级 | 依赖 | 交付物 | 验收标准 | 预计工期 |
| --- | --- | --- | --- | --- | --- | --- |
| A-01 | 仓库选择真实化 | P0 | 现有 Rust 仓库检测命令 | 真实仓库选择页 | 可选择本地 Git 仓库并显示分支、脏状态、最近记录 | 0.5 天 |
| A-02 | 新建任务入口真实化 | P0 | Rust task/worktree 命令、Python agent 入口 | 新建任务弹窗/页 | 创建任务后不再写死 task id，而是真实创建 task、branch、worktree、agent session | 1 天 |
| A-03 | 任务列表去 fixture | P0 | SQLite task 数据 | 真实任务列表页 | 列表全部来自数据库，支持状态筛选与排序 | 0.5 天 |
| A-04 | 任务详情真实加载 | P0 | task/todo/command/artifact 数据 | 真实任务详情数据流 | 切换任务时不再读 demo 数据，详情可恢复 | 1 天 |
| A-05 | 任务状态恢复与导航 | P0 | 应用 store、查询层 | 状态恢复逻辑 | 重启应用后可恢复最近任务并进入详情 | 0.5 天 |
| A-06 | 任务列表与详情联动优化 | P1 | A-03/A-04 | 主工作台交互联动 | 列表选中、详情切换、状态刷新不卡顿不串数据 | 0.5 天 |

### A-3 重点文件

1. [NewTaskDialog.tsx](/D:/codemax/apps/desktop/src/features/tasks/NewTaskDialog.tsx)
2. [TaskOverviewPage.tsx](/D:/codemax/apps/desktop/src/features/tasks/TaskOverviewPage.tsx)
3. [taskFixtures.ts](/D:/codemax/apps/desktop/src/features/tasks/taskFixtures.ts)
4. `apps/desktop/src/features/tasks/*`

### A-4 边界说明

1. A 不负责审批中心完整体验呀。
2. A 不负责交付审查页的 Gate/Score/Proof Pack 聚合。
3. A 只负责把“任务主链是真的”这件事做扎实哦。

---

## 5. B 负责人执行清单：Agent 运行态与审批闭环

### B-1 开发目标

把 Agent 运行过程变成用户看得懂、可跟踪、可审批的真实工作台呀。

### B-2 开发项

| 编号 | 开发项 | 优先级 | 依赖 | 交付物 | 验收标准 | 预计工期 |
| --- | --- | --- | --- | --- | --- | --- |
| B-01 | Todo 看板真实联通 | P0 | Python workflow 事件、todo 数据 | Todo 面板 | Agent 规划后 Todo 可实时更新状态 | 0.5 天 |
| B-02 | 运行时间线与阶段态 | P0 | task status / event stream | 时间线面板 | 可看到 planning/editing/validating/repairing/awaitingApproval 等阶段 | 1 天 |
| B-03 | 实时日志面板 | P0 | Rust exec/log 命令 | 日志面板 | 日志实时滚动、支持分页/增量读取 | 1 天 |
| B-04 | 失败摘要与修复轮次展示 | P0 | 验证/修复结果数据 | 失败摘要卡片 | 每轮失败原因、修复意图、验证结果可见 | 0.5 天 |
| B-05 | 审批中心与任务详情联通 | P0 | approvals 命令和数据 | 审批抽屉/中心 | 审批结果能回写任务状态和时间线 | 1 天 |
| B-06 | 高风险动作统一入口 | P1 | 风险事件识别 | 审批入口统一化 | 危险命令、越界路径、契约突破都进统一审批流 | 0.5 天 |

### B-3 重点模块

1. `apps/desktop/src/features/tasks/*`
2. `apps/desktop/src/features/approvals/*`
3. `apps/desktop/src/api/events.ts`
4. `apps/desktop/src-tauri/src/commands/exec.rs`
5. `apps/desktop/src-tauri/src/commands/approvals.rs`

### B-4 边界说明

1. B 不重做任务创建逻辑呀。
2. B 不负责最终交付页的证据包聚合。
3. B 的核心是把“Agent 在做什么”讲清楚哦。

---

## 6. C 负责人执行清单：交付审查与亮点能力

### C-1 开发目标

把交付审查页做成最终决策入口，并把编程智能体的产品化亮点真正接出来呀。

### C-2 开发项

| 编号 | 开发项 | 优先级 | 依赖 | 交付物 | 验收标准 | 预计工期 |
| --- | --- | --- | --- | --- | --- | --- |
| C-01 | 统一交付审查页 | P0 | diff/delivery/merge 数据 | 审查页主界面 | 可统一查看 Diff、验证、审批、merge preview | 1 天 |
| C-02 | Quality Gate 前后端接通 | P0 | `quality_gate_results` 表、规则服务 | Gate 面板与门禁逻辑 | Gate 未通过默认不能合并 | 1 天 |
| C-03 | Delivery Score 接通 | P0 | `delivery_scores` 表、评分服务 | Score 面板 | 任务可展示评分、评分项和解释 | 0.5 天 |
| C-04 | Proof Pack 导出 | P0 | proof 数据、artifact 路径 | Proof Pack 导出能力 | 每个完成任务可导出完整证据包 | 1 天 |
| C-05 | Privacy Ledger 摘要入口 | P1 | Python/Rust 隐私记录 | 隐私摘要卡片 | 用户可查看读了什么、发了什么、脱敏了什么 | 0.5 天 |
| C-06 | Run Contract 摘要入口 | P1 | contract 数据 | 契约摘要卡片 | 用户可查看本次任务运行边界 | 0.5 天 |
| C-07 | Token Budget 摘要入口 | P1 | budget 数据 | 预算摘要卡片 | 用户可看到预算、上下文来源和超限提示 | 0.5 天 |
| C-08 | Risk Radar / 风险解释聚合 | P1 | 风险规则命中结果 | 风险说明面板 | 合并前可看主要风险项和来源 | 0.5 天 |

### C-3 重点模块

1. `apps/desktop/src/features/review/*`
2. `apps/desktop/src/features/tasks/*`
3. `apps/desktop/src-tauri/src/commands/diff.rs`
4. `apps/desktop/src-tauri/src/commands/delivery.rs`
5. `apps/desktop/src-tauri/src/commands/merge.rs`
6. `database/migrations/0001_initial.sql`

### C-4 边界说明

1. C 不负责模型设置与存储设置呀。
2. C 不负责打包流程。
3. C 的交付结果必须成为“合并前唯一必看页”哦。

---

## 7. D 负责人执行清单：设置、稳定性、打包与最终验收

### D-1 开发目标

把产品从“开发态工程”收成“可安装、可配置、可维护、可上线”的桌面产品呀。

### D-2 开发项

| 编号 | 开发项 | 优先级 | 依赖 | 交付物 | 验收标准 | 预计工期 |
| --- | --- | --- | --- | --- | --- | --- |
| D-01 | 模型设置页收口 | P0 | models 命令 | 模型配置页 | 可配置、脱敏展示、测试连接 | 0.5 天 |
| D-02 | 存储治理页 | P0 | storage 统计与清理服务 | 存储设置页 | 用户可见数据路径、占用和清理策略 | 1 天 |
| D-03 | Memory Cockpit | P0 | memory API/service | 记忆管理页 | 用户可查看、编辑、删除长期记忆 | 1 天 |
| D-04 | 应用启动自检与异常提示 | P0 | Tauri 启动流程 | 启动自检逻辑 | 缺配置、路径异常、Agent 未拉起时有可读提示 | 0.5 天 |
| D-05 | 国际化与主题收尾 | P1 | 现有 i18n/theme 基础 | i18n 与主题收口 | 不留硬编码关键文案，支持中英文、深浅/紧凑模式 | 0.5 天 |
| D-06 | 品牌图标与安装资源统一 | P0 | `ico/CodeMax.png` | 正式图标资源 | 安装包、窗口、任务栏统一图标 | 0.5 天 |
| D-07 | Tauri 打包与安装器校验 | P0 | 全模块联通 | 安装包产物 | 可构建 Windows 安装包并成功安装 | 1 天 |
| D-08 | 最终 smoke 验证与交付说明 | P0 | A/B/C 完成联调 | 验收报告与上线说明 | 按最终验证清单完整跑通 | 1 天 |

### D-3 重点模块

1. `apps/desktop/src/features/settings/*`
2. `agent/app/api/memory.py`
3. `agent/app/memory/service.py`
4. `apps/desktop/src-tauri/src/commands/models.rs`
5. `apps/desktop/src-tauri/src/storage/*`
6. `apps/desktop/src-tauri/tauri.conf.json`

### D-4 边界说明

1. D 不重做主任务链和交付页呀。
2. D 的目标是让产品“真的能发出去”，不是继续扩需求哦。

---

## 8. 联调顺序

### 8.1 第一轮联调：A + B

目标：

1. 真任务创建成功。
2. 任务详情页能看到 Todo、状态、日志、审批。

完成标准：

1. 从新建任务到进入详情页无假数据。
2. Agent 运行过程在 UI 中可见。

### 8.2 第二轮联调：B + C

目标：

1. 运行过程和交付结果接起来。
2. 审批、验证、Diff、审查页口径一致。

完成标准：

1. 交付审查页能拿到运行态、审批、验证、Diff 真数据。

### 8.3 第三轮联调：C + D

目标：

1. 把亮点能力、设置、存储、记忆、预算、契约串起来。

完成标准：

1. 审查页与设置页之间数据一致。
2. Proof Pack、Privacy、Memory、Contract、Budget 有实际入口与结果。

### 8.4 第四轮总联调：A + B + C + D

目标：

1. 完成完整主链闭环。
2. 完成最终打包与 smoke。

完成标准：

1. 按最终验证清单全部通过。

---

## 9. 统一依赖关系

| 模块 | 主要依赖 |
| --- | --- |
| A 任务主链 | Rust task/worktree 命令、SQLite task 数据 |
| B 运行态/审批 | A 的真任务详情、Rust exec/approvals、Python workflow 事件 |
| C 交付审查 | B 的运行结果、Rust diff/delivery/merge、proof/gate/score 数据 |
| D 设置/打包 | A/B/C 联调完成、memory/storage/models 基础、正式资源文件 |

---

## 10. 统一验收口径

### 10.1 各负责人本地验收

每个人在提测前必须先完成自己模块的最小自验呀：

1. 页面能打开。
2. 数据是真实的。
3. 边界错误有提示。
4. 不引入新的 demo/fixture 默认路径。
5. 核心流程至少手动跑一遍。

### 10.2 联调验收

联调必须确认：

1. 任务创建后进入真实详情页。
2. Agent 过程可见。
3. 审批动作可回写。
4. 审查页可统一决策。
5. 合并成功或冲突都能正确处理。

### 10.3 上线前总验收

必须严格按 [final-launch-prd.md](/D:/codemax/docs/s12/final-launch-prd.md) 中“最终验证清单”执行呀，尤其是：

1. 自动化验证。
2. 手工主链 smoke。
3. 安全 smoke。
4. 打包 smoke。

---

## 11. 建议排期

这是按四人并行、尽量不互相阻塞估的一个实际排期呀：

| 阶段 | 参与人 | 目标 | 预计时长 |
| --- | --- | --- | --- |
| 第 1 阶段 | A/B | 主链真数据 + 运行态可见 | 2-3 天 |
| 第 2 阶段 | B/C | 审批闭环 + 交付审查页 | 2 天 |
| 第 3 阶段 | C/D | 亮点能力入口 + 设置存储记忆 | 2 天 |
| 第 4 阶段 | A/B/C/D | 总联调 + 修缺陷 | 1-2 天 |
| 第 5 阶段 | D 主导，全员配合 | 打包 + smoke + 上线门禁 | 1 天 |

总计建议：`8-10 天` 可完成一版上线收口哦。  
如果中间出现结构级问题，比如 Python 事件口径或审查页数据模型需要重构，整体预留再加 `1-2 天` 更稳妥呀。

---

## 12. 风险与预警

### 12.1 高风险点

1. 任务详情页仍残留 demo fallback，容易让联调出现“看起来能跑，实际是假数据”呀。
2. Python workflow 事件如果粒度不够，B 的运行态会很难做漂亮。
3. Quality Gate、Delivery Score、Proof Pack 如果只有表没有服务，会拖慢 C。
4. Memory、Privacy、Contract、Budget 如果没有统一数据结构，D 会被迫补后端口径。
5. 打包时 Python Agent 的携带方式如果不稳定，会直接卡上线。

### 12.2 风险处理建议

1. A 第一天先把假数据入口全部找全并拉清单。
2. B 先与 Python 侧确认事件字段和状态字典。
3. C 在做 UI 前先确认 gate/score/proof 的最小数据结构。
4. D 尽早验证一次本地打包，不要等最后一天。

---

## 13. 每日同步建议

建议每天同步一次，格式固定为下面 6 项呀：

1. 今天完成了什么。
2. 当前阻塞点是什么。
3. 是否影响别人的模块。
4. 是否改了共享数据结构。
5. 明天要做什么。
6. 是否需要联调。

---

## 14. 最终交付要求

最终交付给你时，四位同事至少要给出这些东西呀：

1. 代码改动。
2. 各自模块自验说明。
3. 联调记录。
4. 最终 smoke 记录。
5. 打包产物路径。
6. 已知风险清单。

---

## 15. 一句话执行结论

这次不是继续“做功能”啦，而是四个人围绕一个共同目标收口成品：  
A 把任务链做真，B 把过程做清楚，C 把交付做可信，D 把产品做能发。  
四条线都收住以后，按统一验证清单跑完，就可以直接上线哦。
