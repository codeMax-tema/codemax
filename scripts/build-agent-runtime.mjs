import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { assertPythonBuild, assertPythonModule, resolveAgentPython } from './lib/agent-python.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const agentRoot = path.join(root, 'agent');
const buildRoot = path.join(root, 'output', 'build', 'agent-runtime');
const runtimeRoot = path.join(root, 'output', 'runtime', 'agent');
const executableName = process.platform === 'win32' ? 'codemax-agent.exe' : 'codemax-agent';
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

rmSync(buildRoot, { recursive: true, force: true });
mkdirSync(buildRoot, { recursive: true });
mkdirSync(runtimeRoot, { recursive: true });

const result = spawnSync(
  python.command,
  [
    '-m',
    'PyInstaller',
    '--noconfirm',
    '--clean',
    '--distpath',
    path.join(buildRoot, 'dist'),
    '--workpath',
    path.join(buildRoot, 'work'),
    path.join(agentRoot, 'codemax_agent.spec'),
  ],
  { cwd: agentRoot, stdio: 'inherit', shell: false },
);
assertPythonBuild(result);
if (result.status !== 0) process.exit(result.status);

const source = path.join(buildRoot, 'dist', executableName);
if (!existsSync(source)) throw new Error('Agent runtime was not created.');
cpSync(source, runtimeTarget);
console.log(`Agent runtime written to ${path.relative(root, runtimeTarget)}`);
