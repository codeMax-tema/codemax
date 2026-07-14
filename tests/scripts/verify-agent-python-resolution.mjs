import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import path from 'node:path';
import { assertPythonBuild, assertPythonModule, resolveAgentPython } from '../../scripts/lib/agent-python.mjs';

const unavailableDiagnostic = 'Agent Python is unavailable. Configure CODEMAX_AGENT_PYTHON, create the project Agent virtual environment, or install Python on PATH.';
const missingRootDiagnostic = 'Agent Python cannot be resolved because the project root is not configured.';

function file(mode = 0o755) {
  return { type: 'file', mode };
}

function directory() {
  return { type: 'directory', mode: 0o755 };
}

function resolverOptions(entries) {
  return {
    exists: (candidate) => entries.has(candidate),
    stat: (candidate) => {
      const entry = entries.get(candidate);
      if (!entry) throw new Error(`Missing test entry: ${candidate}`);
      return {
        isFile: () => entry.type === 'file',
        mode: entry.mode,
      };
    },
  };
}

const windowsRoot = 'C:\\codemax\\agent-resolution';
const windowsVenvPython = path.win32.join(windowsRoot, 'agent', '.venv', 'Scripts', 'python.exe');
const windowsOverridePython = path.win32.join(windowsRoot, 'tools', 'python.exe');
const windowsDirectoryOverride = path.win32.join(windowsRoot, 'tools', 'python-directory');
const windowsEntries = new Map([
  [windowsVenvPython, file()],
  [windowsOverridePython, file()],
  [windowsDirectoryOverride, directory()],
]);
const windowsOptions = resolverOptions(windowsEntries);

assert.match(windowsVenvPython, /\\/);
assert.doesNotMatch(windowsVenvPython, /\//);
assert.deepEqual(
  resolveAgentPython({
    root: windowsRoot,
    platform: 'win32',
    env: { CODEMAX_AGENT_PYTHON: windowsOverridePython },
    pathPython: null,
    ...windowsOptions,
  }),
  { command: windowsOverridePython, source: 'environment' },
);
assert.deepEqual(
  resolveAgentPython({ root: windowsRoot, platform: 'win32', env: {}, pathPython: null, ...windowsOptions }),
  { command: windowsVenvPython, source: 'project_venv' },
);
assert.deepEqual(
  resolveAgentPython({
    root: windowsRoot,
    platform: 'win32',
    env: { CODEMAX_AGENT_PYTHON: windowsDirectoryOverride },
    pathPython: null,
    ...windowsOptions,
  }),
  { command: windowsVenvPython, source: 'project_venv' },
);

const posixRoot = '/workspace/codemax';
const posixVenvPython = path.posix.join(posixRoot, 'agent', '.venv', 'bin', 'python');
const executablePosixOptions = resolverOptions(new Map([[posixVenvPython, file(0o755)]]));
const nonExecutablePosixOptions = resolverOptions(new Map([[posixVenvPython, file(0o644)]]));

assert.match(posixVenvPython, /\//);
assert.doesNotMatch(posixVenvPython, /\\/);
assert.deepEqual(
  resolveAgentPython({
    root: posixRoot,
    platform: 'linux',
    env: {},
    pathPython: null,
    ...executablePosixOptions,
  }),
  { command: posixVenvPython, source: 'project_venv' },
);
assert.deepEqual(
  resolveAgentPython({
    root: posixRoot,
    platform: 'linux',
    env: {},
    pathPython: 'python3',
    ...nonExecutablePosixOptions,
  }),
  { command: 'python3', source: 'path' },
);
assert.deepEqual(
  resolveAgentPython({ root: windowsRoot, platform: 'win32', env: {}, pathPython: 'python', exists: () => false }),
  { command: 'python', source: 'path' },
);

const confidentialRoot = '/workspace/confidential-user-SECRET';
const confidentialOverride = '/tools/confidential-user-SECRET/python';
assert.throws(
  () => resolveAgentPython({
    root: confidentialRoot,
    platform: 'linux',
    env: { CODEMAX_AGENT_PYTHON: confidentialOverride },
    pathPython: null,
    exists: () => false,
  }),
  (error) => {
    assert.equal(error.message, unavailableDiagnostic);
    assert.equal(error.message.includes('confidential-user-SECRET'), false);
    assert.equal(error.message.includes(confidentialRoot), false);
    assert.equal(error.message.includes(confidentialOverride), false);
    return true;
  },
);

for (const root of [undefined, '']) {
  assert.throws(
    () => resolveAgentPython({ root, env: { CODEMAX_AGENT_PYTHON: confidentialOverride }, pathPython: null }),
    (error) => {
      assert.equal(error.message, missingRootDiagnostic);
      assert.equal(error.message.includes(confidentialOverride), false);
      return true;
    },
  );
}

const pyInstallerDiagnostic = 'PyInstaller is required to build the Agent runtime. Install the build dependency with python -m pip install "pyinstaller>=6,<7" or set CODEMAX_AGENT_PYTHON to a prepared interpreter.';
const pythonStartDiagnostic = 'Agent Python could not be started. Configure CODEMAX_AGENT_PYTHON, create the project Agent virtual environment, or install Python on PATH.';
const pythonCheckInterruptedDiagnostic = 'Agent Python check was interrupted.';
const pythonCheckIncompleteDiagnostic = 'Agent Python check did not complete.';
const runtimeBuildStartDiagnostic = 'Agent runtime build could not be started. Verify the selected Agent Python installation and retry.';
const runtimeBuildInterruptedDiagnostic = 'Agent runtime build was interrupted.';
const runtimeBuildIncompleteDiagnostic = 'Agent runtime build did not complete.';
const confidentialPythonCommand = '/workspace/confidential-user-SECRET/.venv/bin/python';

for (const [result, diagnostic] of [
  [{ error: new Error(`spawn ${confidentialPythonCommand} ENOENT`), status: null }, pythonStartDiagnostic],
  [{ signal: 'SIGTERM', status: null }, pythonCheckInterruptedDiagnostic],
  [{ status: null }, pythonCheckIncompleteDiagnostic],
  [{ status: 1 }, pyInstallerDiagnostic],
]) {
  assert.throws(
    () => assertPythonModule({ command: confidentialPythonCommand, moduleName: 'PyInstaller', ...result }),
    (error) => {
      assert.equal(error.message, diagnostic);
      assert.equal(error.message.includes('confidential-user-SECRET'), false);
      return true;
    },
  );
}
assert.doesNotThrow(() => assertPythonModule({ moduleName: 'PyInstaller', status: 0 }));

for (const [result, diagnostic] of [
  [{ error: new Error(`spawn ${confidentialPythonCommand} ENOENT`), status: null }, runtimeBuildStartDiagnostic],
  [{ signal: 'SIGTERM', status: null }, runtimeBuildInterruptedDiagnostic],
  [{ status: null }, runtimeBuildIncompleteDiagnostic],
]) {
  assert.throws(
    () => assertPythonBuild(result),
    (error) => {
      assert.equal(error.message, diagnostic);
      assert.equal(error.message.includes('confidential-user-SECRET'), false);
      return true;
    },
  );
}
assert.doesNotThrow(() => assertPythonBuild({ status: 1 }));

const buildScript = readFileSync(new URL('../../scripts/build-agent-runtime.mjs', import.meta.url), 'utf8');
assert.match(buildScript, /console\.log\(`Using Agent Python source: \$\{python\.source\}`\);/);
assert.doesNotMatch(buildScript, /console\.log\([^\n]*python\.command/);
assert.match(buildScript, /throw new Error\('Agent runtime was not created\.'\);/);
assert.doesNotMatch(buildScript, /Agent runtime was not created: \$\{source\}/);
const runtimeTargetDeclaration = buildScript.indexOf('const runtimeTarget = path.join(runtimeRoot, executableName);');
const runtimeTargetRemoval = buildScript.indexOf('rmSync(runtimeTarget, { force: true });');
const resolvePython = buildScript.indexOf('const python = resolveAgentPython({ root });');
const pyInstallerModuleCheck = buildScript.indexOf('const moduleCheck = spawnSync(');
const pyInstallerBuild = buildScript.indexOf('const result = spawnSync(');
const failedBuildExit = buildScript.indexOf('if (result.status !== 0) process.exit(result.status);');
const runtimeSource = buildScript.indexOf("const source = path.join(buildRoot, 'dist', executableName);");
const runtimeCopy = buildScript.indexOf('cpSync(source, runtimeTarget);');
assert.ok(
  runtimeTargetDeclaration >= 0 &&
    runtimeTargetRemoval > runtimeTargetDeclaration &&
    runtimeTargetRemoval < resolvePython &&
    runtimeTargetRemoval < pyInstallerModuleCheck &&
    runtimeTargetRemoval < pyInstallerBuild,
  'The previous runtime must be removed before interpreter resolution or PyInstaller preflight/build.',
);
assert.ok(
  failedBuildExit > pyInstallerBuild && runtimeSource > failedBuildExit && runtimeCopy > runtimeSource,
  'A failed build must exit before later steps can inspect or copy a runtime artifact.',
);

const s11Script = readFileSync(new URL('../../scripts/check-s11.mjs', import.meta.url), 'utf8');
assert.match(s11Script, /import \{ resolveAgentPython \} from '\.\/lib\/agent-python\.mjs';/);
assert.match(s11Script, /const python = resolveAgentPython\(\{ root \}\);/);
assert.match(s11Script, /console\.log\(`Using Agent Python source: \$\{python\.source\}`\);/);
assert.match(
  s11Script,
  /run\(python\.command, \['-m', 'pytest', 'tests\/test_s11_mvp_acceptance\.py', '-q'\], \{[\s\S]*?logCommand: false,[\s\S]*?\}\);/,
);
assert.doesNotMatch(s11Script, /run\(\s*['"]python['"]/);
assert.match(
  s11Script,
  /run\('cargo', \[[\s\S]*?'test',[\s\S]*?'--manifest-path',[\s\S]*?'apps\/desktop\/src-tauri\/Cargo\.toml',[\s\S]*?'s11',[\s\S]*?\]\);/,
);
const pytestRun = s11Script.indexOf("run(python.command, ['-m', 'pytest', 'tests/test_s11_mvp_acceptance.py', '-q']");
const cargoRun = s11Script.indexOf("run('cargo', [");
assert.ok(pytestRun >= 0 && cargoRun > pytestRun, 'S11 Cargo tests must run after resolved Python pytest.');
assert.doesNotMatch(s11Script, /console\.\w+\([^\n]*python\.command/);

const runFunctionMatch = s11Script.match(/function run\(command, args, options = \{\}\) \{[\s\S]*?\n\}\n\nconst python/);
assert.ok(runFunctionMatch, 'S11 runner implementation must remain extractable for diagnostic contract checks.');
const runFunctionSource = runFunctionMatch[0].replace(/\n\nconst python$/, '');
const errorBranchMatch = runFunctionSource.match(/if \(result\.error\) \{[\s\S]*?\n  \}/);
assert.ok(errorBranchMatch, 'S11 runner must handle spawn errors.');
assert.doesNotMatch(errorBranchMatch[0], /error\.message/);
assert.doesNotMatch(errorBranchMatch[0], /console\.error\(\s*command\s*\)/);

function runS11WithResult(result) {
  const diagnostics = [];
  const exitCodes = [];
  const context = {
    root: '/workspace/codemax',
    spawnSync: () => result,
    console: {
      log: () => {},
      error: (diagnostic) => diagnostics.push(diagnostic),
    },
    process: {
      exit: (code) => {
        exitCodes.push(code);
        throw new Error(`exit:${code}`);
      },
    },
  };
  runInNewContext(`${runFunctionSource}\nglobalThis.run = run;`, context);
  assert.throws(
    () => context.run(confidentialPythonCommand, ['-m', 'pytest'], {
      cwd: '/workspace/codemax/agent',
      commandLabel: 'Agent Python',
      logCommand: false,
    }),
    /exit:1/,
  );
  return { diagnostics, exitCodes };
}

for (const [result, diagnostic] of [
  [{ error: new Error(`spawn ${confidentialPythonCommand} ENOENT`), status: null }, 'Agent Python could not be started.'],
  [{ signal: 'SIGTERM', status: null }, 'Agent Python was interrupted.'],
  [{ status: null }, 'Agent Python did not complete.'],
]) {
  const outcome = runS11WithResult(result);
  assert.deepEqual(outcome.diagnostics, [diagnostic]);
  assert.deepEqual(outcome.exitCodes, [1]);
  assert.equal(JSON.stringify(outcome.diagnostics).includes('confidential-user-SECRET'), false);
}

const packageJson = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'));
assert.equal(packageJson.scripts['check:runtime-scripts'], 'node tests/scripts/verify-agent-python-resolution.mjs');
assert.equal(
  packageJson.scripts.check,
  'npm run check:architecture && npm run check:contracts && npm run check:frontend && npm run check:release && npm run check:runtime-scripts',
);
assert.equal(packageJson.scripts['check:s11'], 'npm run check:runtime-scripts && node scripts/check-s11.mjs');
console.log('Agent Python resolution contract passed.');
