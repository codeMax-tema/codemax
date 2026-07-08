import type { GeneratedTaskProofPack, TaskStatus, TaskSummary, TaskType } from '@/types/domain';

export interface ModelOption {
  id: string;
  provider: string;
  model: string;
  context: string;
  descriptionKey: string;
}

export interface SettingsFixture {
  appDataPath: string;
  worktreeRoot: string;
  recentMessages: number;
  logRetentionDays: number;
  screenshotRetentionDays: number;
  maxRepairRounds: number;
  models: ModelOption[];
  validationCommands: string[];
}

export const s6TaskFixtures: TaskSummary[] = [
  {
    id: 'task-240707-01',
    title: '修复支付模块金额精度问题',
    type: 'bugfix',
    status: 'running',
    repositoryPath: 'D:\\codemax',
    worktreePath: 'D:\\codemax\\.worktrees\\task-240707-01',
    branchName: 'codex/task-240707-01',
    createdAt: '2026-07-07 08:30',
    updatedAt: '2026-07-07 09:10',
  },
  {
    id: 'task-240707-02',
    title: '补充任务调度服务单元测试',
    type: 'test',
    status: 'waitingApproval',
    repositoryPath: 'D:\\codemax',
    worktreePath: 'D:\\codemax\\.worktrees\\task-240707-02',
    branchName: 'codex/task-240707-02',
    createdAt: '2026-07-07 08:42',
    updatedAt: '2026-07-07 08:58',
  },
  {
    id: 'task-240707-03',
    title: '解释 Rust 命令执行管线',
    type: 'explain',
    status: 'completed',
    repositoryPath: 'D:\\codemax',
    worktreePath: 'D:\\codemax\\.worktrees\\task-240707-03',
    branchName: 'codex/task-240707-03',
    createdAt: '2026-07-07 07:55',
    updatedAt: '2026-07-07 08:20',
  },
];

export const s6SettingsFixture: SettingsFixture = {
  appDataPath: 'C:\\Users\\21578\\AppData\\Roaming\\codemax-agent-console',
  worktreeRoot: 'D:\\codemax\\.worktrees',
  recentMessages: 50,
  logRetentionDays: 30,
  screenshotRetentionDays: 30,
  maxRepairRounds: 5,
  models: [
    {
      id: 'gpt-5-codex',
      provider: 'OpenAI',
      model: 'GPT-5 Codex',
      context: '256k',
      descriptionKey: 'tasks.new.model.gpt5.description',
    },
    {
      id: 'deepseek-coder',
      provider: 'DeepSeek',
      model: 'DeepSeek Coder',
      context: '128k',
      descriptionKey: 'tasks.new.model.deepseek.description',
    },
    {
      id: 'local-openai-compatible',
      provider: 'Local',
      model: 'OpenAI-compatible',
      context: '64k',
      descriptionKey: 'tasks.new.model.local.description',
    },
  ],
  validationCommands: ['npm run check', 'npm run build:desktop', 'cargo check'],
};

export const s12ProofPackFixture: GeneratedTaskProofPack = {
  taskId: 'task-240707-01',
  artifactId: 'demo-proof-pack-artifact',
  generatedAt: '1783372800',
  proofPackPath: 'app-data/tasks/task-240707-01/artifacts/proof-pack.json',
  summaryKey: 'tasks.s12.summary',
  deliveryScore: {
    value: 92,
    grade: 'A-',
    summaryKey: 'tasks.s12.deliveryScore.summary',
  },
  proposals: [
    {
      id: 'proposal-minimal',
      titleKey: 'tasks.s12.proposals.minimal.title',
      summaryKey: 'tasks.s12.proposals.minimal.summary',
      status: 'passed',
      confidence: 94,
    },
    {
      id: 'proposal-hardened',
      titleKey: 'tasks.s12.proposals.hardened.title',
      summaryKey: 'tasks.s12.proposals.hardened.summary',
      status: 'warning',
      confidence: 81,
    },
  ],
  screenshots: [
    {
      id: 'screenshot-overview',
      titleKey: 'tasks.s12.screenshots.overview',
      path: 'app-data/tasks/task-240707-01/screenshots/overview.png',
      capturedAt: '2026-07-07 09:18',
      status: 'passed',
    },
    {
      id: 'screenshot-mobile',
      titleKey: 'tasks.s12.screenshots.mobile',
      path: 'app-data/tasks/task-240707-01/screenshots/mobile.png',
      capturedAt: '2026-07-07 09:20',
      status: 'warning',
    },
  ],
  qualityGates: [
    {
      id: 'gate-tests',
      titleKey: 'tasks.s12.qualityGate.tests.title',
      summaryKey: 'tasks.s12.qualityGate.tests.summary',
      status: 'passed',
    },
    {
      id: 'gate-proof',
      titleKey: 'tasks.s12.qualityGate.proof.title',
      summaryKey: 'tasks.s12.qualityGate.proof.summary',
      status: 'passed',
    },
    {
      id: 'gate-approval',
      titleKey: 'tasks.s12.qualityGate.approval.title',
      summaryKey: 'tasks.s12.qualityGate.approval.summary',
      status: 'warning',
    },
  ],
  risks: [
    {
      id: 'risk-backend',
      titleKey: 'tasks.s12.riskRadar.backend.title',
      summaryKey: 'tasks.s12.riskRadar.backend.summary',
      level: 'medium',
    },
    {
      id: 'risk-storage',
      titleKey: 'tasks.s12.riskRadar.storage.title',
      summaryKey: 'tasks.s12.riskRadar.storage.summary',
      level: 'low',
    },
  ],
};

export function getTaskTypeLabelKey(type: TaskType) {
  return `tasks.type.${type}`;
}

export function getTaskStatusLabelKey(status: TaskStatus) {
  return `status.${status}`;
}
