import { AlertCircle, CheckCircle2, FolderOpen, GitBranch, Loader2, RefreshCw } from 'lucide-react';
import { useState } from 'react';

import { selectRepositoryPath, validateRepositoryPath } from '@/api/tauriClient';
import { Button } from '@/components/ui/button';
import { t } from '@/i18n';
import type { Locale } from '@/i18n';
import { useAppStore } from '@/state/appStore';
import { useNotificationStore } from '@/state/notificationStore';
import type { RepositorySummary } from '@/types/domain';

interface DisplayError {
  title: string;
  description?: string;
}

export function RepositoryPage() {
  const locale = useAppStore((state) => state.locale);
  const repository = useAppStore((state) => state.currentRepository);
  const setCurrentRepository = useAppStore((state) => state.setCurrentRepository);
  const pushNotification = useNotificationStore((state) => state.push);
  const [isLoading, setIsLoading] = useState(false);
  const [lastError, setLastError] = useState<DisplayError | null>(null);

  async function loadRepository(path: string) {
    setIsLoading(true);
    setLastError(null);

    try {
      const nextRepository = await validateRepositoryPath(path);
      setCurrentRepository(nextRepository);
      pushNotification({
        kind: 'info',
        title: t('repository.loaded', locale),
        description: nextRepository.branch,
      });
    } catch (error) {
      const normalized = normalizeDisplayError(error);
      setLastError(normalized);
      pushNotification({
        kind: 'error',
        title: normalized.title,
        description: normalized.description,
      });
    } finally {
      setIsLoading(false);
    }
  }

  async function handleSelectRepository() {
    setIsLoading(true);
    setLastError(null);

    try {
      const selection = await selectRepositoryPath();
      if (selection) {
        await loadRepository(selection.path);
      }
    } catch (error) {
      const normalized = normalizeDisplayError(error);
      setLastError(normalized);
      pushNotification({
        kind: 'error',
        title: normalized.title,
        description: normalized.description,
      });
    } finally {
      setIsLoading(false);
    }
  }

  function handleRefreshRepository() {
    if (repository) {
      void loadRepository(repository.path);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-6 py-6">
      <section className="flex items-start justify-between gap-4 border-b border-border pb-5">
        <div>
          <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">
            {t('nav.repositories', locale)}
          </p>
          <h2 className="mt-1 text-2xl font-semibold">{t('repository.title', locale)}</h2>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button type="button" onClick={() => void handleSelectRepository()} disabled={isLoading}>
            {isLoading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <FolderOpen className="mr-2 h-4 w-4" />}
            {t('repository.select', locale)}
          </Button>
          <Button type="button" variant="secondary" onClick={handleRefreshRepository} disabled={isLoading || !repository}>
            <RefreshCw className="mr-2 h-4 w-4" />
            {t('repository.refresh', locale)}
          </Button>
        </div>
      </section>

      {lastError ? (
        <div className="flex items-start gap-3 rounded-md border border-destructive/35 bg-destructive/5 p-4 text-sm text-destructive">
          <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
          <div>
            <p className="font-medium">{lastError.title}</p>
            {lastError.description ? <p className="mt-1 text-destructive/85">{lastError.description}</p> : null}
          </div>
        </div>
      ) : null}

      {repository ? <RepositorySummaryPanel repository={repository} /> : <RepositoryEmptyState locale={locale} />}
    </div>
  );
}

function RepositorySummaryPanel({ repository }: { repository: RepositorySummary }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <section className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(280px,1fr)]">
      <div className="rounded-md border border-border p-5">
        <dl className="grid gap-5 sm:grid-cols-2">
          <div>
            <dt className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t('repository.name', locale)}
            </dt>
            <dd className="mt-2 text-base font-semibold">{repository.name}</dd>
          </div>
          <div>
            <dt className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t('repository.currentBranch', locale)}
            </dt>
            <dd className="mt-2 flex min-w-0 items-center gap-2 text-base font-semibold">
              <GitBranch className="h-4 w-4 shrink-0 text-primary" />
              <span className="truncate">{repository.branch}</span>
            </dd>
          </div>
          <div className="sm:col-span-2">
            <dt className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              {t('repository.path', locale)}
            </dt>
            <dd className="mt-2 break-all rounded-md bg-muted px-3 py-2 font-mono text-sm text-muted-foreground">
              {repository.path}
            </dd>
          </div>
        </dl>
      </div>

      <div className="rounded-md border border-border p-5">
        <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
          {t('repository.workspaceStatus', locale)}
        </p>
        <div className="mt-4 flex items-center gap-3">
          {repository.dirty ? (
            <AlertCircle className="h-5 w-5 text-destructive" />
          ) : (
            <CheckCircle2 className="h-5 w-5 text-primary" />
          )}
          <span className="text-sm font-medium">
            {repository.dirty ? t('repository.status.dirty', locale) : t('repository.status.clean', locale)}
          </span>
        </div>
      </div>
    </section>
  );
}

function RepositoryEmptyState({ locale }: { locale: Locale }) {
  return (
    <section className="rounded-md border border-dashed border-border p-8">
      <div className="flex items-center gap-3 text-muted-foreground">
        <FolderOpen className="h-5 w-5" />
        <p className="text-sm font-medium">{t('repository.empty', locale)}</p>
      </div>
    </section>
  );
}

function normalizeDisplayError(error: unknown): DisplayError {
  if (typeof error === 'object' && error !== null && 'title' in error) {
    return error as DisplayError;
  }

  if (error instanceof Error) {
    return { title: error.message };
  }

  return {
    title: String(error),
  };
}
