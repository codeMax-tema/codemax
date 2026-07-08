import {
  Activity,
  Archive,
  ArrowLeft,
  Bot,
  Code2,
  Database,
  FolderTree,
  GitBranch,
  Globe2,
  Keyboard,
  Link2,
  LockKeyhole,
  Monitor,
  Palette,
  Plug,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  TerminalSquare,
  Trash2,
  Workflow,
} from 'lucide-react';
import { useEffect, useState, type FormEvent } from 'react';

import {
  cleanupStorage,
  getModelConfig,
  getStartupHealth,
  getStorageRoots,
  getStorageUsage,
  saveModelConfig,
  testModelConnection,
  type StorageRootsResponse,
} from '@/api/tauriClient';
import { Button } from '@/components/ui/button';
import { settingsDefaults } from '@/features/tasks/taskDefaults';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';
import type {
  CleanupStorageResponse,
  ModelConfigView,
  ModelConnectionTestResult,
  StartupHealthResponse,
  StorageUsageResponse,
} from '@/types/domain';

type SettingsCategory =
  | 'general'
  | 'appearance'
  | 'models'
  | 'personalization'
  | 'pets'
  | 'shortcuts'
  | 'mcp'
  | 'browser'
  | 'computer'
  | 'hooks'
  | 'connections'
  | 'git'
  | 'environment'
  | 'worktrees'
  | 'archived'
  | 'permissions'
  | 'modes'
  | 'storage'
  | 'memory'
  | 'language';

const DEFAULT_MODEL_CONFIG_ID = 'model-default';

const settingsGroups: Array<{
  headingKey: string;
  items: Array<{
    id: SettingsCategory;
    icon: typeof Bot;
    labelKey: string;
  }>;
}> = [
  {
    headingKey: 'settings.groups.personal',
    items: [
      { id: 'general', icon: Settings, labelKey: 'settings.categories.general' },
      { id: 'appearance', icon: Palette, labelKey: 'settings.categories.appearance' },
      { id: 'models', icon: Bot, labelKey: 'settings.categories.models' },
      { id: 'personalization', icon: SlidersHorizontal, labelKey: 'settings.categories.personalization' },
      { id: 'pets', icon: Archive, labelKey: 'settings.categories.pets' },
      { id: 'shortcuts', icon: Keyboard, labelKey: 'settings.categories.shortcuts' },
    ],
  },
  {
    headingKey: 'settings.groups.integrations',
    items: [
      { id: 'mcp', icon: Plug, labelKey: 'settings.categories.mcp' },
      { id: 'browser', icon: Globe2, labelKey: 'settings.categories.browser' },
      { id: 'computer', icon: Monitor, labelKey: 'settings.categories.computer' },
    ],
  },
  {
    headingKey: 'settings.groups.coding',
    items: [
      { id: 'hooks', icon: Workflow, labelKey: 'settings.categories.hooks' },
      { id: 'connections', icon: Link2, labelKey: 'settings.categories.connections' },
      { id: 'git', icon: GitBranch, labelKey: 'settings.categories.git' },
      { id: 'environment', icon: TerminalSquare, labelKey: 'settings.categories.environment' },
      { id: 'worktrees', icon: FolderTree, labelKey: 'settings.categories.worktrees' },
    ],
  },
  {
    headingKey: 'settings.groups.archived',
    items: [{ id: 'archived', icon: Archive, labelKey: 'settings.categories.archived' }],
  },
];

export function SettingsPage() {
  const locale = useAppStore((state) => state.locale);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const [activeCategory, setActiveCategory] = useState<SettingsCategory>('general');

  return (
    <div className="settings-shell codex-settings-page">
      <aside className="settings-rail settings-sidebar" aria-label={t('settings.title', locale)}>
        <button type="button" className="settings-return-button" onClick={() => setCurrentRoute('tasks')}>
          <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          {t('settings.return', locale)}
        </button>

        <label className="settings-search-box">
          <Search className="h-4 w-4" aria-hidden="true" />
          <input type="search" placeholder={t('settings.searchPlaceholder', locale)} />
        </label>

        <div className="settings-sidebar-scroll">
          {settingsGroups.map((group) => (
            <section className="settings-sidebar-group" key={group.headingKey}>
              <p className="settings-group-heading">{t(group.headingKey, locale)}</p>
              {group.items.map((category) => {
                const Icon = category.icon;
                return (
                  <button
                    key={category.id}
                    type="button"
                    className={cn('settings-nav-item', activeCategory === category.id && 'active')}
                    onClick={() => setActiveCategory(category.id)}
                  >
                    <Icon className="h-4 w-4" aria-hidden="true" />
                    <span>{t(category.labelKey, locale)}</span>
                  </button>
                );
              })}
            </section>
          ))}
        </div>
      </aside>
      <section className="settings-pane settings-detail">{renderSettingsPane(activeCategory)}</section>
    </div>
  );
}

function renderSettingsPane(category: SettingsCategory) {
  if (category === 'general') {
    return <GeneralSettings />;
  }

  if (category === 'permissions') {
    return <PermissionsSettings />;
  }

  if (category === 'modes') {
    return <ModeSettings />;
  }

  if (category === 'storage' || category === 'environment' || category === 'worktrees') {
    return <StorageSettings />;
  }

  if (category === 'memory' || category === 'personalization') {
    return <MemorySettings />;
  }

  if (category === 'appearance') {
    return <AppearanceSettings />;
  }

  if (category === 'language') {
    return <LanguageSettings />;
  }

  if (category === 'models') {
    return <ModelSettings />;
  }

  return <PlaceholderSettings category={category} />;
}

function SettingsPaneHeader({ titleKey, bodyKey }: { titleKey: string; bodyKey: string }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <header className="settings-pane-header">
      <h3>{t(titleKey, locale)}</h3>
      <p>{t(bodyKey, locale)}</p>
    </header>
  );
}

function GeneralSettings() {
  const locale = useAppStore((state) => state.locale);
  const [health, setHealth] = useState<StartupHealthResponse | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    getStartupHealth()
      .then((result) => {
        if (!cancelled) {
          setHealth(result);
          setHealthError(null);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setHealthError(readErrorMessage(error));
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <>
      <SettingsPaneHeader titleKey="settings.general.title" bodyKey="settings.general.body" />

      <section className="settings-block">
        <div className="settings-block-heading">
          <h4>{t('settings.health.title', locale)}</h4>
          <p>{t('settings.health.body', locale)}</p>
        </div>
        <div className="settings-diagnostic-list">
          {healthError ? (
            <section className="settings-line-section vertical">
              <strong>{t('settings.health.error', locale)}</strong>
              <code>{healthError}</code>
            </section>
          ) : null}
          {health ? (
            <>
              <section className="settings-line-section">
                <span>
                  <strong>{t('settings.health.overall', locale)}</strong>
                </span>
                <StatusPill status={health.status} label={t(`settings.health.status.${health.status}`, locale)} />
              </section>
              {health.items.map((item) => (
                <section className="settings-line-section" key={item.key}>
                  <span>
                    <strong>{t(`settings.health.item.${item.key}`, locale)}</strong>
                    <small>{t(item.messageKey, locale)}</small>
                    {item.detail ? <code>{item.detail}</code> : null}
                  </span>
                  <StatusPill status={item.status} label={t(`settings.health.status.${item.status}`, locale)} />
                </section>
              ))}
            </>
          ) : (
            <section className="settings-line-section">
              <span>
                <strong>{t('settings.health.loading', locale)}</strong>
              </span>
            </section>
          )}
        </div>
      </section>

      <section className="settings-block">
        <div className="settings-block-heading">
          <h4>{t('settings.general.workMode', locale)}</h4>
          <p>{t('settings.general.workModeBody', locale)}</p>
        </div>
        <div className="settings-work-mode-grid">
          <button type="button" className="settings-work-mode-card active">
            <Code2 className="h-5 w-5" aria-hidden="true" />
            <span>
              <strong>{t('settings.general.programming', locale)}</strong>
              <small>{t('settings.general.programmingBody', locale)}</small>
            </span>
            <em />
          </button>
          <button type="button" className="settings-work-mode-card">
            <MessageBubbleIcon />
            <span>
              <strong>{t('settings.general.everyday', locale)}</strong>
              <small>{t('settings.general.everydayBody', locale)}</small>
            </span>
            <em />
          </button>
        </div>
      </section>

      <section className="settings-block">
        <div className="settings-block-heading">
          <h4>{t('settings.general.permissions', locale)}</h4>
        </div>
        <div className="settings-card-list">
          <ToggleLine
            titleKey="settings.general.defaultPermission"
            bodyKey="settings.general.defaultPermissionBody"
            enabled
          />
          <ToggleLine titleKey="settings.general.autoReview" bodyKey="settings.general.autoReviewBody" enabled />
          <ToggleLine titleKey="settings.general.fullAccess" bodyKey="settings.general.fullAccessBody" enabled />
        </div>
      </section>

      <section className="settings-block">
        <div className="settings-block-heading">
          <h4>{t('settings.general.general', locale)}</h4>
        </div>
        <section className="settings-line-section">
          <span>
            <strong>{t('settings.general.defaultFileTarget', locale)}</strong>
            <small>{t('settings.general.defaultFileTargetBody', locale)}</small>
          </span>
          <button type="button" className="settings-select-pill">
            {t('settings.general.vsCode', locale)}
          </button>
        </section>
      </section>
    </>
  );
}

function ModelSettings() {
  const locale = useAppStore((state) => state.locale);
  const [provider, setProvider] = useState('openai-compatible');
  const [baseUrl, setBaseUrl] = useState('');
  const [modelName, setModelName] = useState('gpt-5-codex');
  const [apiKey, setApiKey] = useState('');
  const [savedConfig, setSavedConfig] = useState<ModelConfigView | null>(null);
  const [saveState, setSaveState] = useState<'idle' | 'loading' | 'saving' | 'saved' | 'error'>('idle');
  const [statusMessage, setStatusMessage] = useState('');
  const [connectionState, setConnectionState] = useState<'idle' | 'testing' | 'tested' | 'error'>('idle');
  const [connectionResult, setConnectionResult] = useState<ModelConnectionTestResult | null>(null);

  useEffect(() => {
    let cancelled = false;
    setSaveState('loading');
    getModelConfig(DEFAULT_MODEL_CONFIG_ID)
      .then((config) => {
        if (cancelled) {
          return;
        }

        if (config) {
          setSavedConfig(config);
          setProvider(config.provider);
          setBaseUrl(config.baseUrl);
          setModelName(config.modelName);
        }
        setSaveState('idle');
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setSaveState('error');
        setStatusMessage(readErrorMessage(error));
      });

    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSave(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSaveState('saving');
    setStatusMessage('');

    try {
      const config = await saveModelConfig({
        id: DEFAULT_MODEL_CONFIG_ID,
        provider,
        baseUrl,
        modelName,
        apiKey: apiKey.trim() || undefined,
      });
      setSavedConfig(config);
      setApiKey('');
      setSaveState('saved');
      setStatusMessage(t('settings.models.saved', locale));
    } catch (error) {
      setSaveState('error');
      setStatusMessage(readErrorMessage(error));
    }
  }

  async function handleTestConnection() {
    setConnectionState('testing');
    setConnectionResult(null);
    setStatusMessage('');

    try {
      const result = await testModelConnection(savedConfig?.id ?? DEFAULT_MODEL_CONFIG_ID);
      setConnectionResult(result);
      setConnectionState('tested');
    } catch (error) {
      setConnectionState('error');
      setStatusMessage(readErrorMessage(error));
    }
  }

  const apiKeyPreview = apiKey.trim()
    ? maskApiKey(apiKey)
    : savedConfig?.apiKeyMasked ?? t('settings.models.notConfigured', locale);

  return (
    <>
      <SettingsPaneHeader titleKey="settings.models.title" bodyKey="settings.models.body" />
      <form className="settings-section-list model-secret-form" onSubmit={handleSave}>
        <label className="settings-form-field">
          <span>{t('settings.models.provider', locale)}</span>
          <select value={provider} onChange={(event) => setProvider(event.target.value)}>
            <option value="openai-compatible">OpenAI-compatible</option>
            <option value="claude">Claude-compatible</option>
            <option value="deepseek">DeepSeek</option>
          </select>
        </label>
        <label className="settings-form-field">
          <span>{t('settings.models.baseUrl', locale)}</span>
          <input
            type="url"
            value={baseUrl}
            placeholder="https://api.example.com/v1"
            onChange={(event) => setBaseUrl(event.target.value)}
          />
        </label>
        <label className="settings-form-field">
          <span>{t('settings.models.modelName', locale)}</span>
          <input value={modelName} onChange={(event) => setModelName(event.target.value)} />
        </label>
        <label className="settings-form-field">
          <span>{t('settings.models.apiKey', locale)}</span>
          <input
            className="api-key-input"
            type="password"
            autoComplete="off"
            value={apiKey}
            placeholder={
              savedConfig?.apiKeyConfigured
                ? t('settings.models.keepExistingKey', locale)
                : t('settings.models.apiKeyPlaceholder', locale)
            }
            onChange={(event) => setApiKey(event.target.value)}
          />
        </label>
        <section className="settings-line-section api-key-preview">
          <span>
            <strong>{t('settings.models.apiKeyPreview', locale)}</strong>
            <small>{t('settings.models.apiKeyPreviewBody', locale)}</small>
          </span>
          <em>{apiKeyPreview}</em>
        </section>
        <section className="settings-line-section vertical secret-storage-location">
          <strong>{t('settings.models.secretStorage', locale)}</strong>
          <small>
            {savedConfig?.apiKeyConfigured
              ? `${savedConfig.secretStorage ?? t('settings.models.secretStorageUnknown', locale)} · ${
                  savedConfig.secretLocation ?? t('settings.models.secretLocationHidden', locale)
                }`
              : t('settings.models.secretStorageEmpty', locale)}
          </small>
        </section>
        <div className="settings-form-actions">
          <Button type="submit" size="sm" disabled={saveState === 'saving' || saveState === 'loading'}>
            {saveState === 'saving' ? t('settings.models.saving', locale) : t('settings.models.save', locale)}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={saveState === 'saving' || saveState === 'loading' || connectionState === 'testing'}
            onClick={handleTestConnection}
          >
            <Plug className="h-4 w-4" aria-hidden="true" />
            {connectionState === 'testing'
              ? t('settings.models.testingConnection', locale)
              : t('settings.models.testConnection', locale)}
          </Button>
          <span role="status">{statusMessage}</span>
        </div>
        {connectionResult ? (
          <section className="settings-line-section vertical model-connection-result">
            <strong>{t('settings.models.connectionResult', locale)}</strong>
            <div className="settings-diagnostic-list compact">
              <section className="settings-line-section">
                <span>
                  <small>{t(connectionResult.messageKey, locale)}</small>
                  <small>
                    {connectionResult.provider} / {connectionResult.modelName}
                    {connectionResult.baseUrlHost ? ` / ${connectionResult.baseUrlHost}` : ''}
                  </small>
                </span>
                <StatusPill
                  status={connectionResult.status}
                  label={t(`settings.health.status.${connectionResult.status}`, locale)}
                />
              </section>
            </div>
          </section>
        ) : null}
      </form>
      <div className="settings-section-list model-provider-list">
        {settingsDefaults.models.map((model, index) => (
          <section className="settings-line-section" key={model.id}>
            <span>
              <strong>{model.model}</strong>
              <small>
                {model.provider} · {model.context}
              </small>
            </span>
            <Button type="button" size="sm" variant={index === 0 ? 'default' : 'secondary'} disabled>
              {index === 0 ? t('settings.models.default', locale) : t('settings.models.setDefault', locale)}
            </Button>
          </section>
        ))}
      </div>
    </>
  );
}

function maskApiKey(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return '';
  }

  const suffix = trimmed.length > 4 ? trimmed.slice(-4) : '';
  return `${'*'.repeat(8)}${suffix}`;
}

function readErrorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === 'string') {
    return error;
  }

  return 'Unknown error';
}

function PermissionsSettings() {
  return (
    <>
      <SettingsPaneHeader titleKey="settings.permissions.title" bodyKey="settings.permissions.body" />
      <div className="settings-section-list">
        <PolicyLine titleKey="settings.permissions.workspace" valueKey="settings.permissions.workspaceValue" />
        <PolicyLine titleKey="settings.permissions.commands" valueKey="settings.permissions.commandsValue" />
        <PolicyLine titleKey="settings.permissions.network" valueKey="settings.permissions.networkValue" />
        <PolicyLine titleKey="settings.permissions.approvals" valueKey="settings.permissions.approvalsValue" />
      </div>
    </>
  );
}

function ModeSettings() {
  return (
    <>
      <SettingsPaneHeader titleKey="settings.modes.title" bodyKey="settings.modes.body" />
      <div className="mode-policy-grid">
        <PolicyLine titleKey="tasks.new.mode.agent" valueKey="settings.modes.agentValue" />
        <PolicyLine titleKey="tasks.new.mode.plan" valueKey="settings.modes.planValue" />
        <PolicyLine titleKey="tasks.new.mode.ask" valueKey="settings.modes.askValue" />
        <PolicyLine titleKey="tasks.new.mode.review" valueKey="settings.modes.reviewValue" />
      </div>
    </>
  );
}

function StorageSettings() {
  const locale = useAppStore((state) => state.locale);
  const [roots, setRoots] = useState<StorageRootsResponse | null>(null);
  const [usage, setUsage] = useState<StorageUsageResponse | null>(null);
  const [cleanupPreview, setCleanupPreview] = useState<CleanupStorageResponse | null>(null);
  const [rootsError, setRootsError] = useState<string | null>(null);
  const [storageState, setStorageState] = useState<'idle' | 'loading' | 'cleaning'>('idle');

  useEffect(() => {
    let cancelled = false;

    async function loadRoots() {
      try {
        setStorageState('loading');
        const [result, usageResult] = await Promise.all([getStorageRoots(), getStorageUsage()]);
        if (!cancelled) {
          setRoots(result);
          setUsage(usageResult);
          setRootsError(null);
          setStorageState('idle');
        }
      } catch (error) {
        if (!cancelled) {
          setRootsError(readErrorMessage(error));
          setStorageState('idle');
        }
      }
    }

    void loadRoots();

    return () => {
      cancelled = true;
    };
  }, []);

  async function refreshUsage() {
    setStorageState('loading');
    setRootsError(null);
    try {
      const [result, usageResult] = await Promise.all([getStorageRoots(), getStorageUsage()]);
      setRoots(result);
      setUsage(usageResult);
    } catch (error) {
      setRootsError(readErrorMessage(error));
    } finally {
      setStorageState('idle');
    }
  }

  async function previewCleanup() {
    setStorageState('loading');
    setRootsError(null);
    try {
      const result = await cleanupStorage({
        logs: true,
        screenshots: true,
        temporaryContext: true,
        dryRun: true,
      });
      setCleanupPreview(result);
    } catch (error) {
      setRootsError(readErrorMessage(error));
    } finally {
      setStorageState('idle');
    }
  }

  async function runCleanup() {
    const confirmed = window.confirm(t('settings.storage.cleanupConfirm', locale));
    if (!confirmed) {
      return;
    }

    setStorageState('cleaning');
    setRootsError(null);
    try {
      const result = await cleanupStorage({
        logs: true,
        screenshots: true,
        temporaryContext: true,
        dryRun: false,
      });
      setCleanupPreview(result);
      const usageResult = await getStorageUsage();
      setUsage(usageResult);
    } catch (error) {
      setRootsError(readErrorMessage(error));
    } finally {
      setStorageState('idle');
    }
  }

  return (
    <>
      <SettingsPaneHeader titleKey="settings.storage.title" bodyKey="settings.storage.body" />
      <div className="settings-form-actions storage-actions">
        <Button type="button" size="sm" variant="secondary" disabled={storageState === 'loading'} onClick={refreshUsage}>
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t('settings.storage.refreshUsage', locale)}
        </Button>
        <Button type="button" size="sm" variant="secondary" disabled={storageState !== 'idle'} onClick={previewCleanup}>
          <Activity className="h-4 w-4" aria-hidden="true" />
          {t('settings.storage.previewCleanup', locale)}
        </Button>
        <Button type="button" size="sm" disabled={storageState !== 'idle'} onClick={runCleanup}>
          <Trash2 className="h-4 w-4" aria-hidden="true" />
          {storageState === 'cleaning' ? t('settings.storage.cleaning', locale) : t('settings.storage.runCleanup', locale)}
        </Button>
      </div>
      <div className="settings-section-list">
        {rootsError ? (
          <section className="settings-line-section vertical">
            <strong>{t('settings.storage.pathError', locale)}</strong>
            <code>{rootsError}</code>
          </section>
        ) : null}
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.appData', locale)}</strong>
          <code>{roots?.appDataDir ?? t('settings.storage.loading', locale)}</code>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.database', locale)}</strong>
          <code>{roots?.databasePath ?? t('settings.storage.loading', locale)}</code>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.artifactRoot', locale)}</strong>
          <code>{roots?.artifactRoot ?? t('settings.storage.loading', locale)}</code>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.worktreeRoot', locale)}</strong>
          <code>{roots?.worktreeRoot ?? t('settings.storage.loading', locale)}</code>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.usageTitle', locale)}</strong>
          <div className="settings-usage-grid">
            <UsageMetric labelKey="settings.storage.databaseBytes" value={usage?.databaseBytes} />
            <UsageMetric labelKey="settings.storage.worktreeBytes" value={usage?.worktreeBytes} />
            <UsageMetric labelKey="settings.storage.logsBytes" value={usage?.logsBytes} />
            <UsageMetric labelKey="settings.storage.screenshotsBytes" value={usage?.screenshotsBytes} />
            <UsageMetric labelKey="settings.storage.contextBytes" value={usage?.temporaryContextBytes} />
            <UsageMetric labelKey="settings.storage.permanentEvidenceBytes" value={usage?.permanentEvidenceBytes} />
            <UsageMetric labelKey="settings.storage.totalBytes" value={usage?.totalBytes} />
          </div>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.cleanupTitle', locale)}</strong>
          <small>{t('settings.storage.cleanupBody', locale)}</small>
          {cleanupPreview ? (
            <div className="settings-cleanup-result">
              <span>
                {cleanupPreview.dryRun
                  ? t('settings.storage.cleanupPreviewResult', locale)
                  : t('settings.storage.cleanupResult', locale)}
              </span>
              <strong>{formatBytes(cleanupPreview.deletedBytes)}</strong>
              <small>
                {cleanupPreview.deletedFiles} {t('settings.storage.files', locale)} /{' '}
                {t('settings.storage.protectedEvidence', locale)} {formatBytes(cleanupPreview.protectedBytes)}
              </small>
            </div>
          ) : null}
        </section>
        <PolicyLine titleKey="settings.storage.recentMessages" value={`${settingsDefaults.recentMessages}`} />
        <PolicyLine titleKey="settings.storage.logs" value={`${settingsDefaults.logRetentionDays}d`} />
        <PolicyLine titleKey="settings.storage.screenshots" value={`${settingsDefaults.screenshotRetentionDays}d`} />
      </div>
    </>
  );
}

function MemorySettings() {
  return (
    <>
      <SettingsPaneHeader titleKey="settings.memory.title" bodyKey="settings.memory.body" />
      <div className="settings-section-list">
        <PolicyLine titleKey="settings.memory.recentWindow" valueKey="settings.memory.recentWindowValue" />
        <PolicyLine titleKey="settings.memory.longTerm" valueKey="settings.memory.longTermValue" />
        <PolicyLine titleKey="settings.memory.preference" valueKey="settings.memory.preferenceValue" />
      </div>
    </>
  );
}

function AppearanceSettings() {
  const locale = useAppStore((state) => state.locale);
  const theme = useAppStore((state) => state.theme);
  const compactMode = useAppStore((state) => state.compactMode);
  const highContrastMode = useAppStore((state) => state.highContrastMode);
  const setTheme = useAppStore((state) => state.setTheme);
  const setCompactMode = useAppStore((state) => state.setCompactMode);
  const setHighContrastMode = useAppStore((state) => state.setHighContrastMode);

  return (
    <>
      <SettingsPaneHeader titleKey="settings.appearance.title" bodyKey="settings.appearance.body" />
      <div className="settings-section-list">
        <section className="settings-line-section vertical">
          <strong>{t('settings.appearance.theme', locale)}</strong>
          <div className="segmented-control left">
            <button type="button" className={theme === 'minimal' ? 'active' : ''} onClick={() => setTheme('minimal')}>
              {t('settings.appearance.minimal', locale)}
            </button>
            <button type="button" className={theme === 'dark' ? 'active' : ''} onClick={() => setTheme('dark')}>
              {t('settings.appearance.dark', locale)}
            </button>
            <button
              type="button"
              className={theme === 'highContrast' ? 'active' : ''}
              onClick={() => setTheme('highContrast')}
            >
              {t('settings.appearance.highContrast', locale)}
            </button>
          </div>
        </section>
        <label className="toggle-row">
          <input type="checkbox" checked={compactMode} onChange={(event) => setCompactMode(event.target.checked)} />
          <span>{t('settings.appearance.compact', locale)}</span>
        </label>
        <label className="toggle-row">
          <input
            type="checkbox"
            checked={highContrastMode}
            onChange={(event) => setHighContrastMode(event.target.checked)}
          />
          <span>{t('settings.appearance.highContrastMode', locale)}</span>
        </label>
      </div>
    </>
  );
}

function LanguageSettings() {
  const locale = useAppStore((state) => state.locale);
  const setLocale = useAppStore((state) => state.setLocale);

  return (
    <>
      <SettingsPaneHeader titleKey="settings.language.title" bodyKey="settings.language.body" />
      <div className="settings-section-list">
        <section className="settings-line-section vertical">
          <strong>{t('settings.language.ui', locale)}</strong>
          <div className="segmented-control left">
            <button type="button" className={locale === 'zh-CN' ? 'active' : ''} onClick={() => setLocale('zh-CN')}>
              中文
            </button>
            <button type="button" className={locale === 'en-US' ? 'active' : ''} onClick={() => setLocale('en-US')}>
              English
            </button>
          </div>
        </section>
        <PolicyLine titleKey="settings.language.agent" valueKey="settings.language.followUi" />
        <PolicyLine titleKey="settings.language.proofPack" valueKey="settings.language.followUi" />
      </div>
    </>
  );
}

function PlaceholderSettings({ category }: { category: SettingsCategory }) {
  return (
    <>
      <SettingsPaneHeader titleKey={`settings.categories.${category}`} bodyKey="settings.placeholder.body" />
      <div className="settings-section-list">
        <PolicyLine titleKey="settings.permissions.approvals" valueKey="settings.permissions.approvalsValue" />
        <PolicyLine titleKey="settings.storage.worktreeRoot" valueKey="settings.storage.runtimeManaged" />
      </div>
    </>
  );
}

function ToggleLine({
  titleKey,
  bodyKey,
  enabled,
}: {
  titleKey: string;
  bodyKey: string;
  enabled: boolean;
}) {
  const locale = useAppStore((state) => state.locale);

  return (
    <section className="settings-line-section">
      <span>
        <strong>{t(titleKey, locale)}</strong>
        <small>{t(bodyKey, locale)}</small>
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={enabled}
        className={cn('settings-toggle-switch', enabled && 'active')}
      >
        <span />
      </button>
    </section>
  );
}

function PolicyLine({ titleKey, valueKey, value }: { titleKey: string; valueKey?: string; value?: string }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <section className="settings-line-section">
      <span>
        <strong>{t(titleKey, locale)}</strong>
      </span>
      <em>{value ?? (valueKey ? t(valueKey, locale) : '')}</em>
    </section>
  );
}

function UsageMetric({ labelKey, value }: { labelKey: string; value?: number }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <span className="settings-byte-value">
      <small>{t(labelKey, locale)}</small>
      <strong>{typeof value === 'number' ? formatBytes(value) : t('settings.storage.loading', locale)}</strong>
    </span>
  );
}

function StatusPill({ status, label }: { status: string; label: string }) {
  return <em className={cn('settings-status-pill', `status-${status}`)}>{label}</em>;
}

function formatBytes(value: number) {
  if (value < 1024) {
    return `${value} B`;
  }

  const units = ['KB', 'MB', 'GB', 'TB'];
  let size = value / 1024;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  return `${size.toFixed(size >= 10 ? 1 : 2)} ${units[unitIndex]}`;
}

function MessageBubbleIcon() {
  return (
    <svg className="h-5 w-5" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M7.5 18.25H7A4.75 4.75 0 0 1 2.25 13.5v-2A4.75 4.75 0 0 1 7 6.75h.5M10.5 17.25H15A4.75 4.75 0 0 0 19.75 12.5v-2A4.75 4.75 0 0 0 15 5.75h-4.5A4.75 4.75 0 0 0 5.75 10.5v2a4.73 4.73 0 0 0 1.33 3.29l-.58 2.46 2.52-.86c.47.14.96.21 1.48.21Z"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}
