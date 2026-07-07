# Liquid Glass 可切换主题设计

## 1. 背景

Codemax 桌面端当前采用 Tauri v2 + React + TypeScript + Vite 架构，默认 UI 风格为简约工作台。用户确认希望升级 React 19，并将 `liquid-glass-react` 作为一个可切换的 UI 风格接入，而不是替换默认体验。

本设计选择视觉预览中的 B 方案：局部 Liquid Glass。目标是在保持工程工作台清晰、可读、低干扰的前提下，为关键浮层和输入控件增加更有质感的玻璃效果。

## 2. 目标

1. 将桌面端 React 依赖升级到 React 19，满足 `liquid-glass-react` 的 peer dependency。
2. 新增 `glass` 主题选项，默认主题仍为 `minimal`。
3. 将 Liquid Glass 限定在局部高价值界面区域，避免大面积折射影响阅读和性能。
4. 保持现有国际化结构，新增中英文文案。
5. 为主题、依赖和关键接入点补充前端契约验证。

## 3. 非目标

1. 不重写整体应用布局。
2. 不把日志、命令输出、Diff 预览、长列表做成玻璃效果。
3. 不引入新的设计系统或替换 shadcn/Radix/Tailwind 基础组件。
4. 不在本次实现桌面安装包发布。
5. 不改变存储路径、任务数据模型、Agent 执行链路或 Tauri Rust 命令。

## 4. 用户体验设计

设置页的“界面风格”分段控件新增 `Liquid Glass` 选项。用户切换到该主题后，应用根节点继续通过 `theme-glass` class 暴露主题状态。

默认 `minimal`、`dark`、`highContrast` 行为保持不变。`glass` 只改变局部视觉表现，不改变交互路径、按钮位置、文案含义或键盘可达性。

第一批玻璃化区域：

1. 新任务弹窗 `codex-composer-dialog`
2. 新任务弹窗内部的发送按钮 `codex-send-button`
3. 任务详情页底部继续输入栏 `execution-followup-composer`
4. 任务详情页右侧环境卡片 `environment-card`

明确保持普通样式的区域：

1. 命令输出 `command-output-block`
2. 代码变更面板 `code-change-panel`
3. Diff 预览 `execution-code-diff-preview`
4. 侧边栏任务列表、设置页主体和长文本区域

## 5. 架构设计

### 5.1 依赖升级

`apps/desktop/package.json` 升级：

1. `react` -> React 19
2. `react-dom` -> React 19
3. 新增 `liquid-glass-react`

根 `package-lock.json` 通过 `npm install` 更新，不手写锁文件。

### 5.2 主题状态

`apps/desktop/src/state/appStore.ts` 中 `ThemeName` 新增 `glass`：

```ts
export type ThemeName = 'minimal' | 'dark' | 'highContrast' | 'glass';
```

默认值保持：

```ts
theme: 'minimal'
```

`App.tsx` 已使用 `theme-${theme}` 生成主题 class，因此新增主题不需要改变根节点数据流。

### 5.3 GlassSurface 组件

新增本地包装组件，建议路径：

```text
apps/desktop/src/components/ui/GlassSurface.tsx
```

职责：

1. 读取当前 `theme`。
2. 当 `theme !== 'glass'` 时，返回普通容器，尽量保持 DOM 结构稳定。
3. 当 `theme === 'glass'` 时，使用 `LiquidGlass` 包裹子元素。
4. 暴露少量受控 props，避免每个调用点散落复杂参数。

建议接口：

```ts
type GlassSurfaceVariant = 'dialog' | 'composer' | 'panel' | 'button';

interface GlassSurfaceProps {
  as?: 'div' | 'section' | 'article' | 'button';
  variant: GlassSurfaceVariant;
  className?: string;
  children: React.ReactNode;
  onClick?: () => void;
}
```

各 variant 使用固定参数，便于性能和视觉统一：

1. `dialog`：低弹性、中等 blur，适合新任务弹窗。
2. `composer`：中等弹性、圆角较大，适合底部输入栏。
3. `panel`：低弹性、较弱折射，适合环境卡片。
4. `button`：较高圆角、轻微弹性，适合发送按钮。

### 5.4 样式层

`global.css` 新增 `theme-glass` 变量和局部 class：

1. 玻璃主题背景保持浅色工作台，避免整页变成深色或高饱和。
2. 使用更柔和的边框、阴影和 backdrop fallback。
3. 为非 glass 主题保留现有 CSS。
4. 加入 `@media (prefers-reduced-motion: reduce)` 降低弹性和过渡。

CSS fallback 用于两类情况：

1. `liquid-glass-react` 在 WebView2 中表现不稳定时，基础半透明样式仍可见。
2. 非 glass 主题下组件不加载重视觉效果。

## 6. 数据流

```text
SettingsPage
  -> setTheme('glass')
  -> Zustand appStore.theme
  -> App root className: theme-glass
  -> GlassSurface reads theme
  -> theme === glass ? LiquidGlass : plain container
```

主题状态仍为前端内存状态。现有项目尚未实现设置持久化，本次不新增持久化层，避免越界改动。

## 7. 错误处理与降级

1. 如果 Liquid Glass 组件渲染失败，应避免阻断任务创建和查看。包装组件保持简单，不在调用点加入业务逻辑。
2. 非 glass 主题完全不依赖 Liquid Glass 参数效果。
3. 玻璃主题仍保留 CSS fallback，确保 WebView2 或 GPU 表现不佳时界面可读。
4. 高对比模式与 `glass` 不叠加为新主题组合；如果用户打开增强对比，CSS 应提高文字和边框对比，不继续强化折射。

## 8. 国际化

新增 key：

```text
settings.appearance.glass
```

中文：`Liquid Glass`

英文：`Liquid Glass`

命名保持品牌/效果名一致，不翻译成“液态玻璃”，减少设置项长度和歧义。

## 9. 测试设计

更新 `tests/frontend/verify-s6-ui.mjs`，验证：

1. `package.json` 包含 React 19 和 `liquid-glass-react`。
2. `ThemeName` 包含 `glass`，默认仍是 `minimal`。
3. `SettingsPage.tsx` 包含 `setTheme('glass')` 和对应 i18n key。
4. `GlassSurface` 文件存在并导入 `liquid-glass-react`。
5. `NewTaskDialog.tsx` 和 `TaskOverviewPage.tsx` 使用 `GlassSurface`。
6. `zh-CN.json` 和 `en-US.json` 都包含 `settings.appearance.glass`。
7. `global.css` 包含 `.theme-glass` 和 reduced-motion 降级样式。

实现完成后运行：

```bash
npm install
npm run check:frontend
npm run build:desktop
```

若视觉效果需要验收，再运行：

```bash
npm run dev:desktop
```

并在浏览器或 Tauri dev 窗口中检查主题切换、弹窗、环境卡片和底部输入栏。

## 10. 风险与控制

| 风险 | 控制 |
| --- | --- |
| React 19 升级导致第三方组件兼容问题 | 先运行 TypeScript build 和前端契约测试，失败时定位到具体包 |
| Liquid Glass 增加 GPU 压力 | 仅局部启用，不覆盖日志、Diff 和长列表 |
| 玻璃效果影响可读性 | 文本区域不玻璃化，玻璃容器保留清晰 foreground 和边框 |
| WebView2 渲染表现和 Chrome 不一致 | 保留 CSS fallback，必要时用 Tauri dev 做可视检查 |
| 主题扩展破坏默认体验 | 默认 `minimal` 不变，glass 通过设置页显式切换 |

## 11. 验收标准

1. 项目可以安装依赖，不再出现 `react >=19` peer dependency 冲突。
2. 设置页可以选择 `Liquid Glass` 风格。
3. 默认打开应用仍是简约风格。
4. 切换 glass 后，新任务弹窗、底部输入栏、环境卡片和发送按钮出现玻璃效果。
5. 命令输出、Diff 预览和长文本区域仍保持清晰普通样式。
6. 中英文语言包都包含新增主题文案。
7. `npm run check:frontend` 和 `npm run build:desktop` 通过。

## 12. 实施顺序

1. 更新前端契约测试，先让测试因缺少 glass 主题和依赖而失败。
2. 升级 React 19 并安装 `liquid-glass-react`。
3. 扩展 `ThemeName` 和设置页主题选项。
4. 新增 `GlassSurface` 包装组件。
5. 将包装组件接入弹窗、底部输入栏、环境卡片和发送按钮。
6. 添加 `.theme-glass` 样式和 reduced-motion 降级。
7. 更新中英文 i18n。
8. 运行验证命令并记录结果。
