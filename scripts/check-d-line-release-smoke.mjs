import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const runPackage = args.includes('--package');
const outputIndex = args.indexOf('--output');
const outputDir =
  outputIndex >= 0 && args[outputIndex + 1]
    ? path.resolve(root, args[outputIndex + 1])
    : path.join(root, 'output', 'release-smoke', 'latest');

const commands = [
  {
    id: 'source_contract',
    display: 'npm run check',
    command: 'npm',
    args: ['run', 'check'],
  },
  {
    id: 'desktop_web_build',
    display: 'npm run build:desktop',
    command: 'npm',
    args: ['run', 'build:desktop'],
  },
  {
    id: 'tauri_backend_check',
    display: 'npm run check:tauri',
    command: 'npm',
    args: ['run', 'check:tauri'],
  },
];

if (runPackage) {
  commands.push({
    id: 'windows_installer_bundle',
    display: 'npm --workspace @codemax/desktop run tauri:build',
    command: 'npm',
    args: ['--workspace', '@codemax/desktop', 'run', 'tauri:build'],
  });
}

mkdirSync(outputDir, { recursive: true });

const report = {
  schema: 'release_smoke_report',
  generatedAt: new Date().toISOString(),
  git: gitInfo(),
  mode: runPackage ? 'package' : 'source-smoke',
  commands: [],
  packaging_artifacts: [],
  smoke: {},
  risks: [],
};

for (const command of commands) {
  const result = run(command);
  report.commands.push(result);
  if (result.status !== 'passed') {
    report.risks.push({
      id: `${command.id}_failed`,
      severity: 'blocked',
      summary: `${command.display} failed. Release smoke cannot continue.`,
    });
    break;
  }
}

report.packaging_artifacts = collectPackagingArtifacts();
report.smoke = buildSmokeSections(report);
report.overallStatus = overallStatus(report);

writeReport(report);

console.log(`\nD-line release smoke report written to ${path.relative(root, outputDir)}`);
console.log(`Overall status: ${report.overallStatus}`);

if (report.overallStatus === 'failed') {
  process.exit(1);
}

function run(definition) {
  console.log(`\n> ${definition.display}`);
  const startedAt = Date.now();
  const result = spawnSync(definition.command, definition.args, {
    cwd: root,
    encoding: 'utf8',
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  const durationMs = Date.now() - startedAt;
  const stdout = redact(result.stdout ?? '');
  const stderr = redact(result.stderr ?? '');

  if (stdout.trim()) {
    process.stdout.write(stdout);
  }
  if (stderr.trim()) {
    process.stderr.write(stderr);
  }

  return {
    id: definition.id,
    command: definition.display,
    status: result.status === 0 ? 'passed' : 'failed',
    exitCode: result.status ?? 1,
    durationMs,
    stdoutTail: tail(stdout),
    stderrTail: tail(stderr),
    error: result.error ? result.error.message : null,
  };
}

function buildSmokeSections(currentReport) {
  const commandPassed = (id) =>
    currentReport.commands.some((command) => command.id === id && command.status === 'passed');
  const installerArtifacts = currentReport.packaging_artifacts.filter((artifact) => artifact.kind === 'installer');
  const appArtifacts = currentReport.packaging_artifacts.filter((artifact) => artifact.kind === 'runtime');

  return {
    main_chain_smoke: {
      status: 'pending_integration',
      evidence: [
        'A-line task/worktree/agent loop commands are present in source checks.',
        'Run full installed-app task creation smoke after A/B/C latest branches are merged.',
      ],
    },
    privacy_smoke: {
      status: 'pending_integration',
      evidence: [
        'Model API key display and storage checks are covered by D-line settings smoke.',
        'Full Privacy Ledger assertions should be connected after B-line latest work lands.',
      ],
    },
    profile_memory_smoke: {
      status: 'pending_integration',
      evidence: [
        'Memory cockpit entry points are present in the UI contract.',
        'Long-term memory edit/delete smoke should be connected after B-line latest work lands.',
      ],
    },
    delivery_review_smoke: {
      status: 'pending_integration',
      evidence: [
        'C-line proof, gate, score, risk, rules, hooks, and model arena UI/API markers are present in source checks.',
        'Installed-app export smoke should be connected after C-line latest work lands.',
      ],
    },
    packaging_smoke: {
      status:
        commandPassed('desktop_web_build') &&
        commandPassed('tauri_backend_check') &&
        (!runPackage || installerArtifacts.length > 0 || appArtifacts.length > 0)
          ? 'passed'
          : 'pending_package',
      evidence: [
        commandPassed('desktop_web_build') ? 'Desktop web build completed.' : 'Desktop web build did not complete.',
        commandPassed('tauri_backend_check') ? 'Tauri backend check completed.' : 'Tauri backend check did not complete.',
        runPackage
          ? `${installerArtifacts.length + appArtifacts.length} package artifact(s) detected.`
          : 'Run npm run release:smoke:package to build and record installer artifacts.',
      ],
    },
  };
}

function collectPackagingArtifacts() {
  const candidates = [];
  addIfExists(candidates, 'frontend_dist', path.join(root, 'apps', 'desktop', 'dist', 'index.html'), 'runtime');
  addIfExists(
    candidates,
    'debug_executable',
    path.join(root, 'apps', 'desktop', 'src-tauri', 'target', 'debug', 'codemax-desktop.exe'),
    'runtime',
  );
  addIfExists(
    candidates,
    'official_icon',
    path.join(root, 'apps', 'desktop', 'src-tauri', 'icons', 'icon.ico'),
    'brand',
  );

  const bundleRoot = path.join(root, 'apps', 'desktop', 'src-tauri', 'target', 'release', 'bundle');
  for (const file of findFiles(bundleRoot)) {
    if (/\.(msi|exe|msix|zip|dmg|deb|rpm|appimage)$/i.test(file)) {
      addIfExists(candidates, 'installer', file, 'installer');
    }
  }

  return candidates;
}

function addIfExists(list, id, file, kind) {
  if (!existsSync(file)) {
    return;
  }
  const stats = statSync(file);
  if (!stats.isFile()) {
    return;
  }
  list.push({
    id,
    kind,
    path: path.relative(root, file).replaceAll('\\', '/'),
    bytes: stats.size,
  });
}

function findFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...findFiles(fullPath));
    } else if (entry.isFile()) {
      files.push(fullPath);
    }
  }
  return files;
}

function overallStatus(currentReport) {
  if (currentReport.commands.some((command) => command.status === 'failed')) {
    return 'failed';
  }
  if (Object.values(currentReport.smoke).some((section) => section.status !== 'passed')) {
    return 'degraded';
  }
  return 'passed';
}

function gitInfo() {
  return {
    branch: capture('git', ['branch', '--show-current']),
    commit: capture('git', ['rev-parse', '--short', 'HEAD']),
    status: capture('git', ['status', '--short']),
  };
}

function capture(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status !== 0) {
    return '';
  }
  return redact((result.stdout ?? '').trim());
}

function writeReport(currentReport) {
  const jsonPath = path.join(outputDir, 'release-smoke-report.json');
  const markdownPath = path.join(outputDir, 'release-smoke-report.md');
  writeFileSync(jsonPath, `${JSON.stringify(currentReport, null, 2)}\n`, 'utf8');
  writeFileSync(markdownPath, markdown(currentReport), 'utf8');
}

function markdown(currentReport) {
  const commandRows = currentReport.commands
    .map(
      (command) =>
        `| ${command.command} | ${command.status} | ${command.exitCode} | ${command.durationMs}ms |`,
    )
    .join('\n');
  const artifactRows =
    currentReport.packaging_artifacts
      .map((artifact) => `| ${artifact.id} | ${artifact.kind} | ${artifact.path} | ${artifact.bytes} |`)
      .join('\n') || '| none | pending | - | 0 |';
  const smokeRows = Object.entries(currentReport.smoke)
    .map(([id, section]) => `| ${id} | ${section.status} | ${section.evidence.join('<br>')} |`)
    .join('\n');

  return `# D-line Release Smoke Report

- Schema: \`${currentReport.schema}\`
- Generated: ${currentReport.generatedAt}
- Mode: ${currentReport.mode}
- Branch: ${currentReport.git.branch}
- Commit: ${currentReport.git.commit}
- Overall: ${currentReport.overallStatus}

## Commands

| Command | Status | Exit | Duration |
| --- | --- | --- | --- |
${commandRows}

## Packaging Artifacts

| ID | Kind | Path | Bytes |
| --- | --- | --- | --- |
${artifactRows}

## Smoke Sections

| Section | Status | Evidence |
| --- | --- | --- |
${smokeRows}
`;
}

function tail(value, limit = 12000) {
  if (value.length <= limit) {
    return value;
  }
  return value.slice(value.length - limit);
}

function redact(value) {
  return value
    .replace(/sk-[A-Za-z0-9_-]{12,}/g, 'sk-***')
    .replace(/(api[_-]?key\s*[:=]\s*)[^\s"']+/gi, '$1***')
    .replace(/(authorization:\s*bearer\s+)[^\s"']+/gi, '$1***')
    .replace(/\b[a-f0-9]{32,}\b/gi, '***');
}
