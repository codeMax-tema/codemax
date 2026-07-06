import { invoke } from '@tauri-apps/api/core';

import { normalizeIpcError } from '@/api/errors';
import type {
  RepositoryBranchInfo,
  RepositoryDirtyStatus,
  RepositorySummary,
  TaskBranch,
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

export function cleanupTaskWorktree(taskId: string, confirmed: boolean) {
  return invokeCommand<WorktreeCleanupResult>('cleanup_task_worktree', { taskId, confirmed });
}
