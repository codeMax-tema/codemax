import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';

const root = process.cwd();

const requiredFiles = [
  'apps/desktop/package.json',
  'apps/desktop/index.html',
  'apps/desktop/vite.config.ts',
  'apps/desktop/tsconfig.json',
  'apps/desktop/tailwind.config.ts',
  'apps/desktop/src/main.tsx',
  'apps/desktop/src/app/App.tsx',
  'apps/desktop/src/app/providers.tsx',
  'apps/desktop/src/api/tauriClient.ts',
  'apps/desktop/src/api/events.ts',
  'apps/desktop/src/components/ui/button.tsx',
  'apps/desktop/src/components/ui/dialog.tsx',
  'apps/desktop/src/components/ui/table.tsx',
  'apps/desktop/src/components/ui/tabs.tsx',
  'apps/desktop/src/components/ui/toast.tsx',
  'apps/desktop/src/components/ui/toaster.tsx',
  'apps/desktop/src/lib/utils.ts',
  'apps/desktop/src/i18n/index.ts',
  'apps/desktop/src/i18n/locales/zh-CN.json',
  'apps/desktop/src/i18n/locales/en-US.json',
  'apps/desktop/src/state/appStore.ts',
  'apps/desktop/src/types/domain.ts',
  'apps/desktop/src-tauri/Cargo.toml',
  'apps/desktop/src-tauri/tauri.conf.json',
  'apps/desktop/src-tauri/src/main.rs',
  'apps/desktop/src-tauri/src/lib.rs',
  'apps/desktop/src-tauri/src/commands/app.rs',
  'apps/desktop/src-tauri/src/events.rs',
  'apps/desktop/src-tauri/src/core/error.rs',
  'apps/desktop/src-tauri/src/storage/mod.rs',
  'apps/desktop/src-tauri/src/git/mod.rs',
  'apps/desktop/src-tauri/src/exec/mod.rs',
  'apps/desktop/src-tauri/src/safety/mod.rs',
  'apps/desktop/src-tauri/src/agent/mod.rs',
  'agent/pyproject.toml',
  'agent/app/main.py',
  'agent/app/api/health.py',
  'agent/app/core/config.py',
  'agent/app/graph/state.py',
  'agent/app/graph/nodes.py',
  'agent/app/memory/service.py',
  'agent/app/providers/openai_compatible.py',
  'database/migrations/0001_initial.sql',
  'contracts/ipc.schema.json',
  'config/commands.allowlist.json',
  'config/commands.blocklist.json',
  'config/storage-policy.default.json',
  'docs/architecture/overview.md',
  'docs/architecture/runtime-boundaries.md',
  'docs/s2/local-data-model.md',
];

const requiredContent = [
  ['apps/desktop/package.json', '"@tauri-apps/api"'],
  ['apps/desktop/package.json', '"@radix-ui/react-dialog"'],
  ['apps/desktop/package.json', '"class-variance-authority"'],
  ['apps/desktop/package.json', '"tailwind-merge"'],
  ['apps/desktop/package.json', '"react"'],
  ['apps/desktop/src-tauri/Cargo.toml', 'tauri'],
  ['apps/desktop/src-tauri/Cargo.toml', 'rusqlite'],
  ['apps/desktop/src-tauri/src/lib.rs', 'health'],
  ['apps/desktop/src-tauri/src/lib.rs', 'ping'],
  ['apps/desktop/src-tauri/src/lib.rs', 'emit_app_ready'],
  ['apps/desktop/src-tauri/src/lib.rs', 'ManagedStorage::initialize'],
  ['apps/desktop/src-tauri/src/events.rs', 'APP_READY_EVENT'],
  ['apps/desktop/src/api/tauriClient.ts', 'pingDesktop'],
  ['apps/desktop/src/api/events.ts', 'listenAppReady'],
  ['apps/desktop/src/components/ui/button.tsx', 'buttonVariants'],
  ['apps/desktop/src/components/ui/dialog.tsx', '@radix-ui/react-dialog'],
  ['apps/desktop/src/components/ui/tabs.tsx', '@radix-ui/react-tabs'],
  ['apps/desktop/src/components/ui/table.tsx', 'TableHead'],
  ['apps/desktop/src/components/ui/toaster.tsx', 'ToastProvider'],
  ['agent/pyproject.toml', 'fastapi'],
  ['agent/app/main.py', 'create_app'],
  ['database/migrations/0001_initial.sql', 'CREATE TABLE IF NOT EXISTS tasks'],
  ['database/migrations/0001_initial.sql', 'CREATE TABLE IF NOT EXISTS memory_items'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'schema_migrations'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'TaskRepository'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'MemoryRepository'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'CleanupGuard'],
  ['contracts/ipc.schema.json', 'health'],
  ['docs/architecture/overview.md', 'apps/desktop'],
  ['docs/architecture/runtime-boundaries.md', 'Worktree'],
  ['docs/s2/local-data-model.md', 'rusqlite'],
  ['docs/s2/local-data-model.md', 'CleanupGuard'],
];

const missingFiles = requiredFiles.filter((file) => !existsSync(path.join(root, file)));

const missingContent = requiredContent.filter(([file, expected]) => {
  const fullPath = path.join(root, file);
  if (!existsSync(fullPath)) {
    return true;
  }

  return !readFileSync(fullPath, 'utf8').includes(expected);
});

if (missingFiles.length > 0 || missingContent.length > 0) {
  console.error('Architecture contract failed.');
  if (missingFiles.length > 0) {
    console.error('Missing files:');
    for (const file of missingFiles) {
      console.error(`- ${file}`);
    }
  }

  if (missingContent.length > 0) {
    console.error('Missing content:');
    for (const [file, expected] of missingContent) {
      console.error(`- ${file}: ${expected}`);
    }
  }

  process.exit(1);
}

console.log(`Architecture contract passed with ${requiredFiles.length} required files.`);
