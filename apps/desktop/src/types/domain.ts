export type TaskStatus =
  | 'created'
  | 'analyzing'
  | 'planning'
  | 'running'
  | 'validating'
  | 'repairing'
  | 'waitingApproval'
  | 'needsIntervention'
  | 'completed'
  | 'merging'
  | 'merged'
  | 'cancelled'
  | 'failed';

export type TaskType = 'bugfix' | 'test' | 'refactor' | 'explain' | 'custom';

export interface RepositorySummary {
  path: string;
  name: string;
  branch: string;
  dirty: boolean;
}

export interface RepositoryBranchInfo {
  branch: string;
}

export interface RepositoryDirtyStatus {
  dirty: boolean;
}

export interface TaskBranch {
  taskId: string;
  branchName: string;
}

export interface TaskWorktree {
  taskId: string;
  repositoryPath: string;
  worktreePath: string;
  branchName: string;
}

export type WorktreeFileChangeStatus = 'added' | 'modified' | 'deleted';

export interface WorktreeFileChange {
  path: string;
  status: WorktreeFileChangeStatus;
}

export interface WorktreeStatus {
  taskId: string;
  worktreePath: string;
  branchName: string;
  dirty: boolean;
  changes: WorktreeFileChange[];
}

export interface WorktreeCleanupResult {
  taskId: string;
  worktreePath: string;
  removed: boolean;
}

export type CommandRunStatus = 'passed' | 'failed' | 'timedOut' | 'cancelled';

export type CommandOutputStream = 'stdout' | 'stderr';

export interface CommandExecutionResult {
  runId: string;
  taskId: string;
  command: string;
  cwd: string;
  status: CommandRunStatus;
  stdoutPath: string;
  stderrPath: string;
  exitCode?: number | null;
  durationMs: number;
  timedOut: boolean;
  cancelled: boolean;
}

export interface CommandOutputEvent {
  taskId: string;
  runId: string;
  stream: CommandOutputStream;
  chunk: string;
  sequence: number;
  timestampMs: number;
}

export interface CommandFinishedEvent {
  result: CommandExecutionResult;
}

export interface CommandCancelResult {
  runId: string;
  cancelled: boolean;
}

export interface CommandLogPage {
  taskId: string;
  runId: string;
  stream: CommandOutputStream;
  offsetBytes: number;
  nextOffsetBytes: number;
  content: string;
  eof: boolean;
  compressed: boolean;
}

export interface CommandLogSummary {
  taskId: string;
  runId: string;
  sourceStream: CommandOutputStream;
  lines: string[];
  truncated: boolean;
}

export interface LogCleanupResult {
  retentionDays: number;
  scannedFiles: number;
  deletedFiles: number;
  deletedBytes: number;
  cleanupDisabled: boolean;
}

export type AgentTaskPhase =
  | 'created'
  | 'planned'
  | 'editing'
  | 'validating'
  | 'analyzing_error'
  | 'repairing'
  | 'waiting_approval'
  | 'needs_intervention'
  | 'completed'
  | 'failed';

export interface AgentValidationRequest {
  command: string;
  cwd: string;
  reason: string;
  status: 'requested' | 'passed' | 'failed' | 'cancelled' | 'timed_out';
  createdAt: string;
}

export interface AgentStateSnapshot {
  taskId: string;
  repositoryPath: string;
  worktreePath: string;
  title: string;
  description: string;
  phase: AgentTaskPhase;
  validationRequest?: AgentValidationRequest | null;
  validationCandidates?: Array<{
    language: string;
    ecosystem: string;
    command: string;
    reason: string;
    evidence: string[];
    priority: number;
  }>;
  repairRound?: number;
  maxRepairRounds?: number;
}

export interface AgentTaskResponse {
  taskId: string;
  status: 'created' | 'accepted';
  phase: AgentTaskPhase;
  checkpointId: string;
  message?: string;
  state: AgentStateSnapshot;
}

export interface AgentValidationCycleResult {
  taskId: string;
  phase: AgentTaskPhase | string;
  iterations: number;
  commandResults: CommandExecutionResult[];
  state: AgentStateSnapshot;
}

export interface TaskSummary {
  id: string;
  title: string;
  type: TaskType;
  status: TaskStatus;
  repositoryPath: string;
  worktreePath?: string;
  branchName?: string;
  createdAt: string;
  updatedAt: string;
}

export type RiskLevel = 'low' | 'medium' | 'high';

export interface ApprovalSummary {
  id: string;
  taskId: string;
  riskLevel: RiskLevel;
  content: string;
  createdAt: string;
}

