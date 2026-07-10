import {
  ArrowLeft,
  ArrowRight,
  CalendarClock,
  HardDrive,
  MessageSquareText,
  Minus,
  PanelLeft,
  Search,
  Square,
  SquarePen,
  X,
  Zap,
} from 'lucide-react';
import { useEffect, useState } from 'react';

import { listTasks } from '@/api/tauriClient';
import { ApprovalsPage } from '@/features/approvals/ApprovalsPage';
import { HomePage } from '@/features/home/HomePage';
import { RepositoryPage } from '@/features/repositories/RepositoryPage';
import { SearchPage } from '@/features/search/SearchPage';
import { SettingsPage } from '@/features/settings/SettingsPage';
import { SkillsPage } from '@/features/skills/SkillsPage';
import { NewTaskDialog } from '@/features/tasks/NewTaskDialog';
import { TaskOverviewPage } from '@/features/tasks/TaskOverviewPage';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { AppRouteId, useAppStore } from '@/state/appStore';
import type { TaskStatus, TaskSummary } from '@/types/domain';

const taskStatusFilters: Array<{ id: 'all' | TaskStatus; labelKey: string }> = [
  { id: 'all', labelKey: 'tasks.list.filterAll' },
  { id: 'queued', labelKey: 'status.queued' },
  { id: 'planning', labelKey: 'status.planning' },
  { id: 'editing', labelKey: 'status.editing' },
  { id: 'validating', labelKey: 'status.validating' },
  { id: 'repairing', labelKey: 'status.repairing' },
  { id: 'awaitingApproval', labelKey: 'status.awaitingApproval' },
  { id: 'awaitingReview', labelKey: 'status.awaitingReview' },
  { id: 'readyToMerge', labelKey: 'status.readyToMerge' },
  { id: 'needsIntervention', labelKey: 'status.needsIntervention' },
  { id: 'merged', labelKey: 'status.merged' },
  { id: 'failed', labelKey: 'status.failed' },
  { id: 'cancelled', labelKey: 'status.cancelled' },
];

export function App() {
  const locale = useAppStore((state) => state.locale);
  const currentRoute = useAppStore((state) => state.currentRoute);
  const currentRepository = useAppStore((state) => state.currentRepository);
  const selectedTaskId = useAppStore((state) => state.selectedTaskId);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setSelectedTaskId = useAppStore((state) => state.setSelectedTaskId);
  const hydratePreferences = useAppStore((state) => state.hydratePreferences);
  const theme = useAppStore((state) => state.theme);
  const compactMode = useAppStore((state) => state.compactMode);
  const highContrastMode = useAppStore((state) => state.highContrastMode);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [taskStatusFilter, setTaskStatusFilter] = useState<'all' | TaskStatus>('all');
  const [taskListError, setTaskListError] = useState<string | null>(null);

  useEffect(() => {
    void hydratePreferences();
  }, [hydratePreferences]);

  useEffect(() => {
    let cancelled = false;

    async function loadTasks() {
      try {
        const taskList = await listTasks({
          repositoryPath: currentRepository?.path ?? null,
          status: taskStatusFilter === 'all' ? null : taskStatusFilter,
          limit: 50,
        });
        if (cancelled) {
          return;
        }

        setTasks(taskList);
        setTaskListError(null);

        if (taskList.length === 0 && selectedTaskId) {
          setSelectedTaskId(null);
          return;
        }

        if (selectedTaskId && taskList.some((task) => task.id === selectedTaskId)) {
          return;
        }

        if (taskList[0] && currentRoute === 'tasks') {
          setSelectedTaskId(taskList[0].id);
        }
      } catch (error) {
        if (!cancelled) {
          setTasks([]);
          setTaskListError(normalizeTaskListError(error));
        }
      }
    }

    void loadTasks();

    return () => {
      cancelled = true;
    };
  }, [currentRepository?.path, currentRoute, selectedTaskId, setSelectedTaskId, taskStatusFilter]);

  const showCanvasHeader = currentRoute !== 'home' && currentRoute !== 'search';

  return (
    <main
      className={cn(
        'app-shell codex-desktop-shell codemax-minimal-shell search-command-palette min-h-screen bg-background text-foreground',
        `theme-${theme}`,
        compactMode && 'is-compact',
        highContrastMode && 'is-high-contrast',
      )}
      data-testid="app-root"
    >
      <h1 className="sr-only">{t('app.title', locale)}</h1>

      <header className="codex-window-menubar" aria-label={t('app.windowMenu', locale)}>
        <div className="codex-window-menu-left">
          <button type="button" className="codex-window-icon-button" aria-label={t('app.sidebar', locale)}>
            <PanelLeft className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" className="codex-window-icon-button" aria-label={t('app.back', locale)}>
            <ArrowLeft className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" className="codex-window-icon-button" aria-label={t('app.forward', locale)} disabled>
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </button>
          <nav className="codex-menu-items" aria-label={t('app.windowMenu', locale)}>
            <button type="button">{t('app.menu.file', locale)}</button>
            <button type="button">{t('app.menu.edit', locale)}</button>
            <button type="button">{t('app.menu.view', locale)}</button>
            <button type="button">{t('app.menu.help', locale)}</button>
          </nav>
        </div>
        <div className="codex-window-controls" aria-hidden="true">
          <Minus className="h-4 w-4" />
          <Square className="h-3.5 w-3.5" />
          <X className="h-4 w-4" />
        </div>
      </header>

      <div className="codex-app-body">
        <aside className="app-sidebar codex-thread-sidebar" aria-label={t('app.sidebar', locale)}>
          <nav className="codex-sidebar-section codex-quick-nav">
            <button
              type="button"
              className="codex-sidebar-action codex-new-thread-button"
              onClick={() => setCurrentRoute('home')}
            >
              <SquarePen className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.newChat', locale)}</span>
            </button>
            <button
              type="button"
              className={cn('codex-sidebar-action', currentRoute === 'search' && 'active')}
              onClick={() => setCurrentRoute('search')}
            >
              <Search className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.search', locale)}</span>
            </button>
            <button
              type="button"
              className={cn('codex-sidebar-action', currentRoute === 'approvals' && 'active')}
              onClick={() => setCurrentRoute('approvals')}
            >
              <CalendarClock className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.scheduled', locale)}</span>
            </button>
            <button
              type="button"
              className={cn('codex-sidebar-action', currentRoute === 'skills' && 'active')}
              onClick={() => setCurrentRoute('skills')}
            >
              <Zap className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.skills', locale)}</span>
            </button>
          </nav>

          <section className="codex-sidebar-section">
            <p className="codex-section-label">{t('sidebar.projects', locale)}</p>
            <button
              type="button"
              className="codex-project-root"
              onClick={() => setCurrentRoute('repositories')}
            >
              <HardDrive className="h-4 w-4" aria-hidden="true" />
              <span>{currentRepository?.name ?? t('repository.emptyShort', locale)}</span>
            </button>
            <label className="codex-task-filter">
              <span>{t('tasks.list.statusFilter', locale)}</span>
              <select
                value={taskStatusFilter}
                onChange={(event) => setTaskStatusFilter(event.target.value as 'all' | TaskStatus)}
              >
                {taskStatusFilters.map((filter) => (
                  <option key={filter.id} value={filter.id}>
                    {t(filter.labelKey, locale)}
                  </option>
                ))}
              </select>
            </label>
            <div className="codex-thread-list" aria-label={t('tasks.list.title', locale)}>
              {taskListError ? (
                <div className="codex-thread-empty" role="alert">
                  {taskListError}
                </div>
              ) : tasks.length > 0 ? (
                tasks.map((task) => (
                  <button
                    key={task.id}
                    type="button"
                    className={cn('codex-thread-item', selectedTaskId === task.id && currentRoute === 'tasks' && 'active')}
                    onClick={() => {
                      setSelectedTaskId(task.id);
                      setCurrentRoute('tasks');
                    }}
                  >
                    <MessageSquareText className="h-4 w-4" aria-hidden="true" />
                    <span>{task.title}</span>
                    <time>{formatTaskTime(task.updatedAt)}</time>
                  </button>
                ))
              ) : (
                <div className="codex-thread-empty">{t('tasks.list.empty', locale)}</div>
              )}
            </div>
          </section>

          <section className="codex-sidebar-section">
            <p className="codex-section-label">{t('sidebar.conversations', locale)}</p>
            <div className="codex-thread-empty">{t('tasks.list.emptyConversations', locale)}</div>
          </section>

          <div className="codex-sidebar-footer">
            <button type="button" className="codex-account-button" onClick={() => setCurrentRoute('settings')}>
              <span className="codex-account-avatar">设</span>
              <span>
                <strong>{t('nav.settings', locale)}</strong>
                <small>{t('sidebar.account', locale)}</small>
              </span>
            </button>
          </div>
        </aside>

        <section className="app-main codex-main-pane">
          {showCanvasHeader ? (
            <header className="app-topbar codex-titlebar">
              <div>
                <h2>{t(routeTitle(currentRoute), locale)}</h2>
                <p>{t(routeEyebrow(currentRoute), locale)}</p>
              </div>
              <div className="topbar-meta">
                <span>
                  {currentRepository
                    ? currentRepository.isGitRepository
                      ? currentRepository.branch ?? t('repository.noBranch', locale)
                      : t('repository.kind.directory', locale)
                    : t('repository.noBranch', locale)}
                </span>
              </div>
            </header>
          ) : null}

          <div className={cn('app-content codex-content-region', currentRoute === 'home' && 'is-home-canvas')}>
            {renderRoute(currentRoute)}
          </div>
        </section>
      </div>

      <NewTaskDialog />
    </main>
  );
}

function renderRoute(route: AppRouteId) {
  if (route === 'home') {
    return <HomePage />;
  }

  if (route === 'search') {
    return <SearchPage />;
  }

  if (route === 'repositories') {
    return <RepositoryPage />;
  }

  if (route === 'settings') {
    return <SettingsPage />;
  }

  if (route === 'skills') {
    return <SkillsPage />;
  }

  if (route === 'approvals') {
    return <ApprovalsPage />;
  }

  return <TaskOverviewPage />;
}

function routeTitle(route: AppRouteId) {
  return {
    home: 'home.promptTitle',
    search: 'search.title',
    repositories: 'repository.title',
    tasks: 'tasks.title',
    approvals: 'approvals.title',
    settings: 'settings.title',
    skills: 'skills.title',
  }[route];
}

function routeEyebrow(route: AppRouteId) {
  return {
    home: 'sidebar.newChat',
    search: 'sidebar.search',
    repositories: 'nav.repositories',
    tasks: 'nav.tasks',
    approvals: 'nav.approvals',
    settings: 'nav.settings',
    skills: 'sidebar.skills',
  }[route];
}

function formatTaskTime(value: string) {
  return value.replace('T', ' ').replace(/\.\d+Z?$/, '').replace(/Z$/, '');
}

function normalizeTaskListError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'title' in error) {
    return String((error as { title: unknown }).title);
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}
