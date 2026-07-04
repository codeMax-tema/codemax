import { t } from '@/i18n';
import { useAppStore } from '@/state/appStore';

export function App() {
  const locale = useAppStore((state) => state.locale);

  return (
    <main className="min-h-screen bg-background text-foreground" data-testid="app-root">
      <h1 className="sr-only">{t('app.title', locale)}</h1>
    </main>
  );
}

