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

export interface TaskDiffFile {
  path: string;
  status: WorktreeFileChangeStatus;
  additions: number;
  deletions: number;
  patch: string;
}

export interface GeneratedTaskDiff {
  taskId: string;
  baseRef: string;
  worktreePath: string;
  branchName: string;
  artifactId: string;
  diffPath: string;
  files: TaskDiffFile[];
  additions: number;
  deletions: number;
  patch: string;
}

export type DeliveryReportStatus = 'passed' | 'failed' | 'notRun';

export interface TaskValidationRunSummary {
  runId: string;
  command: string;
  cwd: string;
  status: CommandRunStatus;
  exitCode?: number | null;
  durationMs?: number | null;
  createdAt: string;
}

export interface TaskDeliveryReport {
  taskId: string;
  artifactId: string;
  taskTitle: string;
  generatedAt: string;
  overallStatus: DeliveryReportStatus;
  summary: string;
  commandCount: number;
  passedCount: number;
  failedCount: number;
  changedFiles: string[];
  diffPath?: string | null;
  deliveryPath: string;
  runs: TaskValidationRunSummary[];
  risk: string;
}

export type TaskProofStatus = 'passed' | 'warning' | 'blocked';

export interface TaskProofPackProposal {
  id: string;
  titleKey: string;
  summaryKey: string;
  status: TaskProofStatus;
  confidence: number;
}

export interface TaskProofPackScreenshot {
  id: string;
  titleKey: string;
  path: string;
  capturedAt: string;
  status: TaskProofStatus;
}

export interface TaskProofPackGate {
  id: string;
  titleKey: string;
  summaryKey: string;
  status: TaskProofStatus;
}

export interface TaskProofPackRisk {
  id: string;
  titleKey: string;
  summaryKey: string;
  level: RiskLevel;
}

export interface QualityGateRecord {
  id: string;
  taskId: string;
  gateType: string;
  status: string;
  message: string;
  evidencePath?: string | null;
  overrideReason?: string | null;
  createdAt: string;
}

export interface QualityGateOverrideResult {
  taskId: string;
  gateType: string;
  overriddenCount: number;
  reason: string;
}

export interface TaskProofPackScore {
  value: number;
  grade: string;
  summaryKey: string;
}

export interface GeneratedTaskProofPack {
  taskId: string;
  artifactId: string;
  generatedAt: string;
  proofPackPath: string;
  summaryKey: string;
  deliveryScore: TaskProofPackScore;
  proposals: TaskProofPackProposal[];
  screenshots: TaskProofPackScreenshot[];
  qualityGates: TaskProofPackGate[];
  risks: TaskProofPackRisk[];
}

export interface GeneratedTaskDelivery {
  taskId: string;
  artifactId: string;
  reportPath: string;
  deliveryPath: string;
  diffPath?: string | null;
  summary: string;
  commitMessage: string;
  report: TaskDeliveryReport;
}

export type TaskMergeResultStatus = 'merged' | 'conflicted';

export interface PreparedTaskMerge {
  taskId: string;
  targetBranch: string;
  sourceBranch: string;
  worktreePath: string;
  targetDirty: boolean;
  worktreeDirty: boolean;
  validationStatus: DeliveryReportStatus;
  validationRunCount: number;
  validationSummary: string;
  diffFileCount: number;
  additions: number;
  deletions: number;
  diffPath?: string | null;
  commitMessage: string;
  blockers: string[];
  canMerge: boolean;
}

export interface TaskMergeCommandResult {
  taskId: string;
  status: TaskMergeResultStatus;
  targetBranch: string;
  sourceBranch: string;
  commitSha: string;
  commitMessage: string;
  conflictFiles: string[];
  errorReason?: string | null;
  mergeRecordPath?: string | null;
  taskStatus: TaskStatus;
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

export interface ModelConfigView {
  id: string;
  provider: string;
  baseUrl: string;
  modelName: string;
  apiKeyConfigured: boolean;
  apiKeyMasked?: string | null;
  secretStorage?: string | null;
  secretLocation?: string | null;
  createdAt: string;
  updatedAt: string;
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
  approvalType?: string;
  riskLevel: RiskLevel;
  content: string;
  reason?: string;
  decision?: ApprovalDecision | null;
  comment?: string | null;
  createdAt: string;
  decidedAt?: string | null;
}

export type ApprovalDecision = 'approved' | 'rejected' | 'revise';

