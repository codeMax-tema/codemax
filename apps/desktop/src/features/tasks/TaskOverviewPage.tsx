import {
  Check,
  ChevronDown,
  CircleDot,
  Code2,
  Command,
  FolderOpen,
  GitBranch,
  Github,
  Laptop,
  ListFilter,
  Minus,
  MoreHorizontal,
  PanelRight,
  Plus,
  SendHorizontal,
  SlidersHorizontal,
  TerminalSquare,
} from 'lucide-react';

import { Button } from '@/components/ui/button';
import { t } from '@/i18n';
import { useAppStore } from '@/state/appStore';

const commandRuns = [
  {
    id: 'branch',
    label: 'git branch --show-current',
    command: 'git branch --show-current',
    output: 'codex/s6-codex-like-ui',
  },
  {
    id: 'status',
    label: 'git status --short',
    command: 'git status --short',
    output:
      ' M apps/desktop/src/app/App.tsx\n M apps/desktop/src/styles/global.css\n?? apps/desktop/src/features/tasks/NewTaskDialog.tsx\n?? apps/desktop/src/features/tasks/TaskOverviewPage.tsx\n?? tests/frontend/',
  },
  {
    id: 'check',
    label: 'npm run check',
    command: 'npm run check',
    output: 'Architecture contract passed with 50 required files.\nS6 UI contract verified',
  },
];

const changedFiles = [
  {
    path: 'apps/desktop/src/features/tasks/TaskOverviewPage.tsx',
    summaryKey: 'tasks.execution.diff.taskOverview',
    added: 214,
    removed: 32,
  },
  {
    path: 'apps/desktop/src/features/tasks/NewTaskDialog.tsx',
    summaryKey: 'tasks.execution.diff.newTaskDialog',
    added: 188,
    removed: 24,
  },
  {
    path: 'apps/desktop/src/styles/global.css',
    summaryKey: 'tasks.execution.diff.styles',
    added: 620,
    removed: 44,
  },
  {
    path: 'apps/desktop/src/i18n/locales/zh-CN.json',
    summaryKey: 'tasks.execution.diff.i18n',
    added: 90,
    removed: 8,
  },
];

export function TaskOverviewPage() {
  const locale = useAppStore((state) => state.locale);
  const setNewTaskDialogOpen = useAppStore((state) => state.setNewTaskDialogOpen);

  return (
    <div className="codex-execution-layout">
      <header className="execution-topbar">
        <div className="execution-topbar-title">
          <TerminalSquare className="h-4 w-4" aria-hidden="true" />
          <h3>{t('tasks.execution.title', locale)}</h3>
          <button type="button" aria-label={t('tasks.execution.more', locale)}>
            <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <div className="execution-topbar-actions">
          <button type="button">
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.openLocation', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.filters', locale)}>
            <ListFilter className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.layoutCompact', locale)}>
            <SlidersHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.environmentToggle', locale)}>
            <PanelRight className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
      </header>

      <section className="codex-run-transcript" aria-label={t('tasks.chat.thread', locale)}>
        <header className="execution-thread-header">
          <div>
            <h3>{t('tasks.execution.title', locale)}</h3>
            <p>{t('tasks.execution.subtitle', locale)}</p>
          </div>
          <button type="button" aria-label={t('tasks.execution.more', locale)}>
            <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
        </header>

        <article className="execution-message">
          <p>{t('tasks.execution.lead', locale)}</p>
        </article>

        <section className="execution-section">
          <button type="button" className="execution-collapse">
            <TerminalSquare className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.commands', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="command-run-list">
            {commandRuns.map((run) => (
              <CommandRunCard key={run.id} label={run.label} command={run.command} output={run.output} />
            ))}
          </div>
        </section>

        <section className="code-change-panel">
          <div className="code-change-heading">
            <div>
              <span>{t('tasks.execution.codeChanges', locale)}</span>
              <strong>+3,036 -22</strong>
            </div>
            <Button type="button" size="sm" variant="secondary">
              {t('tasks.execution.reviewDiff', locale)}
            </Button>
          </div>
          <div className="diff-file-list">
            {changedFiles.map((file) => (
              <div className="diff-file-row" key={file.path}>
                <Code2 className="h-4 w-4" aria-hidden="true" />
                <span>
                  <strong>{file.path}</strong>
                  <small>{t(file.summaryKey, locale)}</small>
                </span>
                <em>
                  <Plus className="h-3.5 w-3.5" aria-hidden="true" />
                  {file.added}
                  <Minus className="h-3.5 w-3.5" aria-hidden="true" />
                  {file.removed}
                </em>
              </div>
            ))}
          </div>
          <pre className="diff-preview execution-code-diff-preview" aria-label={t('tasks.execution.diffPreview', locale)}>
            <code>{`+ <section className="codex-execution-layout">
+   <CommandRunCard command="npm run check" />
+   <section className="code-change-panel">
- <div className="codex-chat-surface">`}</code>
          </pre>
        </section>
      </section>

      <aside className="environment-panel" aria-label={t('tasks.environment.title', locale)}>
        <div className="environment-card">
          <header>
            <span>{t('tasks.environment.title', locale)}</span>
            <button type="button" aria-label={t('tasks.environment.add', locale)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
            </button>
          </header>
          <div className="environment-list">
            <EnvironmentRow icon={Code2} labelKey="tasks.environment.changes" value="+3,036 -22" accent />
            <EnvironmentRow icon={Laptop} labelKey="tasks.environment.local" value={t('tasks.environment.localMode', locale)} />
            <EnvironmentRow icon={GitBranch} labelKey="tasks.environment.branch" value="codex/s6-codex-like-ui" />
            <EnvironmentRow icon={CircleDot} labelKey="tasks.environment.commit" value={t('tasks.environment.commitValue', locale)} />
            <EnvironmentRow icon={Github} labelKey="tasks.environment.github" value={t('tasks.environment.githubValue', locale)} muted />
          </div>
          <div className="environment-source">
            <strong>{t('tasks.environment.sources', locale)}</strong>
            <span>{t('tasks.environment.noSources', locale)}</span>
          </div>
        </div>
      </aside>

      <section className="execution-followup-composer" aria-label={t('tasks.execution.followup', locale)}>
        <button type="button" onClick={() => setNewTaskDialogOpen(true)}>
          {t('tasks.execution.followupPlaceholder', locale)}
        </button>
        <div>
          <span>
            <Plus className="h-4 w-4" aria-hidden="true" />
            {t('tasks.composer.attach', locale)}
          </span>
          <span>{t('tasks.new.permissions.network', locale)}</span>
          <span>5.5 {t('tasks.new.strength.deep', locale)}</span>
          <Button type="button" size="icon" onClick={() => setNewTaskDialogOpen(true)}>
            <SendHorizontal className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      </section>
    </div>
  );
}

function CommandRunCard({ label, command, output }: { label: string; command: string; output: string }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <article className="command-run-card">
      <button type="button" className="command-run-summary">
        <Command className="h-4 w-4" aria-hidden="true" />
        {t('tasks.execution.ran', locale)} {label}
        <ChevronDown className="h-4 w-4" aria-hidden="true" />
      </button>
      <pre className="command-output-block">
        <code>{`$ ${command}\n\n${output}`}</code>
      </pre>
      <div className="command-success">
        <Check className="h-4 w-4" aria-hidden="true" />
        {t('tasks.execution.success', locale)}
      </div>
    </article>
  );
}

function EnvironmentRow({
  icon: Icon,
  labelKey,
  value,
  accent,
  muted,
}: {
  icon: typeof Code2;
  labelKey: string;
  value: string;
  accent?: boolean;
  muted?: boolean;
}) {
  const locale = useAppStore((state) => state.locale);

  return (
    <div className="environment-row">
      <Icon className="h-4 w-4" aria-hidden="true" />
      <span>{t(labelKey, locale)}</span>
      <em className={accent ? 'accent' : muted ? 'muted' : ''}>{value}</em>
    </div>
  );
}
