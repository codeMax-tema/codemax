import { Command, FolderOpen, Layers3, Search } from 'lucide-react';
import { useDeferredValue, useEffect, useMemo, useState } from 'react';

import { getSkillSources } from '@/api/tauriClient';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';
import type { SkillSource } from '@/types/domain';

type SkillSourceTab = 'project' | 'workspace' | 'global' | 'builtIn';

const sourceTabs: Array<{ id: SkillSourceTab; labelKey: string }> = [
  { id: 'project', labelKey: 'skills.source.project' },
  { id: 'workspace', labelKey: 'skills.source.workspace' },
  { id: 'global', labelKey: 'skills.source.global' },
  { id: 'builtIn', labelKey: 'skills.source.builtIn' },
];
const skillFolderHint = '.codemax/skills';

export function SkillsPage() {
  const locale = useAppStore((state) => state.locale);
  const currentRepository = useAppStore((state) => state.currentRepository);
  const [activeTab, setActiveTab] = useState<SkillSourceTab>('project');
  const [searchQuery, setSearchQuery] = useState('');
  const [sourceItems, setSourceItems] = useState<SkillSource[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const deferredSearchQuery = useDeferredValue(searchQuery);

  useEffect(() => {
    let cancelled = false;

    getSkillSources(currentRepository?.path ?? null)
      .then((nextSources) => {
        if (cancelled) {
          return;
        }
        setSourceItems(nextSources);
        setLoadError(null);
      })
      .catch((error: unknown) => {
        if (cancelled) {
          return;
        }
        setSourceItems([]);
        setLoadError(error instanceof Error ? error.message : String(error));
      });

    return () => {
      cancelled = true;
    };
  }, [currentRepository?.path]);

  const sourceRows = useMemo(() => {
    const byId = new Map(sourceItems.map((item) => [item.id, item] as const));
    return {
      project: byId.get('project'),
      workspace: byId.get('workspace'),
      global: byId.get('global'),
      builtIn: byId.get('builtIn'),
    } satisfies Record<SkillSourceTab, SkillSource | undefined>;
  }, [sourceItems]);

  const activeSource = sourceRows[activeTab];
  const activeSourcePath = activeSource?.path ?? t('skills.note.selectProject', locale);
  const activeSourceStatus = activeSource?.status ?? 'unavailable';
  const activeSourceCount = activeSource?.skillCount ?? 0;
  const filteredEntries = useMemo(() => {
    const query = deferredSearchQuery.trim().toLowerCase();
    const entries = activeSource?.entries ?? [];
    if (!query) {
      return entries;
    }

    return entries.filter((skill) =>
      [skill.name, skill.description, skill.path].some((field) =>
        field.toLowerCase().includes(query),
      ),
    );
  }, [activeSource?.entries, deferredSearchQuery]);

  return (
    <div className="skills-page">
      <header className="skills-header">
        <div>
          <p className="eyebrow">{t('sidebar.skills', locale)}</p>
          <h3>{t('skills.title', locale)}</h3>
          <p>{t('skills.subtitle', locale)}</p>
        </div>
      </header>

      <section className="skills-source-tabs" aria-label={t('skills.title', locale)}>
        {sourceTabs.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className={cn(activeTab === tab.id && 'active')}
            onClick={() => setActiveTab(tab.id)}
          >
            {t(tab.labelKey, locale)}
          </button>
        ))}
      </section>

      <label className="skills-search-box">
        <Search className="h-4 w-4" aria-hidden="true" />
        <input
          className="skills-search-input"
          type="search"
          value={searchQuery}
          onChange={(event) => setSearchQuery(event.target.value)}
          placeholder={t('skills.searchPlaceholder', locale)}
          aria-label={t('skills.searchPlaceholder', locale)}
        />
      </label>

      <section className="skills-list">
        <article className="skills-source-card">
          <header>
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            <strong>{t(`skills.source.${activeTab}`, locale)}</strong>
          </header>
          <div className="skills-source-meta">
            <span className={cn('skills-source-status', `is-${activeSourceStatus}`)}>
              {t(`skills.status.${activeSourceStatus}`, locale)}
            </span>
            <span className="skills-count-badge">
              {activeSourceCount} {t('skills.countLabel', locale)}
            </span>
          </div>
          <code>{activeSourcePath}</code>
          {loadError ? <p>{loadError}</p> : null}
        </article>

        {filteredEntries.length ? (
          <section className="skills-entry-list">
            {filteredEntries.map((skill) => (
              <article key={skill.id} className="skills-entry-row">
                <div>
                  <strong>{skill.name}</strong>
                  <p data-skill-field="skill.description">{skill.description || t('skills.emptyHint', locale)}</p>
                  <small>{skill.path}</small>
                </div>
                <Command className="h-4 w-4" aria-hidden="true" />
              </article>
            ))}
          </section>
        ) : activeSource?.entries.length ? (
          <article className="skills-empty-state skills-no-results">
            <Layers3 className="h-5 w-5" aria-hidden="true" />
            <div>
              <strong>{t('skills.noMatchesTitle', locale)}</strong>
              <p>{t('skills.noMatchesHint', locale)}</p>
              <small>{searchQuery}</small>
            </div>
            <Command className="h-4 w-4" aria-hidden="true" />
          </article>
        ) : (
          <article className="skills-empty-state">
            <Layers3 className="h-5 w-5" aria-hidden="true" />
            <div>
              <strong>{t('skills.emptyTitle', locale)}</strong>
              <p>{t('skills.emptyHint', locale)}</p>
              <small>{activeSourcePath || skillFolderHint}</small>
            </div>
            <Command className="h-4 w-4" aria-hidden="true" />
          </article>
        )}
      </section>
    </div>
  );
}
