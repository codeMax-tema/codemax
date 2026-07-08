export interface ModelOption {
  id: string;
  provider: string;
  model: string;
  context: string;
  descriptionKey: string;
}

export interface SettingsDefaults {
  recentMessages: number;
  logRetentionDays: number;
  screenshotRetentionDays: number;
  maxRepairRounds: number;
  models: ModelOption[];
  validationCommands: string[];
}

export const taskModelOptions: ModelOption[] = [
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
];

export const validationCommandPresets = ['npm run check', 'npm run build:desktop', 'cargo check'];

export const settingsDefaults: SettingsDefaults = {
  recentMessages: 50,
  logRetentionDays: 30,
  screenshotRetentionDays: 30,
  maxRepairRounds: 5,
  models: taskModelOptions,
  validationCommands: validationCommandPresets,
};
