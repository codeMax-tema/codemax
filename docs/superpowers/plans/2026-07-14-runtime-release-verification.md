# Runtime Release Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make runtime packaging and S11 acceptance checks use a deterministic Agent Python interpreter so Tauri/Rust release validation reaches real build and test failures.

**Architecture:** Add a small Node helper that selects an explicitly configured Python, then the project Agent virtual environment, then PATH as a last fallback. Runtime packaging and S11 will import this helper. Runtime packaging first deletes its final target, then resolves/preflights Python; it copies a runtime back only after a successful build. PyInstaller remains a build-only development dependency; scripts validate its presence and never install packages implicitly. Diagnostics record only the selected source, never an interpreter absolute path.

**Tech Stack:** Node.js ESM, npm workspaces, Python 3.11+, uv, PyInstaller, pytest, Rust/Tauri.

---

## File map

| File | Responsibility |
| --- | --- |
| `scripts/lib/agent-python.mjs` | Resolve Agent Python deterministically and validate PyInstaller availability. |
| `tests/scripts/verify-agent-python-resolution.mjs` | Node assertions for precedence, platform paths and failure diagnostics. |
| `scripts/build-agent-runtime.mjs` | Invoke PyInstaller through the resolved Agent Python. |
| `scripts/check-s11.mjs` | Invoke S11 pytest through the resolved Agent Python. |
| `agent/pyproject.toml` | Declare PyInstaller as a development-only build dependency. |
| `agent/uv.lock` | Lock the declared development build dependency. |

### Task 1: Add a red Node contract test for interpreter resolution

**Files:**
- Create: `tests/scripts/verify-agent-python-resolution.mjs`
- Create: `scripts/lib/agent-python.mjs`

- [ ] **Step 1: Write the failing test**

```js
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { resolveAgentPython } from '../../scripts/lib/agent-python.mjs';

const root = mkdtempSync(path.join(os.tmpdir(), 'codemax-agent-python-'));
const windowsPython = path.join(root, 'agent', '.venv', 'Scripts', 'python.exe');
const unixPython = path.join(root, 'agent', '.venv', 'bin', 'python');
const overridePython = path.join(root, 'tools', 'python.exe');

function file(filePath) {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, '');
}

try {
  file(windowsPython);
  file(unixPython);
  file(overridePython);

  assert.deepEqual(
    resolveAgentPython({ root, platform: 'win32', env: { CODEMAX_AGENT_PYTHON: overridePython } }),
    { command: overridePython, source: 'environment' },
  );
  assert.deepEqual(
    resolveAgentPython({ root, platform: 'win32', env: {} }),
    { command: windowsPython, source: 'project_venv' },
  );
  assert.deepEqual(
    resolveAgentPython({ root, platform: 'linux', env: {} }),
    { command: unixPython, source: 'project_venv' },
  );
  assert.throws(
    () => resolveAgentPython({ root: path.join(root, 'missing'), platform: 'win32', env: {}, pathPython: null }),
    /CODEMAX_AGENT_PYTHON/,
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log('Agent Python resolution contract passed.');
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
```

Expected: failure because `scripts/lib/agent-python.mjs` does not exist.

- [ ] **Step 3: Write the minimal resolver implementation**

```js
import { existsSync } from 'node:fs';
import path from 'node:path';

export function resolveAgentPython({
  root,
  env = process.env,
  platform = process.platform,
  pathPython = platform === 'win32' ? 'python' : 'python3',
  exists = existsSync,
} = {}) {
  const venvPython = path.join(
    root,
    'agent',
    '.venv',
    platform === 'win32' ? 'Scripts' : 'bin',
    platform === 'win32' ? 'python.exe' : 'python',
  );
  const candidates = [
    ['environment', env.CODEMAX_AGENT_PYTHON],
    ['project_venv', venvPython],
    ['path', pathPython],
  ];

  for (const [source, command] of candidates) {
    if (!command) continue;
    if (source === 'path' || exists(command)) return { command, source };
  }

  throw new Error(
    `Agent Python was not found. Set CODEMAX_AGENT_PYTHON or create ${venvPython}.`,
  );
}
```

- [ ] **Step 4: Make PATH fallback testable without invoking a shell**

Update the test to pass `pathPython: null` for the missing-path assertion. Keep PATH resolution as a command name rather than a filesystem path, because `spawnSync` resolves it through the host environment.

- [ ] **Step 5: Run the test to verify it passes**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
```

Expected: `Agent Python resolution contract passed.`

- [ ] **Step 6: Commit the focused test/helper change**

```powershell
git add scripts/lib/agent-python.mjs tests/scripts/verify-agent-python-resolution.mjs
git commit -m "test: cover deterministic agent python resolution"
```

### Task 2: Make runtime packaging deterministic, diagnostic and stale-runtime-safe

**Files:**
- Modify: `scripts/lib/agent-python.mjs`
- Modify: `scripts/build-agent-runtime.mjs`
- Modify: `tests/scripts/verify-agent-python-resolution.mjs`

- [ ] **Step 1: Add a failing module-validation test**

Append to `tests/scripts/verify-agent-python-resolution.mjs`:

```js
import { assertPythonModule } from '../../scripts/lib/agent-python.mjs';

assert.throws(
  () => assertPythonModule({ command: 'python', moduleName: 'PyInstaller', status: 1 }),
  /PyInstaller is required.*python -m pip install "pyinstaller>=6,<7"/s,
);
```

Design `assertPythonModule` as a pure result validator so it can be tested without calling a real Python executable.

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
```

Expected: import/export failure for `assertPythonModule`.

- [ ] **Step 3: Implement explicit PyInstaller validation**

Add to `scripts/lib/agent-python.mjs`:

```js
export function assertPythonModule({ moduleName, error, signal, status }) {
  if (error || signal || typeof status !== 'number') throw new Error('Agent Python check did not complete.');
  if (status === 0) return;
  throw new Error(
    `${moduleName} is required to build the Agent runtime. Install the build dependency with ` +
      `python -m pip install "pyinstaller>=6,<7" or set CODEMAX_AGENT_PYTHON to a prepared interpreter.`,
  );
}
```

In `scripts/build-agent-runtime.mjs`, import `resolveAgentPython` and `assertPythonModule`, then replace `spawnSync('python', ...)` with:

```js
const runtimeTarget = path.join(runtimeRoot, executableName);
rmSync(runtimeTarget, { force: true });

const python = resolveAgentPython({ root });
console.log(`Using Agent Python source: ${python.source}`);
const moduleCheck = spawnSync(python.command, ['-c', 'import PyInstaller'], {
  cwd: agentRoot,
  stdio: 'ignore',
  shell: false,
});
assertPythonModule({ moduleName: 'PyInstaller', ...moduleCheck });
const result = spawnSync(python.command, ['-m', 'PyInstaller', /* existing args */], {
  cwd: agentRoot,
  stdio: 'inherit',
  shell: false,
});
```

The source-level contract must assert this order: final `runtimeTarget` removal occurs after target calculation but before `resolveAgentPython({ root })` and the PyInstaller module preflight/build. It must also assert that a failed build exits before any later source inspection or `cpSync(source, runtimeTarget)` can use an artifact. A successful build remains the only path that copies the verified source to `runtimeTarget`.

- [ ] **Step 4: Run the resolver contract again**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
```

Expected: `Agent Python resolution contract passed.`

- [ ] **Step 5: Run runtime packaging for a red/green environment result**

Run:

```powershell
npm run build:agent-runtime
```

Expected before dependency bootstrap: either a generated runtime, or a failure that records only the selected interpreter source and contains the PyInstaller installation command. The failure must leave no prior final runtime for later validation to reuse. No generic `No module named PyInstaller` should remain.

- [ ] **Step 6: Commit the deterministic runtime packaging change**

```powershell
git add scripts/lib/agent-python.mjs scripts/build-agent-runtime.mjs tests/scripts/verify-agent-python-resolution.mjs
git commit -m "fix: make agent runtime python selection explicit"
```

### Task 3: Track the build-only PyInstaller dependency and prepare the project environment

**Files:**
- Modify: `agent/pyproject.toml`
- Modify: `agent/uv.lock`

- [ ] **Step 1: Add the declared development dependency**

Update the existing `dev` extra in `agent/pyproject.toml`:

```toml
[project.optional-dependencies]
dev = [
  "pytest>=8.0.0",
  "ruff>=0.5.0",
  "pyinstaller>=6,<7"
]
```

- [ ] **Step 2: Regenerate the lock file without changing runtime dependencies**

Run from `<repo-root>/agent`:

```powershell
uv lock
uv sync --extra dev
```

Expected: `agent/uv.lock` contains PyInstaller and `agent/.venv` can import it.

- [ ] **Step 3: Verify the package uses the project Agent environment**

Run:

```powershell
<repo-root>/agent/.venv/Scripts/python.exe -c "import PyInstaller; print(PyInstaller.__version__)"
```

Expected: a PyInstaller version in the `6.x` range.

- [ ] **Step 4: Build the runtime**

Run:

```powershell
npm run build:agent-runtime
Test-Path '<repo-root>/output/runtime/agent/codemax-agent.exe'
```

Expected: build success and `True`.

- [ ] **Step 5: Commit dependency reproducibility data**

```powershell
git add agent/pyproject.toml agent/uv.lock
git commit -m "build: declare agent runtime packaging dependency"
```

### Task 4: Use the same Python contract for S11

**Files:**
- Modify: `scripts/check-s11.mjs`
- Modify: `tests/scripts/verify-agent-python-resolution.mjs`

- [ ] **Step 1: Add a source-level regression assertion**

Append to `tests/scripts/verify-agent-python-resolution.mjs`:

```js
import { readFileSync } from 'node:fs';
const s11 = readFileSync(path.join(process.cwd(), 'scripts', 'check-s11.mjs'), 'utf8');
assert.match(s11, /resolveAgentPython/);
assert.match(s11, /-m', 'pytest'/);
assert.doesNotMatch(s11, /run\('python', \['tests\/test_s11_mvp_acceptance\.py'\]/);
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
```

Expected: assertion failure because S11 still hard-codes `python`.

- [ ] **Step 3: Change S11 to resolve the Agent Python once**

At the top of `scripts/check-s11.mjs` add:

```js
import { resolveAgentPython } from './lib/agent-python.mjs';
```

Before the first `run` call add:

```js
const agentPython = resolveAgentPython({ root });
console.log(`Using Agent Python source: ${agentPython.source}`);
```

Replace the first S11 invocation with:

```js
run(agentPython.command, ['-m', 'pytest', 'tests/test_s11_mvp_acceptance.py', '-q'], {
  cwd: path.join(root, 'agent'),
});
```

- [ ] **Step 4: Run the script contract and the actual S11 check**

Run:

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
npm run check:s11
```

Expected: pytest starts from `agent/.venv`; any failure after that comes from Rust/Tauri or the actual S11 acceptance test, not a missing system pytest module.

- [ ] **Step 5: Commit the S11 interpreter fix**

```powershell
git add scripts/check-s11.mjs tests/scripts/verify-agent-python-resolution.mjs
git commit -m "fix: run s11 acceptance with agent environment"
```

### Task 5: Re-run release gates and record actual remaining failures

**Files:**
- No source files required unless a newly exposed failure is in scope.
- Generated only: `output/runtime/agent/codemax-agent.exe`, `output/release-smoke/latest/*`

- [ ] **Step 1: Run targeted checks**

```powershell
node tests/scripts/verify-agent-python-resolution.mjs
npm run build:agent-runtime
npm run check:tauri
npm run check:a-line
npm run check:s11
```

Expected: all commands pass, or each failure is a real compilation/test failure with no missing-runtime-resource and no missing-system-pytest error.

- [ ] **Step 2: Run the D-line release smoke**

```powershell
npm run check:d-line-release
```

Expected: report at `output/release-smoke/latest/release-smoke-report.md` records current command statuses and does not list `tauri_backend_check_failed` due to a missing Agent executable.

- [ ] **Step 3: Inspect repo state and report residual release work**

```powershell
git status --short
git diff --check
Get-Content '<repo-root>/output/release-smoke/latest/release-smoke-report.md'
```

Expected: only planned source/lock/spec/plan changes are present; generated `output/` remains ignored.

- [ ] **Step 4: Commit verification documentation only if changed**

```powershell
git status --short
```

Do not commit generated runtime or release-smoke artifacts.

## Plan self-review

- Spec coverage: Task 1 covers deterministic selection; Task 2 covers PyInstaller diagnostic behavior; Task 3 makes the build dependency reproducible; Task 4 fixes S11; Task 5 verifies the entire intended release chain.
- No hidden installation: Task 3 explicitly bootstraps via `uv sync --extra dev`; scripts never install packages.
- No secret exposure: commands and expected output avoid printing credentials, model configuration, or private tokens.
- Scope restraint: no UI, database, workflow, agent protocol or installer behavior changes are included.
