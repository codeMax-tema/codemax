import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();

function read(path) {
  return readFileSync(join(root, path), 'utf8');
}

function assertIncludes(file, content, expected) {
  if (!content.includes(expected)) {
    throw new Error(`${file} must include ${expected}`);
  }
}

function assertJsonKey(locale, dictionary, key) {
  if (!(key in dictionary)) {
    throw new Error(`${locale} is missing i18n key: ${key}`);
  }
}

const app = read('apps/desktop/src/app/App.tsx');
const store = read('apps/desktop/src/state/appStore.ts');
const dialog = read('apps/desktop/src/features/tasks/NewTaskDialog.tsx');
const taskOverview = read('apps/desktop/src/features/tasks/TaskOverviewPage.tsx');
const settings = read('apps/desktop/src/features/settings/SettingsPage.tsx');
const approvals = read('apps/desktop/src/features/approvals/ApprovalsPage.tsx');
const tauriClient = read('apps/desktop/src/api/tauriClient.ts');
const css = read('apps/desktop/src/styles/global.css');
const zhCN = JSON.parse(read('apps/desktop/src/i18n/locales/zh-CN.json'));
const enUS = JSON.parse(read('apps/desktop/src/i18n/locales/en-US.json'));

assertIncludes('App.tsx', app, 'NewTaskDialog');
assertIncludes('App.tsx', app, 'SettingsPage');
assertIncludes('App.tsx', app, 'ApprovalsPage');
assertIncludes('appStore.ts', store, 'newTaskDialogOpen');
assertIncludes('appStore.ts', store, 'currentRoute');
assertIncludes('appStore.ts', store, 'getInitialRoute');
assertIncludes('appStore.ts', store, 'getInitialDialogOpen');

for (const marker of [
  'codex-desktop-shell',
  'codex-window-menubar',
  'codex-app-body',
  'codex-thread-sidebar',
  'codex-sidebar-section',
  'codex-sidebar-footer',
  'codex-main-pane',
]) {
  assertIncludes('App.tsx', app, marker);
}

for (const marker of [
  'codex-composer-dialog',
  'codex-composer',
  'composer-toolbar',
  'AGENT',
  'PLAN',
  'ASK',
  'modelStrength',
  'accessPermissions',
  'model-select-trigger',
  'reasoning-control',
  'access-mode-control',
  'mode-control',
  'codex-contract-grid',
  'codex-send-button',
  'workspace-write',
  'command-execution',
  'network-access',
]) {
  assertIncludes('NewTaskDialog.tsx', dialog, marker);
}

for (const marker of [
  'codex-execution-layout',
  'execution-topbar',
  'codex-run-transcript',
  'command-run-card',
  'command-output-block',
  'code-change-panel',
  'execution-code-diff-preview',
  'diff-file-row',
  'environment-panel',
  'execution-followup-composer',
]) {
  assertIncludes('TaskOverviewPage.tsx', taskOverview, marker);
}

for (const marker of [
  'codex-settings-page',
  'settings-return-button',
  'settings-search-box',
  'settings-sidebar',
  'settings-group-heading',
  'settings-detail',
  'settings-toggle-switch',
  'settings.categories.models',
  'saveModelConfig',
  'getModelConfig',
  'api-key-input',
  'api-key-preview',
  'secret-storage-location',
  'testModelConnection',
  'getStorageUsage',
  'cleanupStorage',
  'getStartupHealth',
]) {
  assertIncludes('SettingsPage.tsx', settings, marker);
}

for (const marker of [
  'listPendingApprovals',
  'decideApproval',
  'approval-center-page',
  'approval-card',
  'approved',
  'rejected',
  'revise',
]) {
  assertIncludes('ApprovalsPage.tsx', approvals, marker);
}

for (const marker of [
  'list_pending_approvals',
  'list_task_approvals',
  'decide_approval',
  'save_model_config',
  'get_model_config',
  'generateTaskProofPack',
  'generate_task_proof_pack',
  'recordQualityGateResult',
  'record_quality_gate_result',
  'overrideQualityGate',
  'override_quality_gate',
  'test_model_connection',
  'get_storage_usage',
  'cleanup_storage',
  'get_startup_health',
  'get_app_setting',
  'set_app_setting',
  'ApprovalSummary',
  'ModelConfigView',
  'ModelConnectionTestResult',
  'StorageUsageResponse',
  'StartupHealthResponse',
  'GeneratedTaskProofPack',
  'QualityGateRecord',
]) {
  assertIncludes('tauriClient.ts', tauriClient, marker);
}

for (const marker of [
  'hydratePreferences',
  'ui.locale',
  'ui.theme',
  'ui.compactMode',
  'ui.highContrastMode',
]) {
  assertIncludes('appStore.ts', store, marker);
}

for (const marker of [
  '.codex-desktop-shell',
  '.codex-window-menubar',
  '.codex-app-body',
  '.codex-thread-sidebar',
  '.codex-sidebar-section',
  '.codex-sidebar-footer',
  '.codex-execution-layout',
  '.execution-topbar',
  '.command-run-card',
  '.code-change-panel',
  '.execution-code-diff-preview',
  '.environment-panel',
  '.codex-composer',
  '.codex-composer-dialog',
  '.codex-contract-grid',
  '.codex-send-button',
  '.codex-settings-page',
  '.settings-return-button',
  '.settings-search-box',
  '.settings-group-heading',
  '.settings-toggle-switch',
  '.settings-form-field',
  '.model-secret-form',
  '.secret-storage-location',
  '.settings-diagnostic-list',
  '.settings-status-pill',
  '.settings-usage-grid',
  '.settings-cleanup-result',
  '.settings-byte-value',
  '.permission-toggle',
  '.model-option',
  '.approval-center-page',
  '.approval-card',
  '.approval-risk-pill',
  '.s12-proposal-cards',
  '.s12-screenshots-panel',
  '.s12-proof-pack',
  '.s12-quality-gate',
  '.s12-delivery-score',
  '.s12-risk-radar',
]) {
  assertIncludes('global.css', css, marker);
}

for (const marker of [
  's12-proposal-cards',
  's12-screenshots-panel',
  's12-proof-pack',
  's12-quality-gate',
  's12-delivery-score',
  's12-risk-radar',
]) {
  assertIncludes('TaskOverviewPage.tsx', taskOverview, marker);
}

for (const key of [
  'tasks.new.mode.agent',
  'tasks.new.model.title',
  'tasks.new.modelStrength',
  'tasks.new.accessPermissions',
  'tasks.execution.commands',
  'tasks.execution.codeChanges',
  'tasks.environment.title',
  'settings.categories.models',
  'settings.categories.permissions',
  'settings.categories.modes',
  'settings.categories.storage',
  'settings.models.apiKey',
  'settings.models.apiKeyPreview',
  'settings.models.secretStorage',
  'settings.models.testConnection',
  'settings.storage.usageTitle',
  'settings.storage.cleanupTitle',
  'settings.health.title',
  'approvals.approve',
  'approvals.reject',
  'approvals.revise',
  'approvals.pendingList',
  'tasks.s12.title',
  'tasks.s12.proposals.title',
  'tasks.s12.screenshots.title',
  'tasks.s12.proofPack.title',
  'tasks.s12.qualityGate.title',
  'tasks.s12.deliveryScore.title',
  'tasks.s12.riskRadar.title',
]) {
  assertJsonKey('zh-CN', zhCN, key);
  assertJsonKey('en-US', enUS, key);
}

console.log('S6 UI contract verified');
