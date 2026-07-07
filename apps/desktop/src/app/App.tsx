import {
  ArrowLeft,
  ArrowRight,
  CalendarClock,
  FolderGit2,
  HardDrive,
  ListChecks,
  MessageSquareText,
  Minus,
  PanelLeft,
  Plug,
  Search,
  Settings,
  ShieldAlert,
  Square,
  SquarePen,
  X,
} from 'lucide-react';

import { ApprovalsPage } from '@/features/approvals/ApprovalsPage';
import { RepositoryPage } from '@/features/repositories/RepositoryPage';
import { SettingsPage } from '@/features/settings/SettingsPage';
import { NewTaskDialog } from '@/features/tasks/NewTaskDialog';
import { TaskOverviewPage } from '@/features/tasks/TaskOverviewPage';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { AppRouteId, useAppStore } from '@/state/appStore';

const navItems: Array<{
  id: AppRouteId;
  icon: typeof ListChecks;
  labelKey: string;
}> = [
  { id: 'tasks', icon: ListChecks, labelKey: 'nav.tasks' },
  { id: 'repositories', icon: FolderGit2, labelKey: 'nav.repositories' },
  { id: 'approvals', icon: ShieldAlert, labelKey: 'nav.approvals' },
  { id: 'settings', icon: Settings, labelKey: 'nav.settings' },
];

const projectThreads = [
  { title: '清理D盘空间', age: '3 天' },
  { title: '整理D盘工具分类', age: '2 周' },
  { title: '制定纯前端桌面APP方案', age: '3 周' },
  { title: '制作 Codex 桌面宠物', age: '3 周' },
  { title: '新对话', age: '3 周' },
];

const projectRoots = ['codemax', 'LYC', 'C:', '面试题', 'Blog'];

const conversations = [
  { title: '你好', age: '23 小时' },
  { title: '个人博客网站需要后端吗', age: '1 周' },
  { title: '分析自我介绍不足', age: '2 周' },
];

export function App() {
  const locale = useAppStore((state) => state.locale);
  const currentRoute = useAppStore((state) => state.currentRoute);
  const currentRepository = useAppStore((state) => state.currentRepository);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setNewTaskDialogOpen = useAppStore((state) => state.setNewTaskDialogOpen);
  const theme = useAppStore((state) => state.theme);
  const compactMode = useAppStore((state) => state.compactMode);
  const highContrastMode = useAppStore((state) => state.highContrastMode);

  return (
    <main
      className={cn(
        'app-shell codex-desktop-shell min-h-screen bg-background text-foreground',
        `theme-${theme}`,
        compactMode && 'is-compact',
        highContrastMode && 'is-high-contrast',
        currentRoute === 'settings' && 'is-settings-route',
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
              onClick={() => setNewTaskDialogOpen(true)}
            >
              <SquarePen className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.newChat', locale)}</span>
            </button>
            <button type="button" className="codex-sidebar-action">
              <Search className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.search', locale)}</span>
            </button>
            <button type="button" className="codex-sidebar-action">
              <CalendarClock className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.scheduled', locale)}</span>
            </button>
            <button type="button" className="codex-sidebar-action">
              <Plug className="h-4 w-4" aria-hidden="true" />
              <span>{t('sidebar.plugins', locale)}</span>
            </button>
          </nav>

          <section className="codex-sidebar-section">
            <p className="codex-section-label">{t('sidebar.projects', locale)}</p>
            <button type="button" className="codex-project-root">
              <HardDrive className="h-4 w-4" aria-hidden="true" />
              <span>D:</span>
            </button>
            <div className="codex-thread-list" aria-label={t('tasks.list.title', locale)}>
              {projectThreads.map((thread, index) => (
                <button
                  key={thread.title}
                  type="button"
                  className={cn('codex-thread-item', index === 0 && 'active')}
                  onClick={() => setCurrentRoute('tasks')}
                >
                  <MessageSquareText className="h-4 w-4" aria-hidden="true" />
                  <span>{thread.title}</span>
                  <time>{thread.age}</time>
                </button>
              ))}
            </div>
            <button type="button" className="codex-sidebar-link">
              {t('sidebar.showMore', locale)}
            </button>
          </section>

          <section className="codex-sidebar-section codex-project-roots">
            {projectRoots.map((project) => (
              <button key={project} type="button" className="codex-sidebar-action" onClick={() => setCurrentRoute('repositories')}>
                <FolderGit2 className="h-4 w-4" aria-hidden="true" />
                <span>{project}</span>
              </button>
            ))}
          </section>

          <section className="codex-sidebar-section">
            <p className="codex-section-label">{t('sidebar.conversations', locale)}</p>
            {conversations.map((conversation) => (
              <button key={conversation.title} type="button" className="codex-thread-item compact">
                <MessageSquareText className="h-4 w-4" aria-hidden="true" />
                <span>{conversation.title}</span>
                <time>{conversation.age}</time>
              </button>
            ))}
          </section>

          <nav className="app-nav codex-sidebar-nav" aria-label={t('app.sidebar', locale)}>
            {navItems.map((item) => {
              const Icon = item.icon;
              return (
                <button
                  key={item.id}
                  type="button"
                  className={cn('nav-item', currentRoute === item.id && 'active')}
                  onClick={() => setCurrentRoute(item.id)}
                >
                  <Icon className="h-4 w-4" aria-hidden="true" />
                  <span>{t(item.labelKey, locale)}</span>
                </button>
              );
            })}
          </nav>

          <div className="codex-sidebar-footer">
            <button type="button" className="codex-account-button" onClick={() => setCurrentRoute('settings')}>
              <span className="codex-account-avatar">设</span>
              <span>
                <strong>{t('nav.settings', locale)}</strong>
                <small>{t('sidebar.account', locale)}</small>
              </span>
            </button>
            <span className="codex-download-pill">{t('sidebar.downloading', locale)}</span>
          </div>
        </aside>

        <section className="app-main codex-main-pane">
          <header className="app-topbar codex-titlebar">
            <div>
              <h2>{t(routeTitle(currentRoute), locale)}</h2>
              <p>{t(routeEyebrow(currentRoute), locale)}</p>
            </div>
            <div className="topbar-meta">
              <span>{currentRepository ? currentRepository.branch : t('repository.noBranch', locale)}</span>
            </div>
          </header>
          <div className="app-content codex-content-region">{renderRoute(currentRoute)}</div>
        </section>
      </div>

      <NewTaskDialog />
    </main>
  );
}

function renderRoute(route: AppRouteId) {
  if (route === 'repositories') {
    return <RepositoryPage />;
  }

  if (route === 'settings') {
    return <SettingsPage />;
  }

  if (route === 'approvals') {
    return <ApprovalsPage />;
  }

  return <TaskOverviewPage />;
}

function routeTitle(route: AppRouteId) {
  return {
    repositories: 'repository.title',
    tasks: 'tasks.title',
    approvals: 'approvals.title',
    settings: 'settings.title',
  }[route];
}

function routeEyebrow(route: AppRouteId) {
  return {
    repositories: 'nav.repositories',
    tasks: 'nav.tasks',
    approvals: 'nav.approvals',
    settings: 'nav.settings',
  }[route];
}
