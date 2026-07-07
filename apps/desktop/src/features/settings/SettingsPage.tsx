import {
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
  Search,
  Settings,
  SlidersHorizontal,
  TerminalSquare,
  Workflow,
} from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { s6SettingsFixture } from '@/features/tasks/taskFixtures';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';

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

  return (
    <>
      <SettingsPaneHeader titleKey="settings.general.title" bodyKey="settings.general.body" />

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

  return (
    <>
      <SettingsPaneHeader titleKey="settings.models.title" bodyKey="settings.models.body" />
      <div className="settings-section-list">
        {s6SettingsFixture.models.map((model, index) => (
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

  return (
    <>
      <SettingsPaneHeader titleKey="settings.storage.title" bodyKey="settings.storage.body" />
      <div className="settings-section-list">
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.appData', locale)}</strong>
          <code>{s6SettingsFixture.appDataPath}</code>
        </section>
        <section className="settings-line-section vertical">
          <strong>{t('settings.storage.worktreeRoot', locale)}</strong>
          <code>{s6SettingsFixture.worktreeRoot}</code>
        </section>
        <PolicyLine titleKey="settings.storage.recentMessages" value={`${s6SettingsFixture.recentMessages}`} />
        <PolicyLine titleKey="settings.storage.logs" value={`${s6SettingsFixture.logRetentionDays}d`} />
        <PolicyLine titleKey="settings.storage.screenshots" value={`${s6SettingsFixture.screenshotRetentionDays}d`} />
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
        <PolicyLine titleKey="settings.storage.worktreeRoot" value={s6SettingsFixture.worktreeRoot} />
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
