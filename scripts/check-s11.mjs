import { spawnSync } from 'node:child_process';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { resolveAgentPython } from './lib/agent-python.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function run(command, args, options = {}) {
  const cwd = options.cwd ?? root;
  const commandLabel = options.commandLabel ?? command;
  const diagnosticLabel = options.commandLabel ?? (command === 'cargo' ? 'cargo' : 'Command');
  if (options.logCommand !== false) {
    console.log(`\n> ${commandLabel} ${args.join(' ')}`);
  }
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    shell: false,
  });

  if (result.error) {
    console.error(`${diagnosticLabel} could not be started.`);
    process.exit(1);
  }

  if (result.signal) {
    console.error(`${diagnosticLabel} was interrupted.`);
    process.exit(1);
  }

  if (result.status === null) {
    console.error(`${diagnosticLabel} did not complete.`);
    process.exit(1);
  }

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

const python = resolveAgentPython({ root });
console.log(`Using Agent Python source: ${python.source}`);
run(python.command, ['-m', 'pytest', 'tests/test_s11_mvp_acceptance.py', '-q'], {
  cwd: path.join(root, 'agent'),
  commandLabel: 'Agent Python',
  logCommand: false,
});
run('cargo', [
  'test',
  '--manifest-path',
  'apps/desktop/src-tauri/Cargo.toml',
  's11',
  '--',
  '--nocapture',
]);

console.log('\nS11 acceptance checks passed.');
