import { FormEvent, useDeferredValue, useEffect, useMemo, useState } from 'react';
import {
  ChevronDown,
  CircleAlert,
  Copy,
  FolderPen,
  FolderLock,
  Gauge,
  GitBranch,
  Globe2,
  LockKeyhole,
  Plus,
  SendHorizontal,
  Settings,
  TerminalSquare,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  createAgentTask,
  createTaskRecord,
  deleteTaskRecord,
  estimateTaskWorkspace,
  getModelConfig,
  getPrivacyPreview,
  getRunContractPreview,
  type TaskWorkspaceEstimate,
} from '@/api/tauriClient';
import { taskModelOptions, validationCommandPresets } from '@/features/tasks/taskDefaults';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore, type ThinkingStrength } from '@/state/appStore';
import type { ModelConfigView, PrivacyPreview, RunContractPreview } from '@/types/domain';

type RunMode = 'AGENT' | 'PLAN' | 'ASK' | 'REVIEW';
type PreviewState = 'idle' | 'loading' | 'ready' | 'error';
type WorkspaceStrategy = 'initialize_git' | 'isolated_copy' | 'direct_original';

const runModes: Array<{ id: RunMode; labelKey: string; bodyKey: string }> = [
  { id: 'AGENT', labelKey: 'tasks.new.mode.agent', bodyKey: 'tasks.new.mode.agentBody' },
  { id: 'PLAN', labelKey: 'tasks.new.mode.plan', bodyKey: 'tasks.new.mode.planBody' },
  { id: 'ASK', labelKey: 'tasks.new.mode.ask', bodyKey: 'tasks.new.mode.askBody' },
  { id: 'REVIEW', labelKey: 'tasks.new.mode.review', bodyKey: 'tasks.new.mode.reviewBody' },
];

const modelStrengthOptions: Array<{ id: ThinkingStrength; labelKey: string }> = [
  { id: 'minimal', labelKey: 'settings.thinking.minimal' },
  { id: 'low', labelKey: 'settings.thinking.low' },
  { id: 'medium', labelKey: 'settings.thinking.medium' },
  { id: 'high', labelKey: 'settings.thinking.high' },
  { id: 'max', labelKey: 'settings.thinking.max' },
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
  const composerDraft = useAppStore((state) => state.composerDraft);
  const thinkingStrength = useAppStore((state) => state.thinkingStrength);
  const workMode = useAppStore((state) => state.workMode);
  const setOpen = useAppStore((state) => state.setNewTaskDialogOpen);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setSelectedTaskId = useAppStore((state) => state.setSelectedTaskId);
  const setComposerDraft = useAppStore((state) => state.setComposerDraft);
  const setThinkingStrength = useAppStore((state) => state.setThinkingStrength);
  const [description, setDescription] = useState('');
  const [mode, setMode] = useState<RunMode>('AGENT');
  const [modelId, setModelId] = useState(taskModelOptions[0].id);
  const [configuredModel, setConfiguredModel] = useState<ModelConfigView | null>(null);
  const [validationCommand, setValidationCommand] = useState(validationCommandPresets[0] ?? '');
  const [enabledPermissions, setEnabledPermissions] = useState<Record<string, boolean>>({
    'workspace-write': true,
    'command-execution': true,
    'network-access': false,
  });
  const [error, setError] = useState<string | null>(null);
  const [isCreating, setIsCreating] = useState(false);
  const [previewState, setPreviewState] = useState<PreviewState>('idle');
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [privacyPreview, setPrivacyPreview] = useState<PrivacyPreview | null>(null);
  const [contractPreview, setContractPreview] = useState<RunContractPreview | null>(null);
  const [workspaceStrategy, setWorkspaceStrategy] = useState<WorkspaceStrategy | null>(null);
  const [originalWriteAuthorized, setOriginalWriteAuthorized] = useState(false);
  const [workspaceExclusions, setWorkspaceExclusions] = useState('');
  const [workspaceEstimate, setWorkspaceEstimate] = useState<TaskWorkspaceEstimate | null>(null);
  const [workspaceEstimateState, setWorkspaceEstimateState] = useState<PreviewState>('idle');
  const [workspaceEstimateError, setWorkspaceEstimateError] = useState<string | null>(null);
  const deferredWorkspaceExclusions = useDeferredValue(workspaceExclusions);
  const deferredDescription = useDeferredValue(description);
  const deferredValidationCommand = useDeferredValue(validationCommand);

  const availableModels = useMemo(() => {
    if (!configuredModel) {
      return taskModelOptions;
    }

    const configuredOption = {
      id: configuredModel.id,
      provider: configuredModel.provider,
      model: configuredModel.modelName,
      context: 'custom',
      descriptionKey: '',
    };

    return [configuredOption, ...taskModelOptions.filter((model) => model.id !== configuredModel.id)];
  }, [configuredModel]);
  const selectedModel = availableModels.find((model) => model.id === modelId) ?? availableModels[0];
  const selectedStrengthLabel = t(
    modelStrengthOptions.find((option) => option.id === thinkingStrength)?.labelKey ?? 'settings.thinking.medium',
    locale,
  );
  const selectedModeLabel = t(
    runModes.find((runMode) => runMode.id === mode)?.labelKey ?? 'tasks.new.mode.agent',
    locale,
  );
  const enabledPermissionSummary = accessPermissions
    .filter((permission) => enabledPermissions[permission.id])
    .map((permission) => t(permission.titleKey, locale))
    .join(' · ') || '—';

  useEffect(() => {
    if (open && composerDraft.trim()) {
      setDescription(composerDraft);
    }
  }, [composerDraft, open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    if (!currentRepository || currentRepository.isGitRepository) {
      setWorkspaceStrategy(null);
      setOriginalWriteAuthorized(false);
      return;
    }

    setWorkspaceStrategy(workMode === 'coding' ? 'isolated_copy' : null);
    setOriginalWriteAuthorized(false);
  }, [currentRepository, open, workMode]);

  useEffect(() => {
    if (!open || !currentRepository) {
      setWorkspaceEstimate(null);
      setWorkspaceEstimateState('idle');
      setWorkspaceEstimateError(null);
      return;
    }
    if (!currentRepository.isGitRepository && !workspaceStrategy) {
      setWorkspaceEstimate(null);
      setWorkspaceEstimateState('idle');
      setWorkspaceEstimateError(null);
      return;
    }

    let cancelled = false;
    setWorkspaceEstimateState('loading');
    setWorkspaceEstimateError(null);
    estimateTaskWorkspace({
      repositoryPath: currentRepository.path,
      workspaceStrategy,
      workspaceExclusions: parseWorkspaceExclusions(deferredWorkspaceExclusions),
    })
      .then((estimate) => {
        if (cancelled) {
          return;
        }
        setWorkspaceEstimate(estimate);
        setWorkspaceEstimateState('ready');
      })
      .catch((nextError) => {
        if (cancelled) {
          return;
        }
        setWorkspaceEstimate(null);
        setWorkspaceEstimateState('error');
        setWorkspaceEstimateError(normalizeTaskCreateError(nextError));
      });

    return () => {
      cancelled = true;
    };
  }, [currentRepository, deferredWorkspaceExclusions, open, workspaceStrategy]);

  useEffect(() => {
    if (!open) {
      return;
    }

    let cancelled = false;
    getModelConfig()
      .then((config) => {
        if (cancelled) {
          return;
        }
        setConfiguredModel(config);
        setModelId(config?.id ?? taskModelOptions[0].id);
      })
      .catch(() => {
        if (!cancelled) {
          setConfiguredModel(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      setPreviewState('idle');
      setPreviewError(null);
      setPrivacyPreview(null);
      setContractPreview(null);
      return;
    }

    if (!currentRepository || !deferredDescription.trim()) {
      setPreviewState('idle');
      setPreviewError(null);
      setPrivacyPreview(null);
      setContractPreview(null);
      return;
    }

    let cancelled = false;
    setPreviewState('loading');
    setPreviewError(null);

    const request = {
      repositoryPath: currentRepository.path,
      description: deferredDescription,
      modelId,
      validationCommand: deferredValidationCommand.trim() || undefined,
      mode: mapRunMode(mode),
      reasoningEffort: mapModelStrength(thinkingStrength),
      permissionLevel: mapPermissionLevel(enabledPermissions['workspace-write']),
      networkPolicy: mapNetworkPolicy(enabledPermissions['network-access']),
    };

    Promise.all([getPrivacyPreview(request), getRunContractPreview(request)])
      .then(([nextPrivacyPreview, nextContractPreview]) => {
        if (cancelled) {
          return;
        }
        setPrivacyPreview(nextPrivacyPreview);
        setContractPreview(nextContractPreview);
        setPreviewState('ready');
      })
      .catch((nextError) => {
        if (cancelled) {
          return;
        }
        setPrivacyPreview(null);
        setContractPreview(null);
        setPreviewState('error');
        setPreviewError(normalizeTaskCreateError(nextError));
      });

    return () => {
      cancelled = true;
    };
  }, [
    currentRepository,
    deferredDescription,
    deferredValidationCommand,
    enabledPermissions,
    mode,
    thinkingStrength,
    modelId,
    locale,
    open,
  ]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!currentRepository) {
      setError(t('tasks.new.errorRepository', locale));
      return;
    }

    if (!description.trim()) {
      setError(t('tasks.new.errorDescription', locale));
      return;
    }

    if (!currentRepository.isGitRepository && !workspaceStrategy) {
      setError(t('tasks.new.workspace.strategyRequired', locale));
      return;
    }

    if (workspaceStrategy === 'direct_original' && !originalWriteAuthorized) {
      setError(t('tasks.new.workspace.authorizationRequired', locale));
      return;
    }

    if (workspaceEstimate && !workspaceEstimate.sufficientSpace) {
      setError(t('tasks.new.workspace.insufficientSpace', locale));
      return;
    }

    if (previewState === 'loading' || workspaceEstimateState === 'loading') {
      setError(t('tasks.new.errorPreviewLoading', locale));
      return;
    }

    if (previewState !== 'ready' || !privacyPreview || !contractPreview) {
      setError(t('tasks.new.errorPreviewUnavailable', locale));
      return;
    }

    if (privacyPreview.blockedCount > 0) {
      setError(t('tasks.new.errorPreviewBlocked', locale));
      return;
    }

    setError(null);
    setIsCreating(true);
    let createdTaskId: string | null = null;

    try {
      const task = await createTaskRecord({
        repositoryPath: currentRepository.path,
        description,
        modelId,
        taskType: 'custom',
        validationCommand: validationCommand.trim() || null,
        mode: mapRunMode(mode),
        reasoningEffort: mapModelStrength(thinkingStrength),
        permissionLevel: mapPermissionLevel(enabledPermissions['workspace-write']),
        networkPolicy: mapNetworkPolicy(enabledPermissions['network-access']),
        workMode,
        workspaceStrategy,
        originalWriteAuthorized,
        workspaceExclusions: parseWorkspaceExclusions(workspaceExclusions),
      });
      createdTaskId = task.id;

      if (!task.worktreePath) {
        throw new Error(t('tasks.new.errorWorktreeMissing', locale));
      }

      await createAgentTask({
        taskId: task.id,
        repositoryPath: task.repositoryPath,
        worktreePath: task.worktreePath,
        title: task.title,
        description: task.description,
        modelId,
        validationCommand: validationCommand.trim() || null,
      });

      setSelectedTaskId(task.id);
      setCurrentRoute('tasks');
      setDescription('');
      setWorkspaceExclusions('');
      setComposerDraft('');
      setOpen(false);
    } catch (error) {
      if (createdTaskId) {
        try {
          await deleteTaskRecord(createdTaskId, true);
        } catch (cleanupError) {
          setError(
            `${normalizeTaskCreateError(error)} ${t('tasks.new.errorTaskRollback', locale)} ${normalizeTaskCreateError(cleanupError)}`,
          );
          return;
        }
      }
      setError(normalizeTaskCreateError(error));
    } finally {
      setIsCreating(false);
    }
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
              {validationCommandPresets.map((command) => (
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

          <fieldset className="workspace-strategy-control">
            <legend>{t('tasks.new.workspace.title', locale)}</legend>
            {currentRepository?.isGitRepository ? (
              <div className="workspace-strategy-summary">
                <GitBranch className="h-4 w-4" aria-hidden="true" />
                <span>
                  <strong>{t('tasks.new.workspace.gitWorktree', locale)}</strong>
                  <small>{currentRepository.path}</small>
                </span>
              </div>
            ) : (
              <>
                <p>
                  {t(
                    workMode === 'daily'
                      ? 'tasks.new.workspace.dailyPrompt'
                      : 'tasks.new.workspace.codingPrompt',
                    locale,
                  )}
                </p>
                <div className="workspace-strategy-options" role="radiogroup">
                  {workMode === 'daily' ? (
                    <WorkspaceStrategyButton
                      active={workspaceStrategy === 'initialize_git'}
                      icon={GitBranch}
                      label={t('tasks.new.workspace.gitInit', locale)}
                      onSelect={() => {
                        setWorkspaceStrategy('initialize_git');
                        setOriginalWriteAuthorized(false);
                      }}
                    />
                  ) : null}
                  <WorkspaceStrategyButton
                    active={workspaceStrategy === 'isolated_copy'}
                    icon={Copy}
                    label={t('tasks.new.workspace.isolatedCopy', locale)}
                    onSelect={() => {
                      setWorkspaceStrategy('isolated_copy');
                      setOriginalWriteAuthorized(false);
                    }}
                  />
                  {workMode === 'coding' ? (
                    <WorkspaceStrategyButton
                      active={workspaceStrategy === 'direct_original'}
                      icon={FolderPen}
                      label={t('tasks.new.workspace.directOriginal', locale)}
                      onSelect={() => {
                        setWorkspaceStrategy('direct_original');
                        setOriginalWriteAuthorized(false);
                      }}
                    />
                  ) : null}
                </div>
                {workspaceStrategy === 'direct_original' ? (
                  <label className="workspace-direct-authorization">
                    <input
                      type="checkbox"
                      checked={originalWriteAuthorized}
                      onChange={(event) => setOriginalWriteAuthorized(event.target.checked)}
                    />
                    <span>
                      <strong>{t('tasks.new.workspace.authorizeDirect', locale)}</strong>
                      <small>{t('tasks.new.workspace.directOriginalWarning', locale)}</small>
                    </span>
                  </label>
                ) : null}
                {workspaceStrategy === 'isolated_copy' ? (
                  <label className="workspace-exclusions-field" htmlFor="new-task-workspace-exclusions">
                    <span>{t('tasks.new.workspace.exclusions', locale)}</span>
                    <input
                      id="new-task-workspace-exclusions"
                      value={workspaceExclusions}
                      onChange={(event) => setWorkspaceExclusions(event.target.value)}
                      placeholder={t('tasks.new.workspace.exclusionsPlaceholder', locale)}
                    />
                    <small>{t('tasks.new.workspace.exclusionsHint', locale)}</small>
                  </label>
                ) : null}
              </>
            )}

            {currentRepository ? (
              <div className={cn('workspace-estimate-grid', workspaceEstimate && !workspaceEstimate.sufficientSpace && 'insufficient')}>
                <WorkspaceEstimateRow
                  label={t('tasks.new.workspace.sourcePath', locale)}
                  value={workspaceEstimate?.sourcePath ?? currentRepository.path}
                  code
                />
                <WorkspaceEstimateRow
                  label={t('tasks.new.workspace.destinationPath', locale)}
                  value={
                    workspaceEstimate
                      ? workspaceEstimate.workspaceKind === 'direct_original'
                        ? workspaceEstimate.destinationRoot
                        : `${workspaceEstimate.destinationRoot}/<task-id>`
                      : t('tasks.new.workspace.estimateLoading', locale)
                  }
                  code
                />
                <WorkspaceEstimateRow
                  label={t('tasks.new.workspace.estimatedSize', locale)}
                  value={
                    workspaceEstimate
                      ? formatWorkspaceBytes(workspaceEstimate.estimatedBytes) +
                        ' · ' +
                        workspaceEstimate.estimatedFiles +
                        ' ' +
                        t('tasks.new.workspace.files', locale)
                      : workspaceEstimateState === 'error'
                        ? workspaceEstimateError ?? t('tasks.new.workspace.estimateError', locale)
                        : t('tasks.new.workspace.estimateLoading', locale)
                  }
                />
                <WorkspaceEstimateRow
                  label={t('tasks.new.workspace.availableSpace', locale)}
                  value={
                    workspaceEstimate
                      ? formatWorkspaceBytes(workspaceEstimate.availableBytes) +
                        ' · ' +
                        t(
                          workspaceEstimate.sufficientSpace
                            ? 'tasks.new.workspace.spaceEnough'
                            : 'tasks.new.workspace.spaceInsufficient',
                          locale,
                        )
                      : '-'
                  }
                />
                <WorkspaceEstimateRow
                  label={t('tasks.new.workspace.cleanupImpact', locale)}
                  value={t(
                    `tasks.new.workspace.cleanup.${workspaceEstimate?.cleanupPolicy ?? 'remove_workspace_keep_source'}`,
                    locale,
                  )}
                />
              </div>
            ) : null}
          </fieldset>

          <div className="composer-toolbar codex-primary-toolbar">
            <button type="button" className="composer-attach-button">
              <Plus className="h-4 w-4" aria-hidden="true" />
              {t('tasks.composer.attach', locale)}
            </button>
            <button type="button" className="model-select-trigger">
              <span>{selectedModel.model}</span>
              <small>{selectedStrengthLabel}</small>
              <ChevronDown className="h-4 w-4" aria-hidden="true" />
            </button>

            <div className="reasoning-control" aria-label={t('tasks.new.modelStrength', locale)}>
              <Gauge className="h-4 w-4" aria-hidden="true" />
              {modelStrengthOptions.map((option) => (
                <button
                  key={option.id}
                  type="button"
                  className={cn(thinkingStrength === option.id && 'active')}
                  onClick={() => setThinkingStrength(option.id)}
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

            <Button
              type="submit"
              size="icon"
              className="codex-send-button"
              aria-label={t('tasks.new.submit', locale)}
              disabled={isCreating || previewState === 'loading'}
            >
              <SendHorizontal className={cn('h-4 w-4', isCreating && 'diff-spin')} aria-hidden="true" />
            </Button>
          </div>

          <div className="codex-contract-grid" aria-label={t('tasks.new.runContract', locale)}>
            <section className="codex-contract-card">
              <strong>{t('tasks.new.modelStrength', locale)}</strong>
              <span>{selectedStrengthLabel}</span>
            </section>
            <section className="codex-contract-card">
              <strong>{t('tasks.new.mode.title', locale)}</strong>
              <span>{selectedModeLabel}</span>
            </section>
            <section className="codex-contract-card wide">
              <strong>{t('tasks.new.accessPermissions', locale)}</strong>
              <span>{enabledPermissionSummary}</span>
            </section>
            <section className="codex-contract-card wide">
              <strong>{t('tasks.new.validationCommand', locale)}</strong>
              <span>{validationCommand.trim() || t('tasks.new.validationAuto', locale)}</span>
            </section>
          </div>

          <div className="codex-preview-grid" aria-label={t('tasks.new.runContractBody', locale)}>
            <section className="codex-contract-card privacy-preview-card">
              <strong>{t('tasks.new.preview.privacy', locale)}</strong>
              <span>{privacyPreviewHeadline(previewState, locale, previewError)}</span>
              <span>
                {privacyPreview
                  ? `${privacyPreview.totalSources} ${t('tasks.new.preview.sources', locale)} · ${privacyPreview.redactedCount} ${t('tasks.new.preview.redacted', locale)} · ${privacyPreview.blockedCount} ${t('tasks.new.preview.blocked', locale)}`
                  : t('tasks.new.preview.idle', locale)}
              </span>
            </section>

            <section className="codex-contract-card contract-preview-card">
              <strong>{t('tasks.new.preview.contract', locale)}</strong>
              <span>{contractPreviewHeadline(previewState, contractPreview, locale, previewError)}</span>
              <span>
                {contractPreview
                  ? `${contractPreview.permissionLevel} / ${contractPreview.networkPolicy}`
                  : t('tasks.new.preview.idle', locale)}
              </span>
              <span>
                {contractPreview
                  ? `${t('tasks.new.preview.budget', locale)} ${contractPreview.tokenBudgetTotal}`
                  : t('tasks.new.preview.validation', locale)}
              </span>
            </section>
          </div>

          <div className="codex-model-drawer" aria-label={t('tasks.new.model.title', locale)}>
            {availableModels.map((model) => (
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

function WorkspaceStrategyButton({
  active,
  icon: Icon,
  label,
  onSelect,
}: {
  active: boolean;
  icon: typeof GitBranch;
  label: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      className={cn('workspace-strategy-option', active && 'active')}
      role="radio"
      aria-checked={active}
      onClick={onSelect}
    >
      <Icon className="h-4 w-4" aria-hidden="true" />
      <span>{label}</span>
    </button>
  );
}

function WorkspaceEstimateRow({
  label,
  value,
  code = false,
}: {
  label: string;
  value: string;
  code?: boolean;
}) {
  return (
    <div className="workspace-estimate-row">
      <span>{label}</span>
      {code ? <code>{value}</code> : <strong>{value}</strong>}
    </div>
  );
}

function normalizeTaskCreateError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'title' in error) {
    return String((error as { title: unknown }).title);
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

function parseWorkspaceExclusions(value: string): string[] {
  return [...new Set(value.split(',').map((item) => item.trim()).filter(Boolean))];
}

function formatWorkspaceBytes(bytes: number): string {
  if (bytes < 1024) {
    return String(bytes) + ' B';
  }
  if (bytes < 1024 * 1024) {
    return (bytes / 1024).toFixed(1) + ' KB';
  }
  if (bytes < 1024 * 1024 * 1024) {
    return (bytes / 1024 / 1024).toFixed(1) + ' MB';
  }
  return (bytes / 1024 / 1024 / 1024).toFixed(1) + ' GB';
}

function mapRunMode(mode: RunMode): string {
  return mode.toLowerCase();
}

function mapModelStrength(modelStrength: ThinkingStrength): string {
  return {
    minimal: 'minimal',
    low: 'low',
    medium: 'balanced',
    high: 'high',
    max: 'max',
  }[modelStrength];
}

function mapPermissionLevel(workspaceWriteEnabled: boolean): string {
  return workspaceWriteEnabled ? 'worktree_write' : 'read_only';
}

function mapNetworkPolicy(networkAccessEnabled: boolean): string {
  return networkAccessEnabled ? 'enabled' : 'approval_required';
}

function privacyPreviewHeadline(state: PreviewState, locale: 'zh-CN' | 'en-US', error: string | null) {
  if (state === 'loading') {
    return t('tasks.new.preview.loading', locale);
  }

  if (state === 'error') {
    return error ?? t('tasks.new.preview.error', locale);
  }

  if (state === 'ready') {
    return t('tasks.new.preview.ready', locale);
  }

  return t('tasks.new.preview.idle', locale);
}

function contractPreviewHeadline(
  state: PreviewState,
  contractPreview: RunContractPreview | null,
  locale: 'zh-CN' | 'en-US',
  error: string | null,
) {
  if (state === 'loading') {
    return t('tasks.new.preview.loading', locale);
  }

  if (state === 'error') {
    return error ?? t('tasks.new.preview.error', locale);
  }

  if (!contractPreview) {
    return t('tasks.new.preview.idle', locale);
  }

  return `${contractPreview.sourceProfileName} / ${contractPreview.mode}`;
}
