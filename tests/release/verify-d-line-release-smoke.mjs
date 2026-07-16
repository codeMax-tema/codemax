import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();

function readJson(path) {
  return JSON.parse(readFileSync(join(root, path), 'utf8'));
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const privacyGate = spawnSync(process.execPath, ['tests/privacy/verify-model-gateway.mjs'], {
  cwd: root,
  encoding: 'utf8',
});
if (privacyGate.stdout) process.stdout.write(privacyGate.stdout);
if (privacyGate.stderr) process.stderr.write(privacyGate.stderr);
assert(privacyGate.status === 0, 'REL-P0-006 privacy/model gateway gate must pass before release');

const packageJson = readJson('package.json');

assert(
  packageJson.scripts['check:d-line-release'] === 'node scripts/check-d-line-release-smoke.mjs',
  'package.json must expose check:d-line-release',
);
assert(
  packageJson.scripts['release:smoke:package'] === 'node scripts/check-d-line-release-smoke.mjs --package',
  'package.json must expose release:smoke:package',
);
assert(
  existsSync(join(root, 'scripts/check-d-line-release-smoke.mjs')),
  'D-line release smoke script must exist',
);
assert(
  existsSync(join(root, 'docs/release/d-line-release-smoke.md')),
  'D-line release smoke handoff doc must exist',
);

const script = readFileSync(join(root, 'scripts/check-d-line-release-smoke.mjs'), 'utf8');

for (const marker of [
  'release_smoke_report',
  'packaging_artifacts',
  'npm run check',
  'npm run build:desktop',
  'npm run check:tauri',
  'main_chain_smoke',
  'privacy_smoke',
  'profile_memory_smoke',
  'delivery_review_smoke',
  'packaging_smoke',
]) {
  assert(script.includes(marker), `D-line release smoke script must include ${marker}`);
}

console.log('D-line release smoke contract verified');
