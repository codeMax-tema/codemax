# CodeMax Mac Codex Minimal UI 设计

## 1. 背景

当前 CodeMax 桌面端已经具备 Codex-like UI 的基础结构，但用户反馈界面显得杂乱，不够像 Codex 桌面版，也缺少 Mac 原生应用的清爽质感。本设计选择方案 A：Mac Codex Minimal，作为默认 UI 方向。

本次设计属于 D 线范围，聚焦桌面产品体验、设置、存储透明、国际化、品牌资源和启动体验。启动动画由用户在外部制作，完成后放入项目文件包，本设计定义接入位置与展示规则。

## 2. 目标

1. 默认 UI 呈现神似 Codex 桌面版的任务工作台，而不是传统后台仪表盘。
2. 使用 Mac 风格的浅色、磨砂侧栏、克制边框和轻阴影，减少视觉噪声。
3. 保持 CodeMax 的核心差异可见：本地、隐私、运行契约、存储透明、可审计交付。
4. 启动页接入 CodeMax 品牌动画，启动过程中显示真实自检状态。
5. 所有新增可见文案进入中英文 i18n 字典。
6. UI 默认简约，并保留后续切换暗色、高对比和局部 Liquid Glass 的空间。

## 3. 非目标

1. 不重写任务、仓库、审批、设置的数据模型。
2. 不改变 Agent 执行链路、Tauri 命令或后端接口语义。
3. 不把整页做成大面积玻璃拟态或营销页。
4. 不在长日志、Diff、代码输出区域使用重模糊或强装饰效果。
5. 不依赖启动动画完成才能进入主界面；动画缺失时必须有静态降级。

## 4. 信息架构

主应用采用三栏工作台结构：

1. 左侧侧栏：任务入口、搜索、任务列表、仓库、审批、设置。
2. 中间主工作区：当前任务线程、执行状态、日志、Diff、验证结果和后续输入。
3. 右侧上下文面板：运行契约、模型、权限、存储占用、隐私摘要、交付证据入口。

右侧上下文面板默认在宽屏展示，在中等宽度下折叠为按钮，在窄屏下进入抽屉。这样用户先看到任务本身，需要细节时再展开上下文哦。

## 5. 布局规格

### 5.1 应用外壳

根布局使用完整桌面窗口模型：

```text
Window
  Top chrome
  App body
    Sidebar
    Main workspace
    Context inspector
```

顶部 chrome 保持轻量，只放返回、前进、侧栏开关、当前仓库和窗口控制。不要在顶部堆积业务按钮。

### 5.2 左侧侧栏

宽度建议：

```text
默认：304px
紧凑：84px
```

侧栏内容顺序：

1. 新任务按钮
2. 搜索
3. 当前仓库
4. 任务状态筛选
5. 最近任务线程
6. 审批与设置入口
7. 账户或本机状态

任务列表每项只显示标题、状态点和更新时间。详细信息移到主区或右侧面板，避免侧栏过载。

### 5.3 主工作区

任务页改为线程式体验：

1. 顶部显示任务标题、仓库、分支和当前状态。
2. 中间按时间线展示 Agent 阶段：规划、编辑、验证、修复、等待审批、交付。
3. 日志、命令输出、Diff 使用可折叠块展示。
4. 底部固定 follow-up composer，用于继续指令、补充验证或发起审批。

主工作区不使用多个同级大卡片堆叠。优先使用分割线、轻背景和折叠区组织信息。

### 5.4 右侧上下文面板

宽度建议：

```text
默认：336px
最大：380px
```

模块顺序：

1. Run Contract
2. Model & Mode
3. Permissions
4. Storage
5. Privacy
6. Proof Pack

每个模块使用紧凑标题、状态和一行摘要。高级内容进入详情或设置页。

## 6. 视觉系统

默认主题命名为 `macMinimal`，显示名称为 `Mac Minimal`。

颜色建议：

```text
app background: #F5F5F7
sidebar: #ECECEF
surface: #FFFFFF
surface raised: #FAFAFB
text: #1D1D1F
text secondary: #6E6E73
border: #DCDCE0
brand blue: #0A84FF
brand cyan: #18D5F9
success: #30D158
warning: #FF9F0A
danger: #FF453A
```

圆角：

```text
按钮：8px
输入框：10px
弹窗：12px
面板：10px
```

阴影只用于弹窗、抽屉和悬浮菜单：

```text
0 18px 60px rgb(0 0 0 / 16%)
```

字体使用系统字体栈：

```text
-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", sans-serif
```

字号：

```text
主标题：20-24px
页面标题：18-20px
正文：14px
辅助文字：12-13px
代码与路径：12-13px monospace
```

## 7. 启动页与启动动画

启动动画文件由用户制作后放入项目文件包。推荐路径：

```text
apps/desktop/src/assets/splash/codemax-launch.webm
apps/desktop/src/assets/splash/codemax-launch.mp4
apps/desktop/src/assets/splash/codemax-poster.png
```

静态降级资源：

```text
D:\codemax\ico\CodeMax.png
```

启动流程：

```text
0.0s - 0.2s  显示 Mac Minimal 背景
0.2s - 2.8s  播放 CodeMax 启动动画
2.8s - 3.5s  显示启动自检状态
3.5s+        淡入主工作台
```

启动页只显示必要信息：

```text
CodeMax
正在准备本地智能工作台...
检查 Agent
检查存储
检查模型配置
```

英文：

```text
CodeMax
Preparing your local agent workspace...
Checking Agent
Checking Storage
Checking Model Settings
```

如果动画缺失、加载失败或用户启用减少动态效果，则显示 `CodeMax.png` 静态图标，并直接进入自检状态。

## 8. 关键页面设计

### 8.1 新任务弹窗

新任务弹窗采用 Codex composer 体验：

1. 左侧是大输入框，用户描述任务。
2. 右侧是运行契约摘要：模式、模型、权限、验证命令、存储位置。
3. 底部是发送按钮、计划模式、审核模式和设置入口。

弹窗必须保持视觉安静，不堆满说明文字。

### 8.2 设置页

设置页采用 Mac Settings 风格：

1. 左侧分类 rail。
2. 右侧是单一分类详情。
3. 表单项按行排列，开关使用 toggle，选项使用 segmented control 或 select。

分类：

1. Models
2. Modes
3. Permissions
4. Storage
5. Memory
6. Appearance
7. Language
8. Startup

### 8.3 存储页

存储页必须清楚显示：

1. 数据库路径
2. worktree 路径
3. 日志占用
4. 截图占用
5. 临时上下文占用
6. 可清理内容
7. 不可清理永久证据

清理按钮必须解释影响范围，不允许让用户误删 Proof Pack 或永久证据。

## 9. 国际化

新增文案必须加入：

```text
apps/desktop/src/i18n/locales/zh-CN.json
apps/desktop/src/i18n/locales/en-US.json
```

新增 key 建议：

```text
splash.title
splash.subtitle
splash.check.agent
splash.check.storage
splash.check.model
splash.check.ready
settings.appearance.macMinimal
settings.startup.title
settings.startup.animation
settings.startup.reducedMotion
contextInspector.title
contextInspector.runContract
contextInspector.storage
contextInspector.privacy
```

## 10. 性能与可访问性

1. 启动动画优先使用压缩后的视频资源，避免引入过大的 Lottie 或逐帧图片。
2. 动画文件建议小于 8 MB。
3. 支持 `prefers-reduced-motion: reduce`。
4. 启动动画不可阻塞真实启动自检。
5. 日志、Diff、长列表必须保持普通背景和高可读文本。
6. 所有图标按钮必须有 `aria-label`。
7. 所有状态不能只靠颜色表达。

## 11. 数据流

启动页数据流：

```text
App mount
  -> load splash assets
  -> run startup health check
  -> show health status
  -> mark app ready
  -> fade into workspace
```

主题数据流：

```text
SettingsPage
  -> setTheme("macMinimal")
  -> Zustand appStore.theme
  -> App root class theme-macMinimal
  -> global.css variables
```

右侧上下文面板数据流：

```text
selectedTaskId
  -> task detail
  -> run contract summary
  -> storage summary
  -> privacy summary
  -> proof pack status
```

## 12. 实施顺序

1. 新增 UI 设计验收脚本，锁定主题、启动页、三栏结构和 i18n key。
2. 引入 `macMinimal` 主题变量，保持默认主题切到 `macMinimal`。
3. 调整 App shell 为左侧栏、中间主区、右侧上下文面板。
4. 新增 SplashScreen 组件和启动自检展示。
5. 接入启动动画资源路径和静态降级。
6. 整理任务页为线程式布局。
7. 整理设置页为 Mac Settings 风格。
8. 更新中英文 i18n。
9. 运行前端检查、桌面构建和视觉验收。

## 13. 验收标准

1. 默认打开应用时，整体观感接近 Codex 桌面版，并带 Mac 浅色应用质感。
2. 主界面不再是杂乱 dashboard，而是任务线程式工作台。
3. 左侧侧栏、主区、右侧上下文面板职责清晰。
4. 启动动画存在时正常播放，缺失时使用 `CodeMax.png` 降级。
5. 启动页显示 Agent、存储、模型配置的真实自检状态。
6. 新增文案全部支持中英文。
7. 存储路径与占用在设置页透明可见。
8. 日志、Diff、代码输出保持清晰可读。
9. 减少动态效果模式下不播放启动动画。
10. 前端检查和桌面构建通过。

## 14. 设计自检

1. 无未决项或空白内容。
2. 方案聚焦 D 线 UI、启动页、设置和存储体验，没有改变后端任务链路。
3. Mac Minimal 是默认体验，Liquid Glass 仅作为未来可选增强，不影响本次范围。
4. 启动动画缺失、加载失败、减少动态效果三种情况都有降级路径。
5. 信息层级从任务优先出发，减少侧栏和页面内卡片堆叠，符合用户“不要乱”的反馈。
