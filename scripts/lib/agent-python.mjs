import { existsSync, statSync } from 'node:fs';
import path from 'node:path';

const unavailableDiagnostic = 'Agent Python is unavailable. Configure CODEMAX_AGENT_PYTHON, create the project Agent virtual environment, or install Python on PATH.';
const missingRootDiagnostic = 'Agent Python cannot be resolved because the project root is not configured.';

export function resolveAgentPython({
  root,
  env = process.env,
  platform = process.platform,
  pathPython = platform === 'win32' ? 'python' : 'python3',
  exists = existsSync,
  stat = statSync,
} = {}) {
  if (typeof root !== 'string' || root.length === 0) {
    throw new Error(missingRootDiagnostic);
  }

  const pathApi = platform === 'win32' ? path.win32 : path.posix;
  const venvPython = pathApi.join(
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
    if (source === 'path') return { command, source };
    if (isUsableLocalPython(command, platform, exists, stat)) return { command, source };
  }

  throw new Error(unavailableDiagnostic);
}

function isUsableLocalPython(command, platform, exists, stat) {
  if (!exists(command)) return false;

  try {
    const metadata = stat(command);
    return metadata.isFile() && (platform === 'win32' || (metadata.mode & 0o111) !== 0);
  } catch {
    return false;
  }
}
export function assertPythonModule({ moduleName, error, signal, status } = {}) {
  assertPythonProcessCompleted(
    { error, signal, status },
    {
      start: 'Agent Python could not be started. Configure CODEMAX_AGENT_PYTHON, create the project Agent virtual environment, or install Python on PATH.',
      interrupted: 'Agent Python check was interrupted.',
      incomplete: 'Agent Python check did not complete.',
    },
  );

  if (status === 0) return;

  throw new Error(
    `${moduleName} is required to build the Agent runtime. Install the build dependency with ` +
      `python -m pip install "pyinstaller>=6,<7" or set CODEMAX_AGENT_PYTHON to a prepared interpreter.`,
  );
}

export function assertPythonBuild({ error, signal, status } = {}) {
  assertPythonProcessCompleted(
    { error, signal, status },
    {
      start: 'Agent runtime build could not be started. Verify the selected Agent Python installation and retry.',
      interrupted: 'Agent runtime build was interrupted.',
      incomplete: 'Agent runtime build did not complete.',
    },
  );
}

function assertPythonProcessCompleted({ error, signal, status }, diagnostics) {
  if (error) throw new Error(diagnostics.start);
  if (signal) throw new Error(diagnostics.interrupted);
  if (typeof status !== 'number') throw new Error(diagnostics.incomplete);
}
