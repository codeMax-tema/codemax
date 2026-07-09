import { invoke } from '@tauri-apps/api/core';

import { normalizeIpcError } from '@/api/errors';
import type {
  CommandCancelResult,
  CommandExecutionResult,
  AgentTaskResponse,
  AgentValidationCycleResult,
  ApprovalDecision,
  ApprovalSummary,
  AppSettingValue,
  CleanupStorageRequest,
  CleanupStorageResponse,
  CommandLogPage,
  CommandLogSummary,
  CommandOutputStream,
  ContextSource,
  ContractBreachRecord,
  DeliveryReviewState,
  GeneratedTaskDelivery,
  GeneratedTaskDiff,
  GeneratedTaskProofPack,
  ActiveProfile,
  HookApprovalRecord,
  HookRunRecord,
  PreferenceCandidate,
  PrivacyLedgerEntry,
  PrivacyPreview,
  PrivacyLedgerSummary,
  QualityGateOverrideResult,
  QualityGateRecord,
  ModelArenaDecisionRecord,
  LogCleanupResult,
  ModelConnectionTestResult,
  ModelConfigView,
  PreparedTaskMerge,
  RepositoryBranchInfo,
  RepositoryDirtyStatus,
  RepositorySummary,
  RuleHitRecord,
  RunContract,
  RunContractPreview,
  StartupHealthResponse,
  StorageUsageResponse,
  TaskMemoryUsage,
  TaskBranch,
  TaskDetail,
  TokenBudgetSummary,
  TaskMergeCommandResult,
  TaskSummary,
  TaskType,
  TaskWorktree,
  WorktreeCleanupResult,
  WorktreeStatus,
} from '@/types/domain';

export type InvokeParams = Record<string, unknown>;

export async function invokeCommand<TResponse>(
  command: string,
  params: InvokeParams = {},
): Promise<TResponse> {
  try {
    return await invoke<TResponse>(command, params);
  } catch (error) {
    throw normalizeIpcError(error);
  }
}

export interface HealthResponse {
  service: string;
  status: 'ok';
  version: string;
}

export function getDesktopHealth() {
  return invokeCommand<HealthResponse>('health');
}

export interface PingResponse {
  message: string;
}

export function pingDesktop() {
  return invokeCommand<PingResponse>('ping');
}

export interface StorageRootsResponse {
  appDataDir: string;
  artifactRoot: string;
  worktreeRoot: string;
  databasePath: string;
}

export function getStorageRoots() {
  return invokeCommand<StorageRootsResponse>('get_storage_roots');
}

export function getStorageUsage() {
  return invokeCommand<StorageUsageResponse>('get_storage_usage');
}

export function cleanupStorage(request: CleanupStorageRequest) {
  return invokeCommand<CleanupStorageResponse>('cleanup_storage', { request });
}

export function getStartupHealth() {
  return invokeCommand<StartupHealthResponse>('get_startup_health');
}

export function getAppSetting(key: string) {
  return invokeCommand<AppSettingValue>('get_app_setting', { key });
}

export function setAppSetting(key: string, value: string) {
  return invokeCommand<AppSettingValue>('set_app_setting', { request: { key, value } });
}

export function emitAppReady() {
  return invokeCommand<void>('emit_app_ready');
}

export interface ExecuteTaskCommandRequest {
  taskId: string;
  runId?: string;
  command: string;
  cwd: string;
  env?: Record<string, string>;
  timeoutMs?: number;
}

export function executeTaskCommand(request: ExecuteTaskCommandRequest) {
  return invokeCommand<CommandExecutionResult>('execute_task_command', { request });
}

export function cancelTaskCommand(runId: string) {
  return invokeCommand<CommandCancelResult>('cancel_task_command', { runId });
}

export interface ReadCommandLogRequest {
  taskId: string;
  runId: string;
  stream: CommandOutputStream;
  offsetBytes?: number;
  maxBytes?: number;
}

export function readTaskCommandLog(request: ReadCommandLogRequest) {
  return invokeCommand<CommandLogPage>('read_task_command_log', { request });
}

export interface CommandLogSummaryRequest {
  taskId: string;
  runId: string;
  maxLines?: number;
}

export function summarizeTaskCommandLog(request: CommandLogSummaryRequest) {
  return invokeCommand<CommandLogSummary>('summarize_task_command_log', { request });
}

export function cleanupExpiredTaskLogs() {
  return invokeCommand<LogCleanupResult>('cleanup_expired_task_logs');
}

export interface SaveModelConfigRequest {
  id?: string;
  provider: string;
  baseUrl: string;
  modelName: string;
  apiKey?: string;
  clearApiKey?: boolean;
}

export function getModelConfig(id = 'model-default') {
  return invokeCommand<ModelConfigView | null>('get_model_config', { id });
}

export function saveModelConfig(request: SaveModelConfigRequest) {
  return invokeCommand<ModelConfigView>('save_model_config', { request });
}

export function testModelConnection(id = 'model-default') {
  return invokeCommand<ModelConnectionTestResult>('test_model_connection', { id });
}

export function getActiveProfile() {
  return invokeCommand<ActiveProfile>('active_profile');
}

export interface ProfileCreateRequest {
  id?: string;
  name: string;
  scope?: string;
  scopeId?: string;
  mode?: string;
  modelId?: string;
  reasoningEffort?: string;
  permissionLevel?: string;
  networkPolicy?: string;
  privacyMode?: string;
  tokenBudgetTotal?: number;
  tokenBudgetPerCall?: number;
  validationPolicy?: string;
  outputLanguage?: string;
  memoryScope?: string;
  qualityGatePolicy?: string;
  activate?: boolean;
}

export interface ProfileUpdateRequest extends Partial<ProfileCreateRequest> {
  profileId: string;
  clearScopeId?: boolean;
  clearModelId?: boolean;
}

export function listProfiles() {
  return invokeCommand<ActiveProfile[]>('profile_list');
}

export function createProfile(request: ProfileCreateRequest) {
  return invokeCommand<ActiveProfile>('profile_create', { request });
}

export function updateProfile(request: ProfileUpdateRequest) {
  return invokeCommand<ActiveProfile>('profile_update', { request });
}

export function activateProfile(profileId: string) {
  return invokeCommand<ActiveProfile>('profile_activate', { profileId });
}

export interface TaskStartPreviewRequest {
  repositoryPath: string;
  title?: string;
  description: string;
  modelId?: string;
  validationCommand?: string;
}

export function getPrivacyPreview(request: TaskStartPreviewRequest) {
  return invokeCommand<PrivacyPreview>('privacy_preview', { request });
}

export function getRunContractPreview(request: TaskStartPreviewRequest) {
  return invokeCommand<RunContractPreview>('run_contract_preview', { request });
}

export function getRunContract(taskId: string) {
  return invokeCommand<RunContract | null>('run_contract', { taskId });
}

export function getPrivacyLedgerSummary(taskId: string) {
  return invokeCommand<PrivacyLedgerSummary>('privacy_ledger_summary', { taskId });
}

export function getPrivacyLedgerEntries(taskId: string) {
  return invokeCommand<PrivacyLedgerEntry[]>('privacy_ledger_entries', { taskId });
}

export function getTokenBudgetSummary(taskId: string) {
  return invokeCommand<TokenBudgetSummary>('token_budget_summary', { taskId });
}

export function getContextSources(taskId: string) {
  return invokeCommand<ContextSource[]>('context_sources', { taskId });
}

export function getContractBreachRecords(taskId: string) {
  return invokeCommand<ContractBreachRecord[]>('contract_breach_records', { taskId });
}

export interface RecordContractBreachRequest {
  taskId: string;
  breachType: string;
  requestedValue: string;
  policyValue: string;
  reason?: string;
  status?: string;
}

export function recordContractBreach(request: RecordContractBreachRequest) {
  return invokeCommand<ContractBreachRecord>('record_contract_breach', { request });
}

export function getMemoryUsedByTask(taskId: string) {
  return invokeCommand<TaskMemoryUsage[]>('memory_used_by_task', { taskId });
}

export interface RecordMemoryUsageRequest {
  taskId: string;
  memoryId?: string;
  memoryKey: string;
  memoryScope: string;
  memoryScopeId?: string;
  usageType: string;
  value: string;
}

export function recordMemoryUsedByTask(request: RecordMemoryUsageRequest) {
  return invokeCommand<TaskMemoryUsage>('record_memory_used_by_task', { request });
}

export interface PreferenceCandidatesRequest {
  taskId?: string;
}

export function getPreferenceCandidates(request: PreferenceCandidatesRequest = {}) {
  return invokeCommand<PreferenceCandidate[]>('preference_candidates', { request });
}

export interface CreatePreferenceCandidateRequest {
  taskId?: string;
  scope: string;
  scopeId?: string;
  preferenceKey: string;
  candidateValue: string;
  evidence?: string;
  confidence?: number;
}

export function createPreferenceCandidate(request: CreatePreferenceCandidateRequest) {
  return invokeCommand<PreferenceCandidate>('preference_candidate_create', { request });
}

export interface DecidePreferenceCandidateRequest {
  candidateId: string;
  decision: string;
  editedValue?: string;
  comment?: string;
}

export function decidePreferenceCandidate(request: DecidePreferenceCandidateRequest) {
  return invokeCommand<PreferenceCandidate>('preference_candidate_decide', { request });
}

export function listPendingApprovals() {
  return invokeCommand<ApprovalSummary[]>('list_pending_approvals');
}

export interface ListTaskApprovalsRequest {
  taskId: string;
}

export function listTaskApprovals(request: ListTaskApprovalsRequest) {
  return invokeCommand<ApprovalSummary[]>('list_task_approvals', { request });
}

export interface DecideApprovalRequest {
  approvalId: string;
  decision: ApprovalDecision;
  comment?: string;
}

export function decideApproval(request: DecideApprovalRequest) {
  return invokeCommand<ApprovalSummary>('decide_approval', { request });
}

export interface CreateAgentTaskRequest {
  taskId: string;
  repositoryPath: string;
  worktreePath: string;
  title: string;
  description?: string;
  modelId?: string | null;
  validationCommand?: string | null;
}

export function createAgentTask(request: CreateAgentTaskRequest) {
  return invokeCommand<AgentTaskResponse>('create_agent_task', { request });
}

export function getAgentTaskState(taskId: string) {
  return invokeCommand<AgentTaskResponse['state']>('get_agent_task_state', { taskId });
}

export interface AdvanceAgentTaskRequest {
  reason?: string | null;
  userMessage?: string | null;
  requireApproval?: boolean;
}

export function advanceAgentTask(taskId: string, request: AdvanceAgentTaskRequest = {}) {
  return invokeCommand<AgentTaskResponse>('advance_agent_task', { taskId, request });
}

export interface SubmitAgentValidationResultRequest {
  runId?: string | null;
  command?: string | null;
  cwd?: string | null;
  stdout?: string;
  stderr?: string;
  exitCode?: number | null;
  timedOut?: boolean;
  cancelled?: boolean;
}

export function submitAgentValidationResult(
  taskId: string,
  request: SubmitAgentValidationResultRequest,
) {
  return invokeCommand<AgentTaskResponse>('submit_agent_validation_result', { taskId, request });
}

export interface RunAgentValidationCycleRequest {
  taskId: string;
  reason?: string | null;
  timeoutMs?: number;
  maxIterations?: number;
}

export function runAgentValidationCycle(request: RunAgentValidationCycleRequest) {
  return invokeCommand<AgentValidationCycleResult>('run_agent_validation_cycle', { request });
}

export interface RepositoryPathSelection {
  path: string;
}

export function selectRepositoryPath() {
  return invokeCommand<RepositoryPathSelection | null>('select_repository_path');
}

export interface RepositoryPathRequest {
  path: string;
}

export function validateRepositoryPath(path: string) {
  return invokeCommand<RepositorySummary>('validate_repository_path', { path });
}

export function getRepositoryCurrentBranch(path: string) {
  return invokeCommand<RepositoryBranchInfo>('get_repository_current_branch', { path });
}

export function getRepositoryDirtyStatus(path: string) {
  return invokeCommand<RepositoryDirtyStatus>('get_repository_dirty_status', { path });
}

export interface CreateTaskRecordRequest {
  repositoryPath: string;
  description: string;
  title?: string | null;
  taskType?: TaskType | null;
  modelId?: string | null;
  validationCommand?: string | null;
}

export function createTaskRecord(request: CreateTaskRecordRequest) {
  return invokeCommand<TaskSummary>('create_task_record', { request });
}

export interface ListTasksRequest {
  repositoryPath?: string | null;
  status?: string | null;
  limit?: number;
}

export function listTasks(request: ListTasksRequest = {}) {
  return invokeCommand<TaskSummary[]>('list_tasks', { request });
}

export function getTaskRecord(taskId: string) {
  return invokeCommand<TaskSummary>('get_task_record', { taskId });
}

export function getTaskDetail(taskId: string) {
  return invokeCommand<TaskDetail>('get_task_detail', { taskId });
}

export function createTaskBranch(repositoryPath: string, taskId: string) {
  return invokeCommand<TaskBranch>('create_task_branch', { repositoryPath, taskId });
}

export function createTaskWorktree(taskId: string) {
  return invokeCommand<TaskWorktree>('create_task_worktree', { taskId });
}

export function getTaskWorktreeStatus(taskId: string) {
  return invokeCommand<WorktreeStatus>('get_task_worktree_status', { taskId });
}

export interface GenerateTaskDiffRequest {
  taskId: string;
  baseRef?: string | null;
}

export function generateTaskDiff(request: GenerateTaskDiffRequest) {
  return invokeCommand<GeneratedTaskDiff>('generate_task_diff', { request });
}

export interface GenerateTaskDeliveryRequest {
  taskId: string;
}

export function generateTaskDelivery(request: GenerateTaskDeliveryRequest) {
  return invokeCommand<GeneratedTaskDelivery>('generate_task_delivery', { request });
}

export interface GenerateTaskProofPackRequest {
  taskId: string;
}

export function generateTaskProofPack(request: GenerateTaskProofPackRequest) {
  return invokeCommand<GeneratedTaskProofPack>('generate_task_proof_pack', { request });
}

export interface GetDeliveryReviewStateRequest {
  taskId: string;
}

export function getDeliveryReviewState(request: GetDeliveryReviewStateRequest) {
  return invokeCommand<DeliveryReviewState>('get_delivery_review_state', { request });
}

export interface RecordQualityGateRequest {
  taskId: string;
  gateType: string;
  status: string;
  message: string;
  evidencePath?: string | null;
}

export function recordQualityGateResult(request: RecordQualityGateRequest) {
  return invokeCommand<QualityGateRecord>('record_quality_gate_result', { request });
}

export interface OverrideQualityGateRequest {
  taskId: string;
  gateType: string;
  reason: string;
}

export function overrideQualityGate(request: OverrideQualityGateRequest) {
  return invokeCommand<QualityGateOverrideResult>('override_quality_gate', { request });
}

export interface RecordRuleHitRequest {
  taskId: string;
  rule: string;
  status: string;
  message: string;
  evidencePath?: string | null;
}

export function recordRuleHit(request: RecordRuleHitRequest) {
  return invokeCommand<RuleHitRecord>('record_rule_hit', { request });
}

export interface RecordHookRunRequest {
  taskId: string;
  hook: string;
  lifecycle: string;
  status: string;
  message: string;
  command?: string | null;
  evidencePath?: string | null;
  approvalId?: string | null;
}

export function recordHookRun(request: RecordHookRunRequest) {
  return invokeCommand<HookRunRecord>('record_hook_run', { request });
}

export interface RequestHookApprovalRequest {
  taskId: string;
  hook: string;
  reason: string;
}

export function requestHookApproval(request: RequestHookApprovalRequest) {
  return invokeCommand<HookApprovalRecord>('request_hook_approval', { request });
}

export interface ResolveHookApprovalRequest {
  taskId: string;
  approvalId: string;
  approved: boolean;
  reviewer?: string | null;
  reason: string;
}

export function resolveHookApproval(request: ResolveHookApprovalRequest) {
  return invokeCommand<HookApprovalRecord>('resolve_hook_approval', { request });
}

export interface RecordModelArenaDecisionRequest {
  taskId: string;
  status: string;
  selectedModel?: string | null;
  selectedProposalId?: string | null;
  rationale: string;
  comparedModels: string[];
}

export function recordModelArenaDecision(request: RecordModelArenaDecisionRequest) {
  return invokeCommand<ModelArenaDecisionRecord>('record_model_arena_decision', { request });
}

export interface PrepareTaskMergeRequest {
  taskId: string;
  targetBranch?: string | null;
}

export function prepareTaskMerge(request: PrepareTaskMergeRequest) {
  return invokeCommand<PreparedTaskMerge>('prepare_task_merge', { request });
}

export interface MergeTaskRequest {
  taskId: string;
  targetBranch?: string | null;
  commitMessage: string;
  confirmed: boolean;
}

export function mergeTask(request: MergeTaskRequest) {
  return invokeCommand<TaskMergeCommandResult>('merge_task', { request });
}

export function cleanupTaskWorktree(taskId: string, confirmed: boolean) {
  return invokeCommand<WorktreeCleanupResult>('cleanup_task_worktree', { taskId, confirmed });
}
