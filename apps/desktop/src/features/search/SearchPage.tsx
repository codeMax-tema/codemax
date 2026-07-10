import { Search, Sparkles } from 'lucide-react';
import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { listTasks } from '@/api/tauriClient';
import { t } from '@/i18n';
import { useAppStore } from '@/state/appStore';
import type { TaskSummary } from '@/types/domain';

export function SearchPage() {
  const locale = useAppStore((state) => state.locale);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setSelectedTaskId = useAppStore((state) => state.setSelectedTaskId);
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [searchQuery, setSearchQuery] = useState('');
  const [loadError, setLoadError] = useState<string | null>(null);
  const deferredQuery = useDeferredValue(searchQuery);

  useEffect(() => {
    let cancelled = false;

    listTasks({ limit: 120 })
      .then((results) => {
        if (cancelled) {
          return;
        }
        setTasks(results);
        setLoadError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setTasks([]);
        setLoadError(error instanceof Error ? error.message : String(error));
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const filteredTasks = useMemo(() => {
    const query = deferredQuery.trim().toLowerCase();
    if (!query) {
      return tasks;
    }

    return tasks.filter((task) => task.title.toLowerCase().includes(query));
  }, [deferredQuery, tasks]);
  const groupedResults = useMemo(() => {
    const groups = new Map<string, TaskSummary[]>();

    filteredTasks.forEach((task) => {
      const key = task.repositoryPath;
      const existing = groups.get(key) ?? [];
      existing.push(task);
      groups.set(key, existing);
    });

    return Array.from(groups.entries());
  }, [filteredTasks]);

  return (
    <section className="search-page">
      <div className="search-command-palette">
        <header>
          <strong>{t('search.title', locale)}</strong>
        </header>

        <label className="search-input-shell">
          <Search className="h-4 w-4" aria-hidden="true" />
          <input
            type="search"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder={t('search.placeholder', locale)}
            aria-label={t('search.placeholder', locale)}
          />
        </label>

        {loadError ? <p className="search-error">{loadError}</p> : null}

        {groupedResults.length > 0 ? (
          <div className="search-result-list" aria-label={t('search.title', locale)}>
            {groupedResults.map(([repositoryPath, repositoryTasks]) => (
              <section key={repositoryPath} className="search-result-group">
                <strong className="search-result-group-title">{repositoryPath}</strong>
                {repositoryTasks.map((task) => (
                  <button
                    key={task.id}
                    type="button"
                    className="search-result-row"
                    onClick={() => {
                      setSelectedTaskId(task.id);
                      setCurrentRoute('tasks');
                    }}
                  >
                    <div>
                      <strong>{task.title}</strong>
                      <small>{task.repositoryPath}</small>
                    </div>
                    <span>{t(`status.${task.status}`, locale)}</span>
                  </button>
                ))}
              </section>
            ))}
          </div>
        ) : (
          <div className="search-empty-state">
            <Sparkles className="h-5 w-5" aria-hidden="true" />
            <div>
              <strong>{t('search.emptyTitle', locale)}</strong>
              <p>{t('search.emptyHint', locale)}</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
