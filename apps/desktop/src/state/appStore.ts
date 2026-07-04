import { create } from 'zustand';

import type { Locale } from '@/i18n';

export type ThemeName = 'minimal' | 'compact' | 'dark';

interface AppState {
  locale: Locale;
  theme: ThemeName;
  setLocale: (locale: Locale) => void;
  setTheme: (theme: ThemeName) => void;
}

export const useAppStore = create<AppState>((set) => ({
  locale: 'zh-CN',
  theme: 'minimal',
  setLocale: (locale) => set({ locale }),
  setTheme: (theme) => set({ theme }),
}));

