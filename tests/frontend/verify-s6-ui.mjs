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
const skills = read('apps/desktop/src/features/skills/SkillsPage.tsx');
const settings = read('apps/desktop/src/features/settings/SettingsPage.tsx');
const home = read('apps/desktop/src/features/home/HomePage.tsx');
const search = read('apps/desktop/src/features/search/SearchPage.tsx');
const approvals = read('apps/desktop/src/features/approvals/ApprovalsPage.tsx');
const tauriClient = read('apps/desktop/src/api/tauriClient.ts');
const css = read('apps/desktop/src/styles/global.css');
const zhCN = JSON.parse(read('apps/desktop/src/i18n/locales/zh-CN.json'));
const enUS = JSON.parse(read('apps/desktop/src/i18n/locales/en-US.json'));

assertIncludes('App.tsx', app, 'NewTaskDialog');
assertIncludes('App.tsx', app, 'SettingsPage');
assertIncludes('App.tsx', app, 'ApprovalsPage');
assertIncludes('App.tsx', app, 'SkillsPage');
assertIncludes('App.tsx', app, 'HomePage');
assertIncludes('App.tsx', app, 'SearchPage');
assertIncludes('appStore.ts', store, 'newTaskDialogOpen');
assertIncludes('appStore.ts', store, 'currentRoute');
assertIncludes('appStore.ts', store, 'getInitialRoute');
assertIncludes('appStore.ts', store, 'getInitialDialogOpen');
assertIncludes('appStore.ts', store, "'home'");
assertIncludes('appStore.ts', store, "'search'");

for (const marker of [
  'codex-desktop-shell',
  'codex-window-menubar',
  'codex-app-body',
  'codex-thread-sidebar',
  'codex-sidebar-section',
  'codex-sidebar-footer',
  'codex-main-pane',
  'codemax-minimal-shell',
  'search-command-palette',
]) {
  assertIncludes('App.tsx', app, marker);
}

for (const marker of [
  'home-page',
  'home-prompt-title',
  'home-composer-shell',
  'home-project-row',
  'home-model-trigger',
  'home-send-button',
  'tasks.new.submit',
  'repository.emptyShort',
]) {
  assertIncludes('HomePage.tsx', home, marker);
}

for (const marker of [
  'search-page',
  'search-command-palette',
  'search-result-list',
  'search-empty-state',
  'listTasks',
  'setSearchQuery',
]) {
  assertIncludes('SearchPage.tsx', search, marker);
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
  'codex-preview-grid',
  'privacy-preview-card',
  'contract-preview-card',
  'getPrivacyPreview',
  'getRunContractPreview',
  'codex-send-button',
  'workspace-write',
  'command-execution',
  'network-access',
  'workMode',
  'workspaceStrategy',
  'currentRepository.isGitRepository',
  'originalWriteAuthorized',
  'workspaceExclusions',
  'estimateTaskWorkspace',
  'workspaceEstimate',
  'workspace-strategy-control',
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
  'directory-mode-note',
  'execution-code-diff-preview',
  'diff-file-row',
  'environment-panel',
  'execution-followup-composer',
  'task-approvals-panel',
  'task-approval-card',
  'decideApproval',
  'taskRecord.workspaceKind',
  'taskRecord.sourcePath',
  'taskRecord.originalWriteAuthorized',
  'taskRecord.workspaceEstimatedBytes',
]) {
  assertIncludes('TaskOverviewPage.tsx', taskOverview, marker);
}

for (const marker of [
  'skills-page',
  'skills-source-tabs',
  'skills-list',
  'getSkillSources',
  'useDeferredValue',
  'skills-search-input',
  'setSearchQuery',
  'filteredEntries',
  'skills-no-results',
  'skills-source-status',
  'skills-count-badge',
  'skills-entry-list',
  'skills-entry-row',
  'skill.description',
  '.codemax/skills',
  'skills.source.workspace',
  'skills.source.project',
  'skills.source.global',
  'skills.source.builtIn',
]) {
  assertIncludes('SkillsPage.tsx', skills, marker);
}

for (const marker of [
  'codex-settings-page',
  'settings-return-button',
  'settings-search-box',
  'settings-sidebar',
  'settings-group-heading',
  'settings-detail',
  'settings-toggle-switch',
  'settings-thinking-strength-page',
  'thinking-strength-slider',
  'thinking-strength-meter-grid',
  'thinking-strength-allow-override',
  'settings.categories.models',
  'settings.categories.thinking',
  'saveModelConfig',
  'getModelConfig',
  'api-key-input',
  'api-key-preview',
  'secret-storage-location',
  'testModelConnection',
  'getStorageUsage',
  'cleanupStorage',
  'getStartupHealth',
  'profileList',
  'getActiveProfile',
  'getPreferenceCandidates',
  'listMemoryItems',
  'saveMemoryItem',
  'deleteMemoryItem',
  'memory-cockpit-grid',
  'settings-profile-list',
  'preference-candidate-list',
  'memory-item-list',
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
  'memory_items',
  'save_memory_item',
  'delete_memory_item',
  'save_model_config',
  'get_model_config',
  'generateTaskProofPack',
  'generate_task_proof_pack',
  'recordQualityGateResult',
  'record_quality_gate_result',
  'overrideQualityGate',
  'override_quality_gate',
  'recordRuleHit',
  'record_rule_hit',
  'recordHookRun',
  'record_hook_run',
  'requestHookApproval',
  'request_hook_approval',
  'resolveHookApproval',
  'resolve_hook_approval',
  'recordModelArenaDecision',
  'record_model_arena_decision',
  'test_model_connection',
  'get_storage_usage',
  'cleanup_storage',
  'get_startup_health',
  'get_app_setting',
  'set_app_setting',
  'getSkillSources',
  'get_skill_sources',
  'ApprovalSummary',
  'ModelConfigView',
  'ModelConnectionTestResult',
  'StorageUsageResponse',
  'StartupHealthResponse',
  'MemoryItem',
  'GeneratedTaskProofPack',
  'QualityGateRecord',
  'RuleHitRecord',
  'HookRunRecord',
  'HookApprovalRecord',
  'ModelArenaDecisionRecord',
]) {
  assertIncludes('tauriClient.ts', tauriClient, marker);
}

for (const marker of [
  'hydratePreferences',
  'ui.locale',
  'ui.theme',
  'ui.compactMode',
  'ui.highContrastMode',
  'ui.workMode',
  'setWorkMode',
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
  '.codemax-minimal-shell',
  '.home-page',
  '.home-composer-shell',
  '.home-project-row',
  '.search-command-palette',
  '.search-result-list',
  '.settings-return-button',
  '.settings-search-box',
  '.settings-group-heading',
  '.settings-toggle-switch',
  '.settings-thinking-strength-page',
  '.thinking-strength-slider',
  '.thinking-strength-meter-grid',
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
  '.workspace-strategy-control',
  '.workspace-exclusions-field',
  '.workspace-estimate-grid',
  '.memory-cockpit-grid',
  '.settings-profile-list',
  '.preference-candidate-list',
  '.memory-item-list',
  '.task-approvals-panel',
  '.task-approval-card',
  '.approval-center-page',
  '.approval-card',
  '.approval-risk-pill',
  '.s12-proposal-cards',
  '.s12-screenshots-panel',
  '.s12-proof-pack',
  '.s12-quality-gate',
  '.s12-delivery-score',
  '.s12-risk-radar',
  '.s12-rules-panel',
  '.s12-hooks-panel',
  '.s12-model-arena-panel',
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
  's12-rules-panel',
  's12-hooks-panel',
  's12-model-arena-panel',
]) {
  assertIncludes('TaskOverviewPage.tsx', taskOverview, marker);
}

for (const key of [
  'tasks.new.mode.agent',
  'sidebar.skills',
  'skills.title',
  'skills.subtitle',
  'skills.emptyTitle',
  'skills.emptyHint',
  'skills.noMatchesTitle',
  'skills.noMatchesHint',
  'skills.source.workspace',
  'skills.source.project',
  'skills.source.global',
  'skills.source.builtIn',
  'skills.status.ready',
  'skills.status.missing',
  'skills.status.unavailable',
  'skills.status.virtual',
  'skills.countLabel',
  'skills.note.selectProject',
  'home.promptTitle',
  'home.placeholder',
  'home.chooseProject',
  'search.title',
  'search.placeholder',
  'search.emptyTitle',
  'search.emptyHint',
  'tasks.new.model.title',
  'tasks.new.modelStrength',
  'tasks.new.accessPermissions',
  'tasks.execution.commands',
  'tasks.execution.codeChanges',
  'tasks.execution.directoryMode',
  'tasks.execution.gitUnavailable',
  'tasks.execution.workspaceKind',
  'tasks.execution.sourcePath',
  'tasks.execution.originalWriteAuthorized',
  'tasks.execution.workspaceEstimatedBytes',
  'tasks.environment.title',
  'settings.categories.models',
  'settings.categories.thinking',
  'settings.categories.permissions',
  'settings.categories.modes',
  'settings.categories.storage',
  'settings.thinking.title',
  'settings.thinking.subtitle',
  'settings.thinking.minimal',
  'settings.thinking.low',
  'settings.thinking.medium',
  'settings.thinking.high',
  'settings.thinking.max',
  'settings.thinking.depth',
  'settings.thinking.contextBudget',
  'settings.thinking.validation',
  'settings.thinking.repair',
  'settings.thinking.speed',
  'settings.thinking.cost',
  'settings.thinking.benefit',
  'settings.thinking.tradeoff',
  'settings.thinking.bestFor',
  'settings.models.apiKey',
  'settings.models.apiKeyPreview',
  'settings.models.secretStorage',
  'settings.models.testConnection',
  'settings.storage.usageTitle',
  'settings.storage.cleanupTitle',
  'settings.memory.activeProfile',
  'settings.memory.profiles',
  'settings.memory.preferenceCandidates',
  'settings.memory.savedMemories',
  'settings.memory.addMemory',
  'settings.memory.emptyMemories',
  'settings.memory.emptyCandidates',
  'settings.memory.activateProfile',
  'settings.memory.saveMemory',
  'settings.memory.deleteMemory',
  'settings.health.title',
  'settings.health.agentStopped',
  'settings.health.agentDirectoryMissing',
  'settings.health.agentUnavailable',
  'settings.general.programming',
  'settings.general.everyday',
  'tasks.new.workspace.gitInit',
  'tasks.new.workspace.isolatedCopy',
  'tasks.new.workspace.directOriginal',
  'tasks.new.workspace.directOriginalWarning',
  'tasks.new.workspace.exclusions',
  'tasks.new.workspace.exclusionsHint',
  'tasks.new.workspace.estimatedSize',
  'tasks.new.workspace.availableSpace',
  'tasks.new.workspace.cleanup.remove_workspace_keep_source',
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
  'tasks.s12.privacy.title',
  'tasks.s12.contract.title',
  'tasks.s12.tokenBudget.title',
  'tasks.s12.proofFiles.title',
  'tasks.s12.rules.title',
  'tasks.s12.hooks.title',
  'tasks.s12.modelArena.title',
  'tasks.s12.status.generated',
  'tasks.s12.status.missing',
  'tasks.s12.status.empty',
]) {
  assertJsonKey('zh-CN', zhCN, key);
  assertJsonKey('en-US', enUS, key);
}

console.log('S6 UI contract verified');
