# Desktop App

桌面前端和 Rust/Tauri 本地后端放在同一个应用包里：

- `src/`：React 18 + TypeScript + Vite 前端。
- `src-tauri/`：Rust + Tauri Commands 本地后端。
- `src/i18n/`：中文和英文资源，所有用户可见文本使用 key 管理。
- `src/api/`：前端 IPC client，页面不直接散落 Tauri invoke。
- `src/state/`：轻量 UI 状态。
- `src/features/`：按业务功能聚合页面、组件和 hooks。

S1 之后可以在本目录安装依赖并启动 Tauri 开发环境。当前只搭骨架，不生成大体积依赖目录。
