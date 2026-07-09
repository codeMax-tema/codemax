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
  'agent/app/scheduler.py',
  'agent/app/context/language_registry.py',
  'agent/app/context/parser_service.py',
  'agent/app/context/retriever.py',
  'agent/app/screenshots/service.py',
  'agent/app/proposals.py',
  'agent/app/memory/service.py',
  'agent/app/providers/openai_compatible.py',
  'agent/tests/test_s11_mvp_acceptance.py',
  'agent/tests/test_s12_scheduler_context.py',
  'agent/tests/test_s12_screenshot_proposals.py',
  'database/migrations/0001_initial.sql',
  'database/migrations/0002_s12_evidence.sql',
  'database/migrations/0007_c_line_delivery_review.sql',
  'contracts/ipc.schema.json',
  'config/commands.allowlist.json',
  'config/commands.blocklist.json',
  'config/storage-policy.default.json',
  'scripts/check-s11.mjs',
  'apps/desktop/src-tauri/src/commands/s11_acceptance.rs',
  'apps/desktop/src-tauri/src/commands/s12_evidence.rs',
  'docs/architecture/overview.md',
  'docs/architecture/runtime-boundaries.md',
  'docs/s2/local-data-model.md',
  'docs/s11/mvp-acceptance.md',
  'docs/s12/e01-e05-enhancements.md',
  'docs/superpowers/specs/2026-07-08-s12-e01-e05-design.md',
  'docs/superpowers/plans/2026-07-08-s12-e01-e05.md',
  'docs/superpowers/plans/2026-07-07-s11-mvp-acceptance.md',
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
  ['agent/app/scheduler.py', 'TaskScheduler'],
  ['agent/app/context/language_registry.py', 'typescript'],
  ['agent/app/context/parser_service.py', 'parser_mode'],
  ['agent/app/context/retriever.py', 'max_files'],
  ['agent/app/screenshots/service.py', 'browserUnavailable'],
  ['agent/app/proposals.py', 'ProposalService'],
  ['agent/tests/test_s11_mvp_acceptance.py', 'CODEMAX_REPAIR'],
  ['agent/tests/test_s11_mvp_acceptance.py', 'S11 Agent acceptance passed'],
  ['agent/tests/test_s12_scheduler_context.py', 'test_context_retriever_is_bounded'],
  ['agent/tests/test_s12_screenshot_proposals.py', 'test_proposal_service_regenerates_with_feedback'],
  ['package.json', '"check:s11"'],
  ['scripts/check-s11.mjs', 'test_s11_mvp_acceptance.py'],
  ['scripts/check-s11.mjs', 'cargo'],
  ['database/migrations/0001_initial.sql', 'CREATE TABLE IF NOT EXISTS tasks'],
  ['database/migrations/0001_initial.sql', 'CREATE TABLE IF NOT EXISTS memory_items'],
  ['database/migrations/0002_s12_evidence.sql', 'CREATE TABLE IF NOT EXISTS quality_gate_results'],
  ['database/migrations/0007_c_line_delivery_review.sql', 'CREATE TABLE IF NOT EXISTS rule_registry'],
  ['database/migrations/0007_c_line_delivery_review.sql', 'CREATE TABLE IF NOT EXISTS hook_runs'],
  ['database/migrations/0007_c_line_delivery_review.sql', 'CREATE TABLE IF NOT EXISTS model_arena_decisions'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'schema_migrations'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', '0002_s12_evidence'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', '0007_c_line_delivery_review'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'TaskRepository'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'MemoryRepository'],
  ['apps/desktop/src-tauri/src/storage/mod.rs', 'CleanupGuard'],
  ['contracts/ipc.schema.json', 'health'],
  ['contracts/ipc.schema.json', 'generate_task_proof_pack'],
  ['contracts/ipc.schema.json', 'get_delivery_review_state'],
  ['contracts/ipc.schema.json', 'record_rule_hit'],
  ['contracts/ipc.schema.json', 'record_hook_run'],
  ['contracts/ipc.schema.json', 'request_hook_approval'],
  ['contracts/ipc.schema.json', 'record_model_arena_decision'],
  ['contracts/ipc.schema.json', 'DeliveryReviewState'],
  ['contracts/ipc.schema.json', 'proofPackFiles'],
  ['contracts/ipc.schema.json', 'privacyLedgerSummary'],
  ['docs/architecture/overview.md', 'apps/desktop'],
  ['docs/architecture/runtime-boundaries.md', 'Worktree'],
  ['docs/s2/local-data-model.md', 'rusqlite'],
  ['docs/s2/local-data-model.md', 'CleanupGuard'],
  ['apps/desktop/src-tauri/src/commands/s11_acceptance.rs', 's11_mvp_demo_repo_runs_from_worktree_to_local_merge'],
  ['apps/desktop/src-tauri/src/commands/s11_acceptance.rs', 's11_acceptance_covers_repository_approval_and_memory_edges'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'generate_task_proof_pack'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'record_quality_gate_result'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'override_quality_gate'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'record_rule_hit'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'record_hook_run'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'request_hook_approval'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'record_model_arena_decision'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'calculate_delivery_score'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'get_delivery_review_state'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'delivery_review_blockers_for_task'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'proof_run_contract'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'proof_privacy_ledger'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'proof_context_sources'],
  ['apps/desktop/src-tauri/src/commands/s12_evidence.rs', 'proof_model_arena'],
  ['apps/desktop/src/features/tasks/TaskOverviewPage.tsx', 's12-privacy-panel'],
  ['apps/desktop/src/features/tasks/TaskOverviewPage.tsx', 's12-proof-files-panel'],
  ['apps/desktop/src-tauri/src/commands/merge.rs', 'delivery_review_blockers_for_task'],
  ['docs/s11/mvp-acceptance.md', 'S11-T01'],
  ['docs/s11/mvp-acceptance.md', 'check:s11'],
  ['docs/s12/e01-e05-enhancements.md', 'S12-E01'],
  ['docs/s12/e01-e05-enhancements.md', 'Proof Pack'],
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
