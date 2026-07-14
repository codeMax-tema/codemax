# Runtime 与发布验证链修复设计

**日期：** 2026-07-14
**范围：** D 线发布阻塞修复（Agent runtime 打包、S11 Python 解释器选择、验证链可复跑性）

## 1. 问题与目标

当前发布验证链存在两个环境耦合问题：

1. `scripts/build-agent-runtime.mjs` 固定调用 PATH 中的 `python`。当前解析到的解释器缺少 `PyInstaller`，导致 `output/runtime/agent/codemax-agent.exe` 无法生成，并进一步阻断 Tauri 的资源编译、Rust 测试和安装包 smoke。
2. `scripts/check-s11.mjs` 固定调用 PATH 中的 `python`，没有使用项目 Agent 的虚拟环境，导致即使 `agent/.venv` 中具备 pytest，S11 验收脚本仍以 `ModuleNotFoundError: pytest` 失败。

本批次的目标是消除这两个解释器选择的隐式环境依赖，使发布验证先能够抵达真实的 runtime、Rust/Tauri 和 S11 测试阶段。

## 2. 非目标

- 不实现 REL-P0-006 至 REL-P0-009 的业务能力。
- 不修改 Agent 默认工作流或 UI。
- 不在构建脚本中自动联网安装任意 Python 包。
- 不把 `output/runtime`、构建输出或虚拟环境提交到 Git。

## 3. 方案选择

### 方案 A（采用）：显式、可覆盖的项目 Python 解析器

新增共享的 Node 构建辅助模块，按下列优先级解析 Python：

1. `CODEMAX_AGENT_PYTHON`：用户或 CI 显式指定的解释器绝对路径；
2. `agent/.venv/Scripts/python.exe`（Windows）或 `agent/.venv/bin/python`（Unix）；
3. PATH 中的 `python` / `python3`，仅作为最后兜底。

runtime 打包和 S11 检查都使用同一解析规则，确保 Python 依赖和 Agent 代码位于同一个可控环境。

对于 runtime 打包，脚本先执行 `python -c "import PyInstaller"`：

- 可导入：继续调用 `python -m PyInstaller`；
- 不可导入：失败诊断只记录解析来源（`environment`、`project_venv` 或 `path`）与明确修复命令，绝不输出解释器绝对路径；
- 不自动安装，以避免发布构建隐式修改开发环境和产生不可审计网络副作用。

### 方案 B：保留 PATH Python，仅更新开发文档

修改最少，但无法避免 CI、开发机、安装环境的 PATH 差异，不采用。

### 方案 C：将 PyInstaller 固定纳入 Agent 运行时依赖

会扩大正式 Agent 运行时依赖并改变安装语义。PyInstaller 只属于打包工具，不应进入产品运行依赖，本批次不采用。

## 4. 代码结构

新增：

```text
scripts/lib/agent-python.mjs
```

导出：

- `resolveAgentPython({ root, env, platform })`：返回已验证存在的解释器路径与来源；
- `agentPythonGuidance(...)`：生成不包含任何密钥的故障修复提示；
- `runWithAgentPython(...)`（如有必要）：统一子进程调用参数。

修改：

```text
scripts/build-agent-runtime.mjs
scripts/check-s11.mjs
```

新增 Node 测试：

```text
tests/scripts/verify-agent-python-resolution.mjs
```

覆盖：

- 环境变量覆盖优先级；
- 项目 venv 优先于 PATH；
- Windows/Unix 路径分支；
- 缺少解释器时的稳定错误；
- runtime 与 S11 脚本复用同一解析器约定。

## 5. 用户与发布安全性

- 不写入 API Key、模型配置或用户数据；
- 不自动下载/安装 Python 包；
- runtime 输出继续仅落在被 `.gitignore` 忽略的 `output/`；
- 在 `runtimeTarget` 计算完成后、`resolveAgentPython({ root })` 和 PyInstaller 模块预检之前，立即执行 `rmSync(runtimeTarget, { force: true })`；因此解释器解析、预检或构建失败时都不会保留可被后续 Tauri 检查误用的旧 runtime；
- 只有 PyInstaller 构建成功、产物存在且通过 `existsSync(source)` 验证后，才允许 `cpSync(source, runtimeTarget)` 写回最终 runtime；
- 失败日志仅显示解析来源、缺少的工具模块和显式修复步骤；绝不记录解释器绝对路径；
- 通过 `CODEMAX_AGENT_PYTHON` 支持 CI/Windows 主环境显式控制解释器，降低磁盘和环境不透明问题。

## 6. 验收步骤

先验证新增 Node 测试，再执行：

```powershell
npm run build:agent-runtime
npm run check:tauri
npm run check:s11
npm run check:d-line-release
```

预期：

1. 若项目 venv 含 PyInstaller，runtime 写入 `output/runtime/agent/codemax-agent.exe`；
2. 若不含 PyInstaller，构建失败信息只记录所选解释器的 source 和安装命令，不泄露解释器绝对路径；
3. S11 不再因系统 Python 缺少 pytest 而失败；
4. 后续 Rust/Tauri 失败（如有）应来自真实编译或测试，而不是缺失 runtime 资源。

## 7. 风险与回滚

- 若 Python 3.14 的 PyInstaller 不兼容，使用 `CODEMAX_AGENT_PYTHON` 显式指向兼容的 Python 3.11 构建环境；
- 所有改动集中在脚本层，可通过回退新增 helper 及两个调用点恢复原行为；
- 不修改数据库、任务状态、Agent API 或安装器配置。
