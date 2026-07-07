import { FormEvent, useState } from 'react';
import {
  ChevronDown,
  CircleAlert,
  FolderLock,
  Gauge,
  Globe2,
  LockKeyhole,
  Plus,
  SendHorizontal,
  Settings,
  TerminalSquare,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { s6SettingsFixture } from '@/features/tasks/taskFixtures';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';

type RunMode = 'AGENT' | 'PLAN' | 'ASK' | 'REVIEW';
type ModelStrength = 'fast' | 'balanced' | 'deep' | 'max';

const runModes: Array<{ id: RunMode; labelKey: string; bodyKey: string }> = [
  { id: 'AGENT', labelKey: 'tasks.new.mode.agent', bodyKey: 'tasks.new.mode.agentBody' },
  { id: 'PLAN', labelKey: 'tasks.new.mode.plan', bodyKey: 'tasks.new.mode.planBody' },
  { id: 'ASK', labelKey: 'tasks.new.mode.ask', bodyKey: 'tasks.new.mode.askBody' },
  { id: 'REVIEW', labelKey: 'tasks.new.mode.review', bodyKey: 'tasks.new.mode.reviewBody' },
];

const modelStrengthOptions: Array<{ id: ModelStrength; labelKey: string }> = [
  { id: 'fast', labelKey: 'tasks.new.strength.fast' },
  { id: 'balanced', labelKey: 'tasks.new.strength.balanced' },
  { id: 'deep', labelKey: 'tasks.new.strength.deep' },
  { id: 'max', labelKey: 'tasks.new.strength.max' },
];

const accessPermissions = [
  {
    id: 'workspace-write',
    icon: FolderLock,
    titleKey: 'tasks.new.permissions.workspace',
    bodyKey: 'tasks.new.permissions.workspaceBody',
  },
  {
    id: 'command-execution',
    icon: TerminalSquare,
    titleKey: 'tasks.new.permissions.command',
    bodyKey: 'tasks.new.permissions.commandBody',
  },
  {
    id: 'network-access',
    icon: Globe2,
    titleKey: 'tasks.new.permissions.network',
    bodyKey: 'tasks.new.permissions.networkBody',
  },
] as const;

export function NewTaskDialog() {
  const locale = useAppStore((state) => state.locale);
  const currentRepository = useAppStore((state) => state.currentRepository);
  const open = useAppStore((state) => state.newTaskDialogOpen);
  const setOpen = useAppStore((state) => state.setNewTaskDialogOpen);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setSelectedTaskId = useAppStore((state) => state.setSelectedTaskId);
  const [description, setDescription] = useState('');
  const [mode, setMode] = useState<RunMode>('AGENT');
  const [modelId, setModelId] = useState(s6SettingsFixture.models[0].id);
  const [modelStrength, setModelStrength] = useState<ModelStrength>('balanced');
  const [validationCommand, setValidationCommand] = useState(s6SettingsFixture.validationCommands[0] ?? '');
  const [enabledPermissions, setEnabledPermissions] = useState<Record<string, boolean>>({
    'workspace-write': true,
    'command-execution': true,
    'network-access': false,
  });
  const [error, setError] = useState<string | null>(null);

  const selectedModel = s6SettingsFixture.models.find((model) => model.id === modelId) ?? s6SettingsFixture.models[0];

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!currentRepository) {
      setError(t('tasks.new.errorRepository', locale));
      return;
    }

    if (!description.trim()) {
      setError(t('tasks.new.errorDescription', locale));
      return;
    }

    setError(null);
    setSelectedTaskId('task-240707-01');
    setCurrentRoute('tasks');
    setOpen(false);
  }

  function togglePermission(permissionId: string) {
    setEnabledPermissions((current) => ({
      ...current,
      [permissionId]: !current[permissionId],
    }));
  }

  function openSettings() {
    setCurrentRoute('settings');
    setOpen(false);
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="codex-composer-dialog">
        <DialogHeader className="codex-dialog-titlebar">
          <div>
            <DialogTitle>{t('tasks.new.title', locale)}</DialogTitle>
            <DialogDescription>{t('tasks.new.subtitle', locale)}</DialogDescription>
          </div>
          <button type="button" className="codex-dialog-settings" onClick={openSettings}>
            <Settings className="h-4 w-4" aria-hidden="true" />
            {t('tasks.new.openSettings', locale)}
          </button>
        </DialogHeader>

        <form className="codex-composer" onSubmit={handleSubmit}>
          <textarea
            value={description}
            onChange={(event) => setDescription(event.target.value)}
            rows={7}
            placeholder={t('tasks.new.descriptionPlaceholder', locale)}
            aria-label={t('tasks.new.description', locale)}
          />

          <div className="codex-validation-command" aria-label={t('tasks.new.validationCommand', locale)}>
            <label htmlFor="new-task-validation-command">{t('tasks.new.validationCommand', locale)}</label>
            <div className="codex-validation-command-row">
              <TerminalSquare className="h-4 w-4" aria-hidden="true" />
              <input
                id="new-task-validation-command"
                value={validationCommand}
                onChange={(event) => setValidationCommand(event.target.value)}
                placeholder={t('tasks.new.validationCommandPlaceholder', locale)}
              />
            </div>
            <div className="codex-validation-presets" aria-label={t('tasks.new.validationPresets', locale)}>
              {s6SettingsFixture.validationCommands.map((command) => (
                <button
                  key={command}
                  type="button"
                  className={cn(validationCommand === command && 'active')}
                  onClick={() => setValidationCommand(command)}
                >
                  {command}
                </button>
              ))}
            </div>
          </div>

          <div className="composer-toolbar codex-primary-toolbar">
            <button type="button" className="composer-attach-button">
              <Plus className="h-4 w-4" aria-hidden="true" />
              {t('tasks.composer.attach', locale)}
            </button>
            <button type="button" className="model-select-trigger">
              <span>{selectedModel.model}</span>
              <small>{selectedModel.provider}</small>
              <ChevronDown className="h-4 w-4" aria-hidden="true" />
            </button>

            <div className="reasoning-control" aria-label={t('tasks.new.modelStrength', locale)}>
              <Gauge className="h-4 w-4" aria-hidden="true" />
              {modelStrengthOptions.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className={cn(modelStrength === option.id && 'active')}
                  onClick={() => setModelStrength(option.id)}
                >
                  {t(option.labelKey, locale)}
                </button>
              ))}
            </div>

            <div className="access-mode-control" aria-label={t('tasks.new.accessPermissions', locale)}>
              <LockKeyhole className="h-4 w-4" aria-hidden="true" />
              {accessPermissions.map((permission) => {
                const Icon = permission.icon;
                return (
                  <button
                    key={permission.id}
                    type="button"
                    data-permission={permission.id}
                    className={cn('permission-toggle', enabledPermissions[permission.id] && 'active')}
                    onClick={() => togglePermission(permission.id)}
                    title={`${t(permission.titleKey, locale)} · ${t(permission.bodyKey, locale)}`}
                  >
                    <Icon className="h-4 w-4" aria-hidden="true" />
                    <span>{t(permission.titleKey, locale)}</span>
                  </button>
                );
              })}
            </div>

            <div className="mode-control" aria-label={t('tasks.new.mode.title', locale)}>
              {runModes.map((runMode) => (
                <button
                  key={runMode.id}
                  type="button"
                  className={cn(mode === runMode.id && 'active')}
                  onClick={() => setMode(runMode.id)}
                  title={t(runMode.bodyKey, locale)}
                >
                  {t(runMode.labelKey, locale)}
                </button>
              ))}
            </div>

            <Button type="submit" size="icon" className="codex-send-button" aria-label={t('tasks.new.submit', locale)}>
              <SendHorizontal className="h-4 w-4" aria-hidden="true" />
            </Button>
          </div>

          <div className="codex-contract-grid" aria-label={t('tasks.new.runContract', locale)}>
            <section className="codex-contract-card">
              <strong>{t('tasks.new.modelStrength', locale)}</strong>
              <span>{t(modelStrengthOptions.find((option) => option.id === modelStrength)?.labelKey ?? 'tasks.new.strength.balanced', locale)}</span>
            </section>
            <section className="codex-contract-card">
              <strong>{t('tasks.new.mode.title', locale)}</strong>
              <span>{mode}</span>
            </section>
            <section className="codex-contract-card wide">
              <strong>{t('tasks.new.accessPermissions', locale)}</strong>
              <span>{t('tasks.new.permissions.workspace', locale)} · {t('tasks.new.permissions.command', locale)}</span>
            </section>
            <section className="codex-contract-card wide">
              <strong>{t('tasks.new.validationCommand', locale)}</strong>
              <span>{validationCommand.trim() || t('tasks.new.validationAuto', locale)}</span>
            </section>
          </div>

          <div className="codex-model-drawer" aria-label={t('tasks.new.model.title', locale)}>
            {s6SettingsFixture.models.map((model) => (
              <button
                key={model.id}
                type="button"
                className={cn('model-option', modelId === model.id && 'active')}
                onClick={() => setModelId(model.id)}
              >
                <span>
                  <strong>{model.model}</strong>
                  <small>{t(model.descriptionKey, locale)}</small>
                </span>
                <em>{model.context}</em>
              </button>
            ))}
          </div>

          {error ? (
            <div className="inline-error codex-composer-error" role="alert">
              <CircleAlert className="h-4 w-4" aria-hidden="true" />
              {error}
            </div>
          ) : null}
        </form>
      </DialogContent>
    </Dialog>
  );
}
