import { invoke } from '@tauri-apps/api/core';

import { normalizeIpcError } from '@/api/errors';
import type {
  CommandCancelResult,
  CommandExecutionResult,
  AgentTaskResponse,
  AgentValidationCycleResult,
  ApprovalDecision,
  ApprovalSummary,
  CommandLogPage,
  CommandLogSummary,
  CommandOutputStream,
  GeneratedTaskDelivery,
  GeneratedTaskDiff,
  GeneratedTaskProofPack,
  QualityGateOverrideResult,
  QualityGateRecord,
  LogCleanupResult,
  ModelConfigView,
  PreparedTaskMerge,
  RepositoryBranchInfo,
  RepositoryDirtyStatus,
  RepositorySummary,
  TaskBranch,
  TaskMergeCommandResult,
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
