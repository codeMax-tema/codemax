import { create } from 'zustand';

import type { Locale } from '@/i18n';
import type { RepositorySummary } from '@/types/domain';

export type AppRouteId = 'repositories' | 'tasks' | 'approvals' | 'settings';
export type ThemeName = 'minimal' | 'dark' | 'highContrast';

const routeIds: AppRouteId[] = ['repositories', 'tasks', 'approvals', 'settings'];

export function getInitialRoute(): AppRouteId {
  if (typeof window === 'undefined') {
    return 'tasks';
  }

  const hashRoute = window.location.hash.replace('#', '');
  return routeIds.includes(hashRoute as AppRouteId) ? (hashRoute as AppRouteId) : 'tasks';
}

export function getInitialDialogOpen(): boolean {
  if (typeof window === 'undefined') {
    return false;
  }

  return new URLSearchParams(window.location.search).get('newTask') === '1';
}

interface AppState {
  locale: Locale;
  theme: ThemeName;
  compactMode: boolean;
  highContrastMode: boolean;
  currentRoute: AppRouteId;
  currentRepository: RepositorySummary | null;
  selectedTaskId: string | null;
  newTaskDialogOpen: boolean;
  setLocale: (locale: Locale) => void;
  setTheme: (theme: ThemeName) => void;
  setCompactMode: (enabled: boolean) => void;
  setHighContrastMode: (enabled: boolean) => void;
  setCurrentRoute: (route: AppRouteId) => void;
  setCurrentRepository: (repository: RepositorySummary | null) => void;
  setSelectedTaskId: (taskId: string | null) => void;
  setNewTaskDialogOpen: (open: boolean) => void;
}

export const useAppStore = create<AppState>((set) => ({
  locale: 'zh-CN',
  theme: 'minimal',
  compactMode: false,
  highContrastMode: false,
  currentRoute: getInitialRoute(),
  currentRepository: null,
  selectedTaskId: null,
  newTaskDialogOpen: getInitialDialogOpen(),
  setLocale: (locale) => set({ locale }),
  setTheme: (theme) => set({ theme }),
  setCompactMode: (enabled) => set({ compactMode: enabled }),
  setHighContrastMode: (enabled) => set({ highContrastMode: enabled }),
  setCurrentRoute: (route) => set({ currentRoute: route }),
  setCurrentRepository: (repository) => set({ currentRepository: repository }),
  setSelectedTaskId: (taskId) => set({ selectedTaskId: taskId }),
  setNewTaskDialogOpen: (open) => set({ newTaskDialogOpen: open }),
}));

