import enUS from './locales/en-US.json';
import zhCN from './locales/zh-CN.json';

export type Locale = 'zh-CN' | 'en-US';

const dictionaries: Record<Locale, Record<string, string>> = {
  'zh-CN': zhCN,
  'en-US': enUS,
};

export const defaultLocale: Locale = 'zh-CN';
export const fallbackLocale: Locale = 'en-US';

export function t(key: string, locale: Locale = defaultLocale): string {
  return dictionaries[locale]?.[key] ?? dictionaries[fallbackLocale][key] ?? key;
}

