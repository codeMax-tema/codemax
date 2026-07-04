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

