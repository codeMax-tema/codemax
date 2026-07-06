import { create } from 'zustand';

import type { Locale } from '@/i18n';
import type { RepositorySummary } from '@/types/domain';

export type ThemeName = 'minimal' | 'compact' | 'dark';

interface AppState {
  locale: Locale;
  theme: ThemeName;
  currentRepository: RepositorySummary | null;
  setLocale: (locale: Locale) => void;
  setTheme: (theme: ThemeName) => void;
  setCurrentRepository: (repository: RepositorySummary | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  locale: 'zh-CN',
  theme: 'minimal',
  currentRepository: null,
  setLocale: (locale) => set({ locale }),
  setTheme: (theme) => set({ theme }),
  setCurrentRepository: (repository) => set({ currentRepository: repository }),
}));

