export type TaskStatus =
  | 'queued'
  | 'planning'
  | 'editing'
  | 'validating'
  | 'repairing'
  | 'awaitingApproval'
  | 'awaitingReview'
  | 'readyToMerge'
  | 'needsIntervention'
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
  purpose?: CommandRunPurpose;
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

export type DeliveryReviewStatus = 'passed' | 'warning' | 'blocked';
export type ProofPackStatus = 'generated' | 'missing';

export interface RiskFinding {
  kind: string;
  level: RiskLevel;
  subject: string;
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

export interface QualityGateResultState {
  status: DeliveryReviewStatus;
  gates: TaskProofPackGate[];
  blockers: string[];
}

export interface DeliveryScoreState {
  value: number;
  grade: string;
  testScore: number;
  riskScore: number;
  diffScore: number;
  approvalScore: number;
  explanation: string;
  riskLevel: RiskLevel;
  createdAt?: string | null;
}

export interface TaskCapsuleState {
  taskId: string;
  changedFiles: string[];
  commandCount: number;
  diffPath?: string | null;
  deliveryPath?: string | null;
  manifestPath?: string | null;
  summaryPath?: string | null;
  capsulePath?: string | null;
}

export interface RuleHitState {
  id: string;
  rule: string;
  status: string;
  message: string;
  evidencePath?: string | null;
}

export interface HookRunState {
  id: string;
  hook: string;
  status: string;
  message: string;
  evidencePath?: string | null;
}

export interface ModelArenaDecisionState {
  status: string;
  selectedModel?: string | null;
  reason: string;
}

export interface DeliveryReviewState {
  taskId: string;
  taskStatus: TaskStatus | string;
  status: DeliveryReviewStatus;
  canMerge: boolean;
  blockers: string[];
  validationStatus: DeliveryReportStatus | 'cancelled' | 'timedOut';
  diffFileCount: number;
  approvalBlocked: boolean;
  highestRiskLevel: RiskLevel;
  proofPackStatus: ProofPackStatus;
  proofPackId?: string | null;
  proofPackPath?: string | null;
  qualityGateResult: QualityGateResultState;
  deliveryScore: DeliveryScoreState;
  riskRecords: RiskFinding[];
  taskCapsule: TaskCapsuleState;
  ruleHits: RuleHitState[];
  hookRuns: HookRunState[];
  modelArenaDecision: ModelArenaDecisionState;
  updatedAt: string;
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
  qualityGateResult: QualityGateResultState;
  deliveryScore: DeliveryScoreState;
  proofPackPath?: string | null;
  deliveryReviewState: DeliveryReviewState;
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

export interface MergePreview {
  targetBranch: string;
  sourceBranch?: string | null;
  status: string;
  canMerge: boolean;
  blockers: string[];
  recordPath?: string | null;
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
export type CommandRunPurpose = 'validation' | 'edit' | 'diagnostic';

export type CommandOutputStream = 'stdout' | 'stderr';

export interface CommandExecutionResult {
  runId: string;
  taskId: string;
  purpose?: CommandRunPurpose;
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

export interface AppSettingValue {
  key: string;
  value?: string | null;
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

export interface ModelConnectionTestResult {
  status: 'ready' | 'warning' | 'blocked';
  provider: string;
  modelName: string;
  baseUrlHost?: string | null;
  latencyMs: number;
  messageKey: string;
  detail?: string | null;
}

export interface StorageUsageResponse {
  appDataDir: string;
  databasePath: string;
  artifactRoot: string;
  worktreeRoot: string;
  databaseBytes: number;
  artifactBytes: number;
  worktreeBytes: number;
  logsBytes: number;
  screenshotsBytes: number;
  temporaryContextBytes: number;
  permanentEvidenceBytes: number;
  totalBytes: number;
}

export interface CleanupStorageRequest {
  logs: boolean;
  screenshots: boolean;
  temporaryContext: boolean;
  dryRun: boolean;
}

export interface CleanupStorageResponse {
  dryRun: boolean;
  scannedFiles: number;
  deletedFiles: number;
  deletedBytes: number;
  protectedBytes: number;
}

export type StartupHealthStatus = 'ready' | 'degraded' | 'blocked';
export type StartupHealthItemStatus = 'ready' | 'warning' | 'blocked';

export interface StartupHealthItem {
  key: string;
  status: StartupHealthItemStatus;
  messageKey: string;
  detail?: string | null;
}

export interface StartupHealthResponse {
  status: StartupHealthStatus;
  items: StartupHealthItem[];
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
  description: string;
  type: TaskType;
  status: TaskStatus;
  taskStatus: TaskStatus;
  repositoryId: string;
  repositoryPath: string;
  worktreePath?: string | null;
  branchName?: string | null;
  taskBranch?: string | null;
  targetBranch: string;
  agentStage: string;
  latestValidationStatus: DeliveryReportStatus | 'cancelled' | 'timedOut';
  latestDiffSummary: string;
  mergePreview: MergePreview;
  createdAt: string;
  updatedAt: string;
}

export interface TaskTodo {
  id: string;
  taskId: string;
  title: string;
  description: string;
  status: string;
  startedAt?: string | null;
  completedAt?: string | null;
  errorMessage?: string | null;
}

export interface TaskCommandRun {
  runId: string;
  taskId: string;
  purpose: CommandRunPurpose;
  command: string;
  cwd: string;
  status: CommandRunStatus;
  stdoutPath?: string | null;
  stderrPath?: string | null;
  exitCode?: number | null;
  durationMs?: number | null;
  createdAt: string;
}

export interface TaskArtifact {
  id: string;
  taskId: string;
  changedFiles: string;
  diffPath?: string | null;
  testReportPath?: string | null;
  screenshots: string;
  summary: string;
  commitMessage: string;
}

export interface TaskArtifactFile {
  id: string;
  taskId: string;
  artifactId?: string | null;
  fileType: string;
  path: string;
  sizeBytes: number;
  compressed: boolean;
  retentionClass: string;
  createdAt: string;
  expiresAt?: string | null;
}

export interface AgentSessionSummary {
  id: string;
  taskId: string;
  status: string;
  stage: string;
  checkpointId?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface AgentTimelineEvent {
  eventId: string;
  taskId: string;
  eventType: string;
  stage: string;
  message: string;
  createdAt: string;
  payload: Record<string, unknown>;
}

export interface TaskValidationRound {
  id: string;
  taskId: string;
  roundIndex: number;
  status: string;
  commandRunId?: string | null;
  analysis: string;
  repairSummary: string;
  validationSummary: string;
  createdAt: string;
  updatedAt: string;
}

export interface TaskMergeRecord {
  id: string;
  taskId: string;
  status: TaskMergeResultStatus | 'failed';
  targetBranch: string;
  sourceBranch: string;
  commitSha: string;
  commitMessage: string;
  conflictFiles: string[];
  errorReason?: string | null;
  recordPath?: string | null;
  createdAt: string;
}

export interface TaskDetail {
  task: TaskSummary;
  todos: TaskTodo[];
  commandRuns: TaskCommandRun[];
  approvals: ApprovalSummary[];
  artifacts: TaskArtifact[];
  artifactFiles: TaskArtifactFile[];
  agentSession?: AgentSessionSummary | null;
  timeline: AgentTimelineEvent[];
  validationRounds: TaskValidationRound[];
  mergeRecords: TaskMergeRecord[];
  deliveryReviewState: DeliveryReviewState;
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

