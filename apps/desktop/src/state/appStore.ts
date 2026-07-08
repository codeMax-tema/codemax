import { create } from 'zustand';

import { getAppSetting, setAppSetting } from '@/api/tauriClient';
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
  hydratePreferences: () => Promise<void>;
  setLocale: (locale: Locale) => void;
  setTheme: (theme: ThemeName) => void;
  setCompactMode: (enabled: boolean) => void;
  setHighContrastMode: (enabled: boolean) => void;
  setCurrentRoute: (route: AppRouteId) => void;
  setCurrentRepository: (repository: RepositorySummary | null) => void;
  setSelectedTaskId: (taskId: string | null) => void;
  setNewTaskDialogOpen: (open: boolean) => void;
}

const preferenceKeys = {
  locale: 'ui.locale',
  theme: 'ui.theme',
  compactMode: 'ui.compactMode',
  highContrastMode: 'ui.highContrastMode',
} as const;

export const useAppStore = create<AppState>((set) => ({
  locale: 'zh-CN',
  theme: 'minimal',
  compactMode: false,
  highContrastMode: false,
  currentRoute: getInitialRoute(),
  currentRepository: null,
  selectedTaskId: null,
  newTaskDialogOpen: getInitialDialogOpen(),
  hydratePreferences: async () => {
    const [locale, theme, compactMode, highContrastMode] = await Promise.all([
      readPersistedPreference(preferenceKeys.locale),
      readPersistedPreference(preferenceKeys.theme),
      readPersistedPreference(preferenceKeys.compactMode),
      readPersistedPreference(preferenceKeys.highContrastMode),
    ]);

    set({
      ...(isLocale(locale) ? { locale } : {}),
      ...(isThemeName(theme) ? { theme } : {}),
      ...(isBooleanString(compactMode) ? { compactMode: compactMode === 'true' } : {}),
      ...(isBooleanString(highContrastMode) ? { highContrastMode: highContrastMode === 'true' } : {}),
    });
  },
  setLocale: (locale) => {
    set({ locale });
    persistPreference(preferenceKeys.locale, locale);
  },
  setTheme: (theme) => {
    set({ theme });
    persistPreference(preferenceKeys.theme, theme);
  },
  setCompactMode: (enabled) => {
    set({ compactMode: enabled });
    persistPreference(preferenceKeys.compactMode, String(enabled));
  },
  setHighContrastMode: (enabled) => {
    set({ highContrastMode: enabled });
    persistPreference(preferenceKeys.highContrastMode, String(enabled));
  },
  setCurrentRoute: (route) => set({ currentRoute: route }),
  setCurrentRepository: (repository) => set({ currentRepository: repository }),
  setSelectedTaskId: (taskId) => set({ selectedTaskId: taskId }),
  setNewTaskDialogOpen: (open) => set({ newTaskDialogOpen: open }),
}));

async function readPersistedPreference(key: string): Promise<string | null> {
  try {
    const setting = await getAppSetting(key);
    return setting.value ?? null;
  } catch {
    return null;
  }
}

function persistPreference(key: string, value: string) {
  void setAppSetting(key, value).catch(() => undefined);
}

function isLocale(value: string | null): value is Locale {
  return value === 'zh-CN' || value === 'en-US';
}

function isThemeName(value: string | null): value is ThemeName {
  return value === 'minimal' || value === 'dark' || value === 'highContrast';
}

function isBooleanString(value: string | null): value is 'true' | 'false' {
  return value === 'true' || value === 'false';
}

