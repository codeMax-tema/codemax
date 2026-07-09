# CodeMax Mac Codex Minimal 全应用 UI 设计稿

## 1. 设计定位

CodeMax 默认 UI 采用 **Mac Codex Minimal**。它的目标不是做传统后台首页，也不是展示内部框架能力，而是让用户打开应用后立刻进入类似 Codex 桌面版的新对话工作台：左侧是项目与对话，中央是干净的新任务 composer，设置页像 Codex 一样清晰、安静、可配置。

本设计覆盖整个桌面应用，包括启动页、首页、任务线程、搜索、技能、设置、模型与思考强度、存储、审批、交付、国际化和可访问性。用户正在制作的 CodeMax 启动动画后续作为资源接入，不影响本设计的落地。

## 2. 核心原则

1. **打开即可工作**：首页只回答“我们该做什么？”，不展示内部系统框架、模块宣传或仪表盘。
2. **像 Codex 桌面版**：左侧导航 + 项目/对话列表 + 中央 composer，保留 CodeMax 的本地、隐私、技能和交付特性。
3. **像 Mac 应用**：浅色、留白、细边框、轻阴影、柔和动效，默认克制。
4. **技能优先**：首页和侧栏展示 Skills，不展示插件。技能来自工作区或全局 `.codemax` 文件夹。
5. **设置讲人话**：模型强度和思考强度要说明“低在哪儿、高在哪儿、好处和代价是什么”。
6. **用户可控**：模型、思考强度、权限、存储位置、启动动画、语言和主题都应在设置里透明可调。

## 3. 全应用结构

```text
CodeMax Desktop
  Splash / Startup
  Home / New Conversation
  Conversation / Task Thread
  Search
  Skills
  Project Workspace
  Approvals
  Delivery Review
  Settings
```

默认窗口结构：

```text
Window
  Sidebar
  Main Canvas
  Optional Inspector / Drawer
```

主界面不默认显示右侧 inspector。只有进入任务线程、交付审查或用户主动展开详情时才显示，避免首页杂乱哦。

## 4. 首页设计

### 4.1 首页目标

打开应用后的首页必须神似用户提供的 Codex 桌面版截图：

1. 左侧固定浅灰侧栏。
2. 中央大面积留白。
3. 中央标题：`我们该做什么？`
4. 标题下方是大 composer。
5. composer 下方是项目选择行。
6. 不展示内部架构、系统模块、质量门禁、Proof Pack、存储图表或开发框架。

### 4.2 首页布局

```text
Sidebar 304px
Main Canvas
  Window controls top-right
  Center stack
    Prompt title
    Composer
      text input
      attachment / add button
      access control
      model selector
      thinking slider shortcut
      send button
    Choose project row
```

中央 stack 宽度：

```text
默认：min(912px, calc(100vw - sidebar - 240px))
最小：640px
窄屏：calc(100vw - 32px)
```

标题样式：

```text
font-size: 30px
font-weight: 500
color: #1D1D1F
margin-bottom: 36px
```

composer 样式：

```text
height: 124px
border-radius: 22px
background: #FFFFFF
border: 1px solid #E3E3E7
box-shadow: 0 12px 36px rgb(0 0 0 / 8%)
```

composer 下方项目行：

```text
height: 52px
border-radius: 0 0 22px 22px
background: #F4F4F5
```

### 4.3 首页交互

1. 输入任务后，发送按钮从灰色变为品牌蓝。
2. 未选择项目时，发送会引导用户先选择项目。
3. `Choose project` 打开项目选择 popover，显示最近项目、工作区路径和新增项目入口。
4. 模型选择显示为 `模型名 + 思考强度`，例如 `5.5 超高`。
5. 思考强度可在 composer 中快速切换，也可进入设置详细配置。

## 5. 左侧侧栏

### 5.1 内容顺序

侧栏不展示插件入口，改为技能入口：

```text
新对话
搜索
已安排
技能

项目
  D:
  codemax
    设计启动动画
    制定三人并行方案
    梳理可并行编写项
    确认D任务归属
    评估编程智能体差距
    展开显示
  LYC
  C:
  面试题
  Blog

对话
  你好
  个人博客网站需要后端吗
  分析自我介绍不足

设置 / 账户
```

### 5.2 项目与对话规则

1. 项目区展示本地工作区域。
2. 每个项目下展示最近任务线程。
3. 对话区展示不绑定项目或历史通用对话。
4. 时间显示使用短格式：`6 分`、`14 小时`、`3 天`、`1 周`。
5. 当前选中项使用浅灰背景和左侧细选中条，不使用强色块。

### 5.3 技能入口

`技能` 替代原来的 `插件`。点击后进入 Skills 页面，展示：

1. 当前工作区 `.codemax/skills`
2. 当前项目 `.codemax/skills`
3. 用户全局 `.codemax/skills`
4. 内置系统技能

技能来源优先级：

```text
workspace > project > user global > built-in
```

如果同名技能冲突，列表中显示覆盖关系和实际生效来源。

## 6. 搜索设计

搜索只搜对话名字和任务线程标题，不搜全文日志，不搜代码，不搜 Proof Pack。

搜索入口点击后打开居中 command palette：

```text
Search conversations
  input
  results grouped by project
```

结果项显示：

1. 对话标题
2. 所属项目
3. 更新时间
4. 当前状态

空状态：

```text
未找到对话
换一个名字试试
```

英文：

```text
No conversations found
Try another title
```

## 7. 任务线程页

从首页发送后进入任务线程页。线程页仍然保持 Codex 桌面版风格，不变成 dashboard。

布局：

```text
Main Thread
  task title
  user prompt
  agent timeline
  command output collapsibles
  diff preview collapsible
  validation result
  follow-up composer
Inspector Drawer
  run contract
  model & thinking
  permissions
  storage
  privacy
  proof pack
```

Inspector 默认折叠，使用右上角按钮展开。用户需要审计、存储、隐私、运行契约时再看。

## 8. 技能页

技能页是插件页的替代品。目标是让用户知道 CodeMax 当前能调用哪些技能，以及它们从哪里来。

### 8.1 技能页布局

```text
Skills Page
  header
    title: 技能
    subtitle: 工作区和全局 .codemax 技能
  source tabs
    当前项目
    工作区
    全局
    内置
  search/filter
  skill list
```

### 8.2 技能卡内容

每个技能项显示：

1. 名称
2. 简短描述
3. 来源路径
4. 启用状态
5. 最近使用时间
6. 冲突或覆盖提示

技能项保持列表样式，不使用大卡片瀑布流。

## 9. 设置页总览

设置页必须像 Codex 桌面版设置：左侧分类，右侧详情，留白充足，行式配置，避免拥挤。

分类顺序：

```text
账户
模型
思考强度
权限
技能
存储
记忆
外观
语言
启动
关于
```

设置页布局：

```text
Settings
  Sidebar 320px
  Detail max-width 860px
```

右侧每个设置组使用：

```text
Section title
Section description
Grouped rows
```

## 10. 模型与思考强度

### 10.1 概念区分

模型设置负责选择模型提供商、Base URL、API Key、模型名和连接测试。

思考强度负责控制同一任务下 Agent 的推理深度、上下文预算、验证积极性和自动修复耐心。

### 10.2 思考强度等级

提供 5 个等级：

```text
极低
低
中
高
超高
```

英文：

```text
Minimal
Low
Medium
High
Max
```

### 10.3 拖动式控件

思考强度必须使用拖动式 slider，不使用普通下拉框。

控件结构：

```text
Thinking Strength
  slider with 5 snap points
  animated thumb
  selected level
  explanation panel
  impact meters
```

拖动时：

1. thumb 贴附到 5 个刻度点。
2. 说明面板跟随切换。
3. 动效随强度变化。
4. 保存前可预览变化。

### 10.4 每档说明

极低：

```text
低在哪儿：只做最少推理，优先快速回答和小改动。
好处：最快、最省 token、适合明确简单任务。
代价：复杂任务容易遗漏边界，验证建议较少。
适合：改文案、简单配置、问答、轻量说明。
```

低：

```text
低在哪儿：会做基础分析，但不会大范围探索。
好处：速度快，适合日常小修和单文件任务。
代价：跨模块影响判断较保守，可能需要用户补充指令。
适合：小 bug、样式微调、简单脚本。
```

中：

```text
中在哪儿：平衡速度、上下文和验证，是默认推荐。
好处：适合大多数开发任务，成本和质量平衡。
代价：极复杂架构问题可能需要切到高或超高。
适合：常规功能、设置页、接口联调、普通修复。
```

高：

```text
高在哪儿：会读取更多上下文，主动分析风险和测试路径。
好处：更稳，适合多文件、多模块、交付前修改。
代价：更慢，token 和命令执行成本更高。
适合：复杂 bug、跨端联调、重构、交付审查。
```

超高：

```text
高在哪儿：最大化规划、审计、验证和自我检查。
好处：质量最稳，适合关键上线、隐私、安全和交付证据。
代价：最慢、资源占用最高，不适合简单任务。
适合：上线前验收、架构变更、隐私审计、最终交付。
```

### 10.5 影响指标

每档展示具体影响：

```text
推理深度
上下文预算
验证强度
自动修复轮次
预计速度
预计成本
```

显示方式为 6 条横向 meter，不用抽象文案糊弄用户呀。

示例：

```text
低
推理深度 2/5
上下文预算 2/5
验证强度 2/5
自动修复 1/5
速度 5/5
成本 1/5
```

## 11. 思考强度动效

每个思考强度对应不同动效，动效要克制且可关闭。

```text
极低：thumb 轻快滑动，轨道只有单点亮起。
低：thumb 短滑，轨道出现两段柔和亮光。
中：thumb 平滑弹性，轨道中段亮起，说明面板淡入。
高：thumb 带轻微惯性，轨道出现连续扫描线。
超高：thumb 稳重推进，轨道出现蓝青渐变脉冲，说明面板有轻微层级展开。
```

减少动态效果时：

1. slider 仍可拖动。
2. 不使用扫描、脉冲、弹性动效。
3. 只做即时状态切换。

## 12. Composer 中的强度入口

首页 composer 右侧显示：

```text
模型名  思考强度
```

例如：

```text
5.5 超高
```

点击后打开小 popover：

1. 当前模型
2. 当前思考强度 slider
3. 当前档位一句话说明
4. 进入设置详情

这和设置页复用同一套强度定义，不能出现首页和设置页含义不一致。

## 13. 启动页与启动动画

启动动画文件由用户制作完成后放入：

```text
apps/desktop/src/assets/splash/codemax-launch.webm
apps/desktop/src/assets/splash/codemax-launch.mp4
apps/desktop/src/assets/splash/codemax-poster.png
```

静态降级使用：

```text
D:\codemax\ico\CodeMax.png
```

启动页顺序：

```text
0.0s - 0.2s  Mac Minimal 背景
0.2s - 2.8s  CodeMax 启动动画
2.8s - 3.5s  启动自检
3.5s+        首页淡入
```

启动自检只显示真实状态：

```text
检查 Agent
检查存储
检查模型配置
```

动画缺失、加载失败或减少动态效果开启时，直接显示静态图标。

## 14. 视觉系统

默认主题：

```text
theme id: macMinimal
display: Mac Minimal
```

颜色：

```text
app background: #F7F7F8
sidebar: #EFEFF1
sidebar border: #DCDCE0
surface: #FFFFFF
composer surface: #FFFFFF
composer tray: #F4F4F5
text: #1D1D1F
text secondary: #6E6E73
text faint: #9A9AA2
border: #E3E3E7
brand blue: #0A84FF
brand cyan: #18D5F9
access orange: #FF5A1F
success: #30D158
warning: #FF9F0A
danger: #FF453A
```

圆角：

```text
sidebar item: 8px
composer: 22px
popover: 14px
settings group: 12px
dialog: 16px
```

阴影：

```text
composer: 0 12px 36px rgb(0 0 0 / 8%)
popover: 0 18px 54px rgb(0 0 0 / 14%)
dialog: 0 24px 80px rgb(0 0 0 / 18%)
```

字体：

```text
-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif
```

## 15. 国际化

新增文案进入：

```text
apps/desktop/src/i18n/locales/zh-CN.json
apps/desktop/src/i18n/locales/en-US.json
```

建议 key：

```text
home.promptTitle
home.placeholder
home.chooseProject
sidebar.skills
search.title
search.placeholder
search.emptyTitle
search.emptyHint
skills.title
skills.subtitle
skills.source.workspace
skills.source.project
skills.source.global
skills.source.builtIn
settings.thinking.title
settings.thinking.subtitle
settings.thinking.minimal
settings.thinking.low
settings.thinking.medium
settings.thinking.high
settings.thinking.max
settings.thinking.benefit
settings.thinking.tradeoff
settings.thinking.bestFor
settings.thinking.depth
settings.thinking.contextBudget
settings.thinking.validation
settings.thinking.repair
settings.thinking.speed
settings.thinking.cost
splash.title
splash.subtitle
splash.check.agent
splash.check.storage
splash.check.model
```

## 16. 数据与文件来源

### 16.1 技能发现

技能来源路径：

```text
<workspace>/.codemax/skills
<project>/.codemax/skills
%USERPROFILE%/.codemax/skills
built-in skills
```

展示字段：

```text
id
name
description
source
path
enabled
lastUsedAt
overrides
```

### 16.2 搜索数据

搜索只读取：

```text
conversation_id
title
project_name
updated_at
status
```

不读取对话正文、命令日志或代码内容。

### 16.3 思考强度数据

建议结构：

```text
thinking_strength:
  level: minimal | low | medium | high | max
  reasoning_depth: 1-5
  context_budget: 1-5
  validation_strength: 1-5
  repair_rounds: 0-5
  speed_bias: 1-5
  cost_level: 1-5
```

## 17. 性能与可访问性

1. 首页默认不加载重图表、不拉取大日志、不渲染 Diff。
2. 启动动画建议小于 8 MB。
3. 技能扫描要懒加载，先显示来源，再异步读取描述。
4. 搜索只搜标题，保证即时响应。
5. slider 支持键盘左右键切换。
6. 强度动效遵守 `prefers-reduced-motion`。
7. 所有图标按钮有 `aria-label`。
8. 状态不能只靠颜色表达。

## 18. 实施顺序

1. 更新 UI 验收脚本，锁定首页 composer、技能入口、对话标题搜索和思考强度 slider。
2. 将侧栏插件入口替换为技能入口。
3. 首页改为 Codex-like 新对话 composer，不展示内部框架。
4. 搜索改为只搜对话名字和任务标题。
5. 新增 Skills 页面与技能来源展示。
6. 设置页新增思考强度详情页和拖动式 slider。
7. Composer 接入模型与思考强度快捷 popover。
8. 启动页接入动画资源路径和静态降级。
9. 更新中英文 i18n。
10. 运行前端检查、桌面构建和视觉验收。

## 19. 验收标准

1. 打开应用后看到的是 Codex-like 首页，不是内部框架首页。
2. 首页中央有“我们该做什么？”和大 composer。
3. 左侧显示技能入口，不显示插件入口。
4. 技能页能展示工作区、项目和全局 `.codemax/skills`。
5. 搜索只匹配对话名字或任务线程标题。
6. composer 中能看到模型和思考强度，例如 `5.5 超高`。
7. 设置页有独立思考强度页，使用拖动式 slider。
8. 每个思考强度展示具体变化、好处、代价和适用场景。
9. 每个强度有对应动效，减少动态效果时关闭动画。
10. 启动动画资源存在时播放，不存在时使用 `CodeMax.png`。
11. 所有新增文案支持中英文。
12. 日志、Diff、Proof Pack 等内部细节不出现在首页默认态。

## 20. 设计自检

1. 设计覆盖启动、首页、任务、搜索、技能、设置、存储和模型强度。
2. 首页符合用户给出的 Codex 桌面版参考图，不展示内部框架。
3. 插件概念已替换为技能，并明确 `.codemax/skills` 来源。
4. 思考强度不只是标签，包含指标、说明、好处、代价和动效。
5. 本设计仍聚焦 UI 与体验，不改变 Agent 后端执行链路。
