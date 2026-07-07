import { useEffect, useMemo, useState } from 'react';
import { DiffEditor, loader } from '@monaco-editor/react';
import {
  AlertCircle,
  Check,
  ChevronDown,
  CircleDot,
  ClipboardCheck,
  Code2,
  Command,
  FileCode2,
  FileText,
  FolderOpen,
  GitBranch,
  Github,
  Laptop,
  ListFilter,
  Minus,
  MoreHorizontal,
  PanelRight,
  Plus,
  RefreshCw,
  SendHorizontal,
  SlidersHorizontal,
  TerminalSquare,
} from 'lucide-react';
import * as monaco from 'monaco-editor';

import { generateTaskDelivery, generateTaskDiff } from '@/api/tauriClient';
import { Button } from '@/components/ui/button';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';
import type {
  GeneratedTaskDelivery,
  GeneratedTaskDiff,
  TaskDiffFile,
  TaskValidationRunSummary,
} from '@/types/domain';

loader.config({ monaco });

const largeDiffLineThreshold = 420;
const largeDiffCharThreshold = 32000;

const commandRuns = [
  {
    id: 'branch',
    label: 'git branch --show-current',
    command: 'git branch --show-current',
    output: 'codex/s8-diff-review',
  },
  {
    id: 'status',
    label: 'git status --short',
    command: 'git status --short',
    output:
      ' M apps/desktop/src-tauri/src/git/mod.rs\n M apps/desktop/src/features/tasks/TaskOverviewPage.tsx\n?? apps/desktop/src-tauri/src/commands/diff.rs',
  },
  {
    id: 'check',
    label: 'npm run build:desktop',
    command: 'npm run build:desktop',
    output: 'TypeScript and Vite build verify the S8 Diff review surface.',
  },
];

const demoDiffFiles: TaskDiffFile[] = [
  {
    path: 'apps/desktop/src-tauri/src/commands/diff.rs',
    status: 'added',
    additions: 91,
    deletions: 0,
    patch: `diff --git a/apps/desktop/src-tauri/src/commands/diff.rs b/apps/desktop/src-tauri/src/commands/diff.rs
new file mode 100644
index 0000000..1111111
--- /dev/null
+++ b/apps/desktop/src-tauri/src/commands/diff.rs
@@ -0,0 +1,12 @@
+use tauri::State;
+
+use crate::{
+    core::error::AppResult,
+    git,
+    storage::ManagedStorage,
+};
+
+#[tauri::command]
+pub fn generate_task_diff(storage: State<'_, ManagedStorage>, task_id: String) -> AppResult<()> {
+    Ok(())
+}`,
  },
  {
    path: 'apps/desktop/src/features/tasks/TaskOverviewPage.tsx',
    status: 'modified',
    additions: 156,
    deletions: 24,
    patch: `diff --git a/apps/desktop/src/features/tasks/TaskOverviewPage.tsx b/apps/desktop/src/features/tasks/TaskOverviewPage.tsx
index 2222222..3333333 100644
--- a/apps/desktop/src/features/tasks/TaskOverviewPage.tsx
+++ b/apps/desktop/src/features/tasks/TaskOverviewPage.tsx
@@ -1,7 +1,12 @@
-import { Check, Code2 } from 'lucide-react';
+import { useMemo, useState } from 'react';
+import { DiffEditor } from '@monaco-editor/react';
+import { Check, Code2, RefreshCw } from 'lucide-react';
+
+import { generateTaskDiff } from '@/api/tauriClient';
 import { Button } from '@/components/ui/button';
 import { t } from '@/i18n';

+const largeDiffLineThreshold = 420;
 export function TaskOverviewPage() {
   const locale = useAppStore((state) => state.locale);
+  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
   return <section className="code-change-panel" />;
 }`,
  },
  {
    path: 'apps/desktop/src/styles/global.css',
    status: 'modified',
    additions: 52,
    deletions: 8,
    patch: `diff --git a/apps/desktop/src/styles/global.css b/apps/desktop/src/styles/global.css
index 4444444..5555555 100644
--- a/apps/desktop/src/styles/global.css
+++ b/apps/desktop/src/styles/global.css
@@ -1810,7 +1810,10 @@
 .diff-file-list {
   display: grid;
+  max-height: 280px;
+  overflow: auto;
 }

 .diff-file-row {
+  border: 0;
   border-bottom: 1px solid #eeeeef;
 }`,
  },
];

const demoDiff: GeneratedTaskDiff = {
  taskId: 'task-240707-01',
  baseRef: 'main',
  worktreePath: 'D:\\codemax\\.worktrees\\task-240707-01',
  branchName: 'codex/s8-diff-review',
  artifactId: 'demo-diff-artifact',
  diffPath: 'app-data/tasks/task-240707-01/diff.patch',
  files: demoDiffFiles,
  additions: demoDiffFiles.reduce((total, file) => total + file.additions, 0),
  deletions: demoDiffFiles.reduce((total, file) => total + file.deletions, 0),
  patch: demoDiffFiles.map((file) => file.patch).join('\n'),
};

const demoDelivery: GeneratedTaskDelivery = {
  taskId: demoDiff.taskId,
  artifactId: 'demo-delivery-artifact',
  reportPath: 'app-data/tasks/task-240707-01/report.json',
  deliveryPath: 'app-data/tasks/task-240707-01/artifacts/delivery.md',
  diffPath: demoDiff.diffPath,
  summary:
    '## 问题\nS8-E02 需要把验证结果、交付说明和建议提交信息汇总为可审查交付物。\n\n## 修改点\n新增交付报告生成与展示入口，保留 Diff、测试报告和说明的可追溯路径。\n\n## 文件\n- apps/desktop/src-tauri/src/commands/delivery.rs\n- apps/desktop/src/features/tasks/TaskOverviewPage.tsx\n\n## 验证\n验证通过：共 3 条命令，3 条通过。\n\n## 风险\n演示数据仅用于空状态预览，真实任务需点击生成交付说明读取本地 artifact。',
  commitMessage:
    'feat(desktop): add task delivery report\n\n- Generate S8-E02 validation summary and delivery artifact.\n- Verification: 验证通过：共 3 条命令，3 条通过。\n- Risk: 未发现失败验证命令；合入前仍建议按项目规范复跑关键验证。',
  report: {
    taskId: demoDiff.taskId,
    artifactId: 'demo-delivery-artifact',
    taskTitle: 'S8-E02 测试报告与交付说明',
    generatedAt: '1783372800',
    overallStatus: 'passed',
    summary: '验证通过：共 3 条命令，3 条通过。',
    commandCount: 3,
    passedCount: 3,
    failedCount: 0,
    changedFiles: demoDiff.files.map((file) => file.path),
    diffPath: demoDiff.diffPath,
    deliveryPath: 'app-data/tasks/task-240707-01/artifacts/delivery.md',
    runs: commandRuns.map((run, index) => ({
      runId: `demo-run-${index + 1}`,
      command: run.command,
      cwd: 'D:\\codemax-1',
      status: 'passed',
      exitCode: 0,
      durationMs: 1200 + index * 310,
      createdAt: '1783372800',
    })),
    risk: '未发现失败验证命令；合入前仍建议按项目规范复跑关键验证。',
  },
};

export function TaskOverviewPage() {
  const locale = useAppStore((state) => state.locale);
  const selectedTaskId = useAppStore((state) => state.selectedTaskId);
  const setNewTaskDialogOpen = useAppStore((state) => state.setNewTaskDialogOpen);
  const [generatedDiff, setGeneratedDiff] = useState<GeneratedTaskDiff | null>(null);
  const [generatedDelivery, setGeneratedDelivery] = useState<GeneratedTaskDelivery | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState<string>(demoDiff.files[0]?.path ?? '');
  const [isDiffLoading, setIsDiffLoading] = useState(false);
  const [isDeliveryLoading, setIsDeliveryLoading] = useState(false);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [deliveryError, setDeliveryError] = useState<string | null>(null);
  const [largeDiffExpanded, setLargeDiffExpanded] = useState(false);

  const visibleDiff = generatedDiff ?? demoDiff;
  const visibleDelivery = generatedDelivery ?? demoDelivery;
  const selectedFile =
    visibleDiff.files.find((file) => file.path === selectedFilePath) ?? visibleDiff.files[0] ?? null;
  const selectedFileLarge = selectedFile ? isLargeDiffFile(selectedFile) : false;
  const diffModels = useMemo(
    () => (selectedFile ? buildDiffModels(selectedFile.patch) : { original: '', modified: '' }),
    [selectedFile],
  );

  useEffect(() => {
    if (!visibleDiff.files.some((file) => file.path === selectedFilePath)) {
      setSelectedFilePath(visibleDiff.files[0]?.path ?? '');
    }
  }, [selectedFilePath, visibleDiff.files]);

  async function handleGenerateDiff() {
    if (!selectedTaskId) {
      setDiffError(t('tasks.execution.diffNoTask', locale));
      return;
    }

    setIsDiffLoading(true);
    setDiffError(null);
    try {
      const result = await generateTaskDiff({ taskId: selectedTaskId });
      setGeneratedDiff(result);
      setSelectedFilePath(result.files[0]?.path ?? '');
      setLargeDiffExpanded(false);
    } catch (error) {
      setDiffError(normalizeDiffError(error));
    } finally {
      setIsDiffLoading(false);
    }
  }

  async function handleGenerateDelivery() {
    if (!selectedTaskId) {
      setDeliveryError(t('tasks.execution.deliveryNoTask', locale));
      return;
    }

    setIsDeliveryLoading(true);
    setDeliveryError(null);
    try {
      const result = await generateTaskDelivery({ taskId: selectedTaskId });
      setGeneratedDelivery(result);
    } catch (error) {
      setDeliveryError(normalizeDiffError(error));
    } finally {
      setIsDeliveryLoading(false);
    }
  }

  return (
    <div className="codex-execution-layout">
      <header className="execution-topbar">
        <div className="execution-topbar-title">
          <TerminalSquare className="h-4 w-4" aria-hidden="true" />
          <h3>{t('tasks.execution.title', locale)}</h3>
          <button type="button" aria-label={t('tasks.execution.more', locale)}>
            <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
        <div className="execution-topbar-actions">
          <button type="button">
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.openLocation', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.filters', locale)}>
            <ListFilter className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.layoutCompact', locale)}>
            <SlidersHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
          <button type="button" aria-label={t('tasks.execution.environmentToggle', locale)}>
            <PanelRight className="h-4 w-4" aria-hidden="true" />
          </button>
        </div>
      </header>

      <section className="codex-run-transcript" aria-label={t('tasks.chat.thread', locale)}>
        <header className="execution-thread-header">
          <div>
            <h3>{t('tasks.execution.title', locale)}</h3>
            <p>{t('tasks.execution.subtitle', locale)}</p>
          </div>
          <button type="button" aria-label={t('tasks.execution.more', locale)}>
            <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
        </header>

        <article className="execution-message">
          <p>{t('tasks.execution.lead', locale)}</p>
        </article>

        <section className="execution-section">
          <button type="button" className="execution-collapse">
            <TerminalSquare className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.commands', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="command-run-list">
            {commandRuns.map((run) => (
              <CommandRunCard key={run.id} label={run.label} command={run.command} output={run.output} />
            ))}
          </div>
        </section>

        <section className="code-change-panel">
          <div className="code-change-heading">
            <div>
              <span>{t('tasks.execution.codeChanges', locale)}</span>
              <strong>{formatDiffStat(visibleDiff.additions, visibleDiff.deletions)}</strong>
            </div>
            <Button type="button" size="sm" variant="secondary" onClick={handleGenerateDiff}>
              <RefreshCw className={cn('h-3.5 w-3.5', isDiffLoading && 'diff-spin')} aria-hidden="true" />
              {isDiffLoading ? t('tasks.execution.generatingDiff', locale) : t('tasks.execution.reviewDiff', locale)}
            </Button>
          </div>

          <div className="diff-meta-strip">
            <span>
              {t('tasks.execution.diffBase', locale)} <strong>{visibleDiff.baseRef}</strong>
            </span>
            <span>
              {t('tasks.execution.diffFileCount', locale)} <strong>{visibleDiff.files.length}</strong>
            </span>
            <span>
              {t('tasks.execution.diffArtifactPath', locale)} <strong>{visibleDiff.diffPath}</strong>
            </span>
          </div>

          {diffError ? (
            <div className="diff-error-banner" role="alert">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{diffError}</span>
            </div>
          ) : null}

          <div className="diff-review-layout">
            <div className="diff-file-list" aria-label={t('tasks.execution.diffFileTree', locale)}>
              {visibleDiff.files.length > 0 ? (
                visibleDiff.files.map((file) => (
                  <button
                    type="button"
                    className={cn('diff-file-row', selectedFile?.path === file.path && 'active')}
                    key={file.path}
                    onClick={() => {
                      setSelectedFilePath(file.path);
                      setLargeDiffExpanded(false);
                    }}
                  >
                    <FileCode2 className="h-4 w-4" aria-hidden="true" />
                    <span>
                      <strong>{file.path}</strong>
                      <small>{t(`tasks.execution.diffStatus.${file.status}`, locale)}</small>
                    </span>
                    <em>
                      <Plus className="h-3.5 w-3.5" aria-hidden="true" />
                      {file.additions}
                      <Minus className="h-3.5 w-3.5" aria-hidden="true" />
                      {file.deletions}
                    </em>
                  </button>
                ))
              ) : (
                <div className="diff-empty-state">{t('tasks.execution.diffEmpty', locale)}</div>
              )}
            </div>

            <div className="diff-viewer-shell">
              {selectedFile ? (
                <>
                  <header className="diff-viewer-heading">
                    <span>{selectedFile.path}</span>
                    <em>{formatDiffStat(selectedFile.additions, selectedFile.deletions)}</em>
                  </header>
                  {selectedFileLarge && !largeDiffExpanded ? (
                    <div className="diff-large-collapsed">
                      <strong>{t('tasks.execution.diffLargeCollapsed', locale)}</strong>
                      <span>{formatPatchSize(selectedFile.patch)}</span>
                      <Button type="button" size="sm" variant="secondary" onClick={() => setLargeDiffExpanded(true)}>
                        {t('tasks.execution.expandLargeDiff', locale)}
                      </Button>
                    </div>
                  ) : isBinaryPatch(selectedFile.patch) ? (
                    <pre
                      className="diff-preview monaco-diff-fallback execution-code-diff-preview"
                      aria-label={t('tasks.execution.diffPreview', locale)}
                    >
                      <code>{selectedFile.patch}</code>
                    </pre>
                  ) : (
                    <div className="monaco-diff-view" aria-label={t('tasks.execution.monacoDiff', locale)}>
                      <DiffEditor
                        height="360px"
                        language={languageForPath(selectedFile.path)}
                        original={diffModels.original}
                        modified={diffModels.modified}
                        theme="vs"
                        options={{
                          automaticLayout: true,
                          folding: false,
                          fontSize: 12,
                          glyphMargin: false,
                          minimap: { enabled: false },
                          readOnly: true,
                          renderSideBySide: true,
                          scrollBeyondLastLine: false,
                          wordWrap: 'off',
                        }}
                      />
                    </div>
                  )}
                  {selectedFileLarge && largeDiffExpanded ? (
                    <div className="diff-viewer-actions">
                      <Button type="button" size="sm" variant="ghost" onClick={() => setLargeDiffExpanded(false)}>
                        {t('tasks.execution.collapseLargeDiff', locale)}
                      </Button>
                    </div>
                  ) : null}
                </>
              ) : (
                <div className="diff-empty-state">{t('tasks.execution.diffEmpty', locale)}</div>
              )}
            </div>
          </div>
        </section>

        <section className="delivery-panel">
          <div className="delivery-heading">
            <div>
              <span>{t('tasks.execution.deliveryTitle', locale)}</span>
              <strong className={cn('delivery-status-pill', `is-${visibleDelivery.report.overallStatus}`)}>
                {t(`tasks.execution.deliveryStatus.${visibleDelivery.report.overallStatus}`, locale)}
              </strong>
            </div>
            <Button type="button" size="sm" variant="secondary" onClick={handleGenerateDelivery}>
              <ClipboardCheck className={cn('h-3.5 w-3.5', isDeliveryLoading && 'diff-spin')} aria-hidden="true" />
              {isDeliveryLoading ? t('tasks.execution.generatingDelivery', locale) : t('tasks.execution.generateDelivery', locale)}
            </Button>
          </div>

          <div className="delivery-meta-strip">
            <span>
              {t('tasks.execution.reportFile', locale)} <strong>{visibleDelivery.reportPath}</strong>
            </span>
            <span>
              {t('tasks.execution.deliveryFile', locale)} <strong>{visibleDelivery.deliveryPath}</strong>
            </span>
            <span>
              {t('tasks.execution.deliveryArtifact', locale)} <strong>{visibleDelivery.artifactId}</strong>
            </span>
          </div>

          {deliveryError ? (
            <div className="diff-error-banner" role="alert">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{deliveryError}</span>
            </div>
          ) : null}

          <div className="delivery-grid">
            <article className="test-report-card">
              <header>
                <FileText className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.execution.testReport', locale)}</span>
              </header>
              <p>{visibleDelivery.report.summary}</p>
              <div className="test-report-stats">
                <MetricPill label={t('tasks.execution.reportCommands', locale)} value={visibleDelivery.report.commandCount.toString()} />
                <MetricPill label={t('tasks.execution.reportPassed', locale)} value={visibleDelivery.report.passedCount.toString()} />
                <MetricPill label={t('tasks.execution.reportFailed', locale)} value={visibleDelivery.report.failedCount.toString()} />
              </div>
              <div className="validation-run-table" aria-label={t('tasks.execution.validationRuns', locale)}>
                {visibleDelivery.report.runs.length > 0 ? (
                  visibleDelivery.report.runs.map((run) => <ValidationRunRow key={run.runId} run={run} />)
                ) : (
                  <div className="delivery-empty-state">{t('tasks.execution.noValidationRuns', locale)}</div>
                )}
              </div>
            </article>

            <article className="delivery-summary-card">
              <header>
                <ClipboardCheck className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.execution.agentDelivery', locale)}</span>
              </header>
              <pre className="delivery-summary-block">
                <code>{visibleDelivery.summary}</code>
              </pre>
              <div className="commit-message-box">
                <span>{t('tasks.execution.commitMessage', locale)}</span>
                <code>{visibleDelivery.commitMessage}</code>
              </div>
            </article>
          </div>
        </section>
      </section>

      <aside className="environment-panel" aria-label={t('tasks.environment.title', locale)}>
        <div className="environment-card">
          <header>
            <span>{t('tasks.environment.title', locale)}</span>
            <button type="button" aria-label={t('tasks.environment.add', locale)}>
              <Plus className="h-4 w-4" aria-hidden="true" />
            </button>
          </header>
          <div className="environment-list">
            <EnvironmentRow icon={Code2} labelKey="tasks.environment.changes" value={formatDiffStat(visibleDiff.additions, visibleDiff.deletions)} accent />
            <EnvironmentRow icon={Laptop} labelKey="tasks.environment.local" value={t('tasks.environment.localMode', locale)} />
            <EnvironmentRow icon={GitBranch} labelKey="tasks.environment.branch" value={visibleDiff.branchName} />
            <EnvironmentRow icon={CircleDot} labelKey="tasks.environment.commit" value={t('tasks.environment.commitValue', locale)} />
            <EnvironmentRow icon={Github} labelKey="tasks.environment.github" value={t('tasks.environment.githubValue', locale)} muted />
          </div>
          <div className="environment-source">
            <strong>{t('tasks.environment.sources', locale)}</strong>
            <span>{generatedDiff ? visibleDiff.diffPath : t('tasks.execution.diffDemoSource', locale)}</span>
          </div>
        </div>
      </aside>

      <section className="execution-followup-composer" aria-label={t('tasks.execution.followup', locale)}>
        <button type="button" onClick={() => setNewTaskDialogOpen(true)}>
          {t('tasks.execution.followupPlaceholder', locale)}
        </button>
        <div>
          <span>
            <Plus className="h-4 w-4" aria-hidden="true" />
            {t('tasks.composer.attach', locale)}
          </span>
          <span>{t('tasks.new.permissions.network', locale)}</span>
          <span>5.5 {t('tasks.new.strength.deep', locale)}</span>
          <Button type="button" size="icon" onClick={() => setNewTaskDialogOpen(true)}>
            <SendHorizontal className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      </section>
    </div>
  );
}

function MetricPill({ label, value }: { label: string; value: string }) {
  return (
    <span>
      <small>{label}</small>
      <strong>{value}</strong>
    </span>
  );
}

function ValidationRunRow({ run }: { run: TaskValidationRunSummary }) {
  const locale = useAppStore((state) => state.locale);
  const statusKey = `tasks.execution.commandStatus.${run.status}`;

  return (
    <div className="validation-run-row">
      <Command className="h-4 w-4" aria-hidden="true" />
      <span>
        <strong>{run.command}</strong>
        <small>{run.cwd}</small>
      </span>
      <em className={run.status}>{t(statusKey, locale)}</em>
      <code>{run.exitCode ?? '-'}</code>
      <time>{formatDuration(run.durationMs)}</time>
    </div>
  );
}

function CommandRunCard({ label, command, output }: { label: string; command: string; output: string }) {
  const locale = useAppStore((state) => state.locale);

  return (
    <article className="command-run-card">
      <button type="button" className="command-run-summary">
        <Command className="h-4 w-4" aria-hidden="true" />
        {t('tasks.execution.ran', locale)} {label}
        <ChevronDown className="h-4 w-4" aria-hidden="true" />
      </button>
      <pre className="command-output-block">
        <code>{`$ ${command}\n\n${output}`}</code>
      </pre>
      <div className="command-success">
        <Check className="h-4 w-4" aria-hidden="true" />
        {t('tasks.execution.success', locale)}
      </div>
    </article>
  );
}

function EnvironmentRow({
  icon: Icon,
  labelKey,
  value,
  accent,
  muted,
}: {
  icon: typeof Code2;
  labelKey: string;
  value: string;
  accent?: boolean;
  muted?: boolean;
}) {
  const locale = useAppStore((state) => state.locale);

  return (
    <div className="environment-row">
      <Icon className="h-4 w-4" aria-hidden="true" />
      <span>{t(labelKey, locale)}</span>
      <em className={accent ? 'accent' : muted ? 'muted' : ''}>{value}</em>
    </div>
  );
}

function formatDuration(durationMs?: number | null) {
  if (durationMs === null || durationMs === undefined) {
    return '-';
  }

  if (durationMs < 1000) {
    return `${durationMs}ms`;
  }

  return `${(durationMs / 1000).toFixed(1)}s`;
}

function normalizeDiffError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'title' in error) {
    return String((error as { title: unknown }).title);
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

function formatDiffStat(additions: number, deletions: number) {
  return `+${additions.toLocaleString()} -${deletions.toLocaleString()}`;
}

function formatPatchSize(patch: string) {
  const lines = patch.split(/\r?\n/).length;
  const kb = Math.max(1, Math.round(patch.length / 1024));
  return `${lines.toLocaleString()} lines / ${kb.toLocaleString()} KB`;
}

function isLargeDiffFile(file: TaskDiffFile) {
  return file.patch.length > largeDiffCharThreshold || file.patch.split(/\r?\n/).length > largeDiffLineThreshold;
}

function isBinaryPatch(patch: string) {
  return patch.includes('Binary files ') || patch.includes('GIT binary patch');
}

function buildDiffModels(patch: string) {
  const original: string[] = [];
  const modified: string[] = [];

  for (const line of patch.split(/\r?\n/)) {
    if (isPatchMetadataLine(line)) {
      continue;
    }

    if (line.startsWith('@@') || line.startsWith('\\ No newline')) {
      continue;
    }

    if (line.startsWith('+')) {
      modified.push(line.slice(1));
      continue;
    }

    if (line.startsWith('-')) {
      original.push(line.slice(1));
      continue;
    }

    if (line.startsWith(' ')) {
      original.push(line.slice(1));
      modified.push(line.slice(1));
    }
  }

  if (original.length === 0 && modified.length === 0) {
    return { original: '', modified: patch };
  }

  return {
    original: original.join('\n'),
    modified: modified.join('\n'),
  };
}

function isPatchMetadataLine(line: string) {
  return (
    line.startsWith('diff --git ') ||
    line.startsWith('index ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('old mode ') ||
    line.startsWith('new mode ') ||
    line.startsWith('similarity index ') ||
    line.startsWith('rename from ') ||
    line.startsWith('rename to ')
  );
}

function languageForPath(path: string) {
  const extension = path.split('.').pop()?.toLowerCase();
  return (
    {
      css: 'css',
      html: 'html',
      js: 'javascript',
      json: 'json',
      jsx: 'javascript',
      md: 'markdown',
      py: 'python',
      rs: 'rust',
      sql: 'sql',
      ts: 'typescript',
      tsx: 'typescript',
      yml: 'yaml',
      yaml: 'yaml',
    }[extension ?? ''] ?? 'plaintext'
  );
}
