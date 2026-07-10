import { ChevronDown, FolderGit2, Plus, SendHorizontal, SlidersHorizontal } from 'lucide-react';
import { useMemo } from 'react';

import { taskModelOptions } from '@/features/tasks/taskDefaults';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';

export function HomePage() {
  const locale = useAppStore((state) => state.locale);
  const currentRepository = useAppStore((state) => state.currentRepository);
  const composerDraft = useAppStore((state) => state.composerDraft);
  const thinkingStrength = useAppStore((state) => state.thinkingStrength);
  const setComposerDraft = useAppStore((state) => state.setComposerDraft);
  const setCurrentRoute = useAppStore((state) => state.setCurrentRoute);
  const setNewTaskDialogOpen = useAppStore((state) => state.setNewTaskDialogOpen);

  const selectedModel = useMemo(() => taskModelOptions[0], []);
  const thinkingLabel = t(`settings.thinking.${thinkingStrength}`, locale);

  function handleSubmit() {
    if (!composerDraft.trim()) {
      return;
    }

    if (!currentRepository) {
      setCurrentRoute('repositories');
      return;
    }

    setNewTaskDialogOpen(true);
  }

  return (
    <section className="home-page" aria-label={t('home.promptTitle', locale)}>
      <div className="home-center-stack">
        <h2 className="home-prompt-title">{t('home.promptTitle', locale)}</h2>

        <div className="home-composer-shell">
          <textarea
            value={composerDraft}
            onChange={(event) => setComposerDraft(event.target.value)}
            placeholder={t('home.placeholder', locale)}
            aria-label={t('home.placeholder', locale)}
          />

          <div className="home-composer-toolbar">
            <button type="button" className="home-toolbar-button">
              <Plus className="h-4 w-4" aria-hidden="true" />
            </button>

            <button
              type="button"
              className="home-model-trigger"
              onClick={() => setCurrentRoute('settings')}
            >
              <span>{selectedModel.model}</span>
              <small>{thinkingLabel}</small>
              <ChevronDown className="h-4 w-4" aria-hidden="true" />
            </button>

            <button
              type="button"
              className="home-toolbar-button"
              aria-label={t('settings.categories.thinking', locale)}
              onClick={() => setCurrentRoute('settings')}
            >
              <SlidersHorizontal className="h-4 w-4" aria-hidden="true" />
            </button>

            <button
              type="button"
              className={cn('home-send-button', composerDraft.trim() && 'is-ready')}
              onClick={handleSubmit}
              disabled={!composerDraft.trim()}
              aria-label={t('tasks.new.submit', locale)}
            >
              <SendHorizontal className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>

          <button
            type="button"
            className="home-project-row"
            onClick={() => setCurrentRoute('repositories')}
          >
            <span>{t('home.chooseProject', locale)}</span>
            <strong>
              <FolderGit2 className="h-4 w-4" aria-hidden="true" />
              {currentRepository?.name ?? t('repository.emptyShort', locale)}
            </strong>
          </button>
        </div>
      </div>
    </section>
  );
}
