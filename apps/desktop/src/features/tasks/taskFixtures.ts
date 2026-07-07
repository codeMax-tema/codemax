import type { TaskStatus, TaskSummary, TaskType } from '@/types/domain';

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

export function getTaskTypeLabelKey(type: TaskType) {
  return `tasks.type.${type}`;
}

export function getTaskStatusLabelKey(status: TaskStatus) {
  return `status.${status}`;
}
