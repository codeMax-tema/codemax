import { t } from '@/i18n';
import { useAppStore } from '@/state/appStore';
import { RepositoryPage } from '@/features/repositories/RepositoryPage';

export function App() {
  const locale = useAppStore((state) => state.locale);

  return (
    <main className="min-h-screen bg-background text-foreground" data-testid="app-root">
      <h1 className="sr-only">{t('app.title', locale)}</h1>
      <div className="border-b border-border px-6 py-3 text-sm font-medium">{t('app.title', locale)}</div>
      <RepositoryPage />
    </main>
  );
}
