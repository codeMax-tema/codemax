import { useEffect, useMemo, useRef, useState } from 'react';
import { DiffEditor, loader } from '@monaco-editor/react';
import {
  AlertCircle,
  Check,
  ChevronDown,
  CircleDot,
  ClipboardCheck,
  Code2,
  Command,
  Camera,
  FileCode2,
  FileText,
  FolderOpen,
  Gauge,
  GitBranch,
  GitMerge,
  Github,
  Laptop,
  Layers3,
  ListFilter,
  Minus,
  MoreHorizontal,
  PackageCheck,
  PanelRight,
  Plus,
  Radar,
  RefreshCw,
  SendHorizontal,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
} from 'lucide-react';
import * as monaco from 'monaco-editor';

import {
  getTaskDetail,
  generateTaskDelivery,
  generateTaskDiff,
  generateTaskProofPack,
  mergeTask,
  prepareTaskMerge,
  readTaskCommandLog,
  runAgentValidationCycle,
} from '@/api/tauriClient';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { t, type Locale } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';
import type {
  GeneratedTaskDelivery,
  GeneratedTaskDiff,
  GeneratedTaskProofPack,
  AgentValidationCycleResult,
  CommandLogPage,
  CommandOutputStream,
  DeliveryReviewState,
  DeliveryScoreState,
  PreparedTaskMerge,
  RiskLevel,
  TaskCommandRun,
  TaskDetail,
  TaskDiffFile,
  TaskMergeCommandResult,
  TaskProofPackScore,
  TaskValidationRunSummary,
} from '@/types/domain';

loader.config({ monaco });

const largeDiffLineThreshold = 420;
const largeDiffCharThreshold = 32000;
const commandLogPageBytes = 32 * 1024;
type CommandRunLike = TaskValidationRunSummary | TaskCommandRun;
type VisibleDeliveryScore = TaskProofPackScore | DeliveryScoreState;
type VisibleRiskItem = {
  id: string;
  titleKey: string;
  summaryKey?: string;
  level: RiskLevel;
  label?: string;
};

export function TaskOverviewPage() {
  const locale = useAppStore((state) => state.locale);
  const selectedTaskId = useAppStore((state) => state.selectedTaskId);
  const setNewTaskDialogOpen = useAppStore((state) => state.setNewTaskDialogOpen);
  const selectedTaskIdRef = useRef(selectedTaskId);
  const [taskDetail, setTaskDetail] = useState<TaskDetail | null>(null);
  const [taskError, setTaskError] = useState<string | null>(null);
  const [generatedDiff, setGeneratedDiff] = useState<GeneratedTaskDiff | null>(null);
  const [generatedDelivery, setGeneratedDelivery] = useState<GeneratedTaskDelivery | null>(null);
  const [generatedProofPack, setGeneratedProofPack] = useState<GeneratedTaskProofPack | null>(null);
  const [preparedMerge, setPreparedMerge] = useState<PreparedTaskMerge | null>(null);
  const [mergeResult, setMergeResult] = useState<TaskMergeCommandResult | null>(null);
  const [agentCycleResult, setAgentCycleResult] = useState<AgentValidationCycleResult | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState<string>('');
  const [isDiffLoading, setIsDiffLoading] = useState(false);
  const [isDeliveryLoading, setIsDeliveryLoading] = useState(false);
  const [isProofPackLoading, setIsProofPackLoading] = useState(false);
  const [isMergePreparing, setIsMergePreparing] = useState(false);
  const [isMergeLoading, setIsMergeLoading] = useState(false);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [deliveryError, setDeliveryError] = useState<string | null>(null);
  const [proofPackError, setProofPackError] = useState<string | null>(null);
  const [mergeError, setMergeError] = useState<string | null>(null);
  const [agentCycleError, setAgentCycleError] = useState<string | null>(null);
  const [largeDiffExpanded, setLargeDiffExpanded] = useState(false);
  const [mergeDialogOpen, setMergeDialogOpen] = useState(false);
  const [mergeCommitMessage, setMergeCommitMessage] = useState('');
  const [isAgentCycleRunning, setIsAgentCycleRunning] = useState(false);

  const visibleDiff = generatedDiff;
  const visibleDelivery = generatedDelivery;
  const visibleProofPack = generatedProofPack;
  const visibleReview = taskDetail?.deliveryReviewState ?? visibleDelivery?.deliveryReviewState ?? null;
  const visibleProofPackPath = visibleReview?.proofPackPath ?? visibleProofPack?.proofPackPath ?? null;
  const visibleProofPackId = visibleReview?.proofPackId ?? visibleProofPack?.artifactId ?? null;
  const visibleProofPackStatus = visibleReview?.proofPackStatus ?? (visibleProofPack ? 'generated' : 'missing');
  const visibleDeliveryScore: VisibleDeliveryScore | null = visibleReview?.deliveryScore ?? visibleProofPack?.deliveryScore ?? null;
  const visibleQualityGates = visibleReview?.qualityGateResult.gates ?? visibleProofPack?.qualityGates ?? [];
  const visibleRiskItems: VisibleRiskItem[] = visibleReview
    ? visibleReview.riskRecords.map((risk, index) => ({
      id: `review-risk-${index + 1}`,
      titleKey: risk.kind,
      summaryKey: risk.reason,
      level: risk.level,
      label: risk.subject || risk.kind,
    }))
    : visibleProofPack?.risks ?? [];
  const visibleRuleHits = visibleReview?.ruleHits ?? [];
  const visibleHookRuns = visibleReview?.hookRuns ?? [];
  const visibleModelArenaDecision = visibleReview?.modelArenaDecision ?? null;
  const visibleProofPackFiles = visibleReview?.proofPackFiles ?? visibleProofPack?.proofPackFiles ?? [];
  const visiblePrivacySummary = visibleReview?.privacyLedgerSummary ?? null;
  const visibleRunContractSummary = visibleReview?.runContractSummary ?? null;
  const visibleTokenBudgetSummary = visibleReview?.tokenBudgetSummary ?? null;
  const visibleMerge = preparedMerge;
  const taskRecord = taskDetail?.task ?? null;
  const visibleCommandRuns = taskDetail?.commandRuns ?? visibleDelivery?.report.runs ?? [];
  const visibleTodos = taskDetail?.todos ?? [];
  const visibleTimeline = taskDetail?.timeline ?? [];
  const visibleValidationRounds = taskDetail?.validationRounds ?? [];
  const deliveryStatus = visibleDelivery?.report.overallStatus ?? 'notRun';
  const taskTitle = taskRecord?.title ?? t('tasks.execution.noTaskTitle', locale);
  const taskSubtitle = taskRecord
    ? `${formatTaskStatus(taskRecord.status, locale)} · ${taskRecord.repositoryPath}`
    : t('tasks.execution.noTaskBody', locale);
  const selectedFile =
    visibleDiff?.files.find((file) => file.path === selectedFilePath) ?? visibleDiff?.files[0] ?? null;
  const selectedFileLarge = selectedFile ? isLargeDiffFile(selectedFile) : false;
  const diffModels = useMemo(
    () => (selectedFile ? buildDiffModels(selectedFile.patch) : { original: '', modified: '' }),
    [selectedFile],
  );

  useEffect(() => {
    if (!visibleDiff?.files.some((file) => file.path === selectedFilePath)) {
      setSelectedFilePath(visibleDiff?.files[0]?.path ?? '');
    }
  }, [selectedFilePath, visibleDiff?.files]);

  useEffect(() => {
    selectedTaskIdRef.current = selectedTaskId;
    setTaskDetail(null);
    setTaskError(null);
    setGeneratedDiff(null);
    setGeneratedDelivery(null);
    setGeneratedProofPack(null);
    setPreparedMerge(null);
    setMergeResult(null);
    setAgentCycleResult(null);
    setDiffError(null);
    setDeliveryError(null);
    setProofPackError(null);
    setMergeError(null);
    setAgentCycleError(null);
    setMergeDialogOpen(false);
    setLargeDiffExpanded(false);
    setMergeCommitMessage('');
  }, [selectedTaskId]);

  useEffect(() => {
    if (!selectedTaskId) {
      return;
    }

    const taskId = selectedTaskId;
    let cancelled = false;

    async function loadTaskRecord() {
      try {
        const detail = await getTaskDetail(taskId);
        if (!cancelled && selectedTaskIdRef.current === taskId) {
          setTaskDetail(detail);
          setTaskError(null);
        }
      } catch (error) {
        if (!cancelled && selectedTaskIdRef.current === taskId) {
          setTaskDetail(null);
          setTaskError(normalizeDiffError(error));
        }
      }
    }

    void loadTaskRecord();

    return () => {
      cancelled = true;
    };
  }, [selectedTaskId]);

  useEffect(() => {
    if (!mergeDialogOpen && visibleMerge) {
      setMergeCommitMessage(visibleMerge.commitMessage);
    }
  }, [mergeDialogOpen, visibleMerge]);

  async function loadTaskDiff({ reportMergeError = false } = {}) {
    const taskId = selectedTaskId;
    if (!taskId) {
      const message = t('tasks.execution.diffNoTask', locale);
      setDiffError(message);
      if (reportMergeError) {
        setMergeError(message);
      }
      return null;
    }

    setIsDiffLoading(true);
    setDiffError(null);
    if (reportMergeError) {
      setMergeError(null);
    }
    try {
      const result = await generateTaskDiff({ taskId });
      if (selectedTaskIdRef.current !== taskId) {
        return null;
      }
      setGeneratedDiff(result);
      setPreparedMerge(null);
      setMergeResult(null);
      setSelectedFilePath(result.files[0]?.path ?? '');
      setLargeDiffExpanded(false);
      return result;
    } catch (error) {
      if (selectedTaskIdRef.current === taskId) {
        const message = normalizeDiffError(error);
        setDiffError(message);
        if (reportMergeError) {
          setMergeError(message);
        }
      }
      return null;
    } finally {
      if (selectedTaskIdRef.current === taskId) {
        setIsDiffLoading(false);
      }
    }
  }

  async function handleGenerateDiff() {
    await loadTaskDiff();
  }

  async function handleGenerateDelivery() {
    const taskId = selectedTaskId;
    if (!taskId) {
      setDeliveryError(t('tasks.execution.deliveryNoTask', locale));
      return;
    }

    setIsDeliveryLoading(true);
    setDeliveryError(null);
    try {
      const result = await generateTaskDelivery({ taskId });
      if (selectedTaskIdRef.current !== taskId) {
        return;
      }
      setGeneratedDelivery(result);
      setPreparedMerge(null);
      setMergeResult(null);
      await refreshSelectedTaskDetail(taskId);
    } catch (error) {
      if (selectedTaskIdRef.current === taskId) {
        setDeliveryError(normalizeDiffError(error));
      }
    } finally {
      if (selectedTaskIdRef.current === taskId) {
        setIsDeliveryLoading(false);
      }
    }
  }

  async function handleGenerateProofPack() {
    const taskId = selectedTaskId;
    if (!taskId) {
      setProofPackError(t('tasks.s12.noTask', locale));
      return;
    }

    setIsProofPackLoading(true);
    setProofPackError(null);
    try {
      const result = await generateTaskProofPack({ taskId });
      if (selectedTaskIdRef.current !== taskId) {
        return;
      }
      setGeneratedProofPack(result);
      await refreshSelectedTaskDetail(taskId);
    } catch (error) {
      if (selectedTaskIdRef.current === taskId) {
        setProofPackError(normalizeDiffError(error));
      }
    } finally {
      if (selectedTaskIdRef.current === taskId) {
        setIsProofPackLoading(false);
      }
    }
  }

  async function refreshSelectedTaskDetail(taskId: string) {
    const detail = await getTaskDetail(taskId);
    if (selectedTaskIdRef.current === taskId) {
      setTaskDetail(detail);
      setTaskError(null);
    }
  }

  async function handleRunAgentValidationCycle() {
    const taskId = selectedTaskId;
    if (!taskId) {
      setAgentCycleError(t('tasks.execution.agentNoTask', locale));
      return;
    }

    setIsAgentCycleRunning(true);
    setAgentCycleError(null);
    try {
      const result = await runAgentValidationCycle({
        taskId,
        reason: 'User started or continued the Agent validation cycle from task detail.',
      });
      if (selectedTaskIdRef.current !== taskId) {
        return;
      }
      setAgentCycleResult(result);
      await refreshSelectedTaskDetail(taskId);
    } catch (error) {
      if (selectedTaskIdRef.current === taskId) {
        setAgentCycleError(normalizeDiffError(error));
      }
    } finally {
      if (selectedTaskIdRef.current === taskId) {
        setIsAgentCycleRunning(false);
      }
    }
  }

  async function loadMergePreparation() {
    const taskId = selectedTaskId;
    if (!taskId) {
      setMergeError(t('tasks.execution.mergeNoTask', locale));
      return null;
    }

    setIsMergePreparing(true);
    setMergeError(null);
    try {
      const result = await prepareTaskMerge({ taskId });
      if (selectedTaskIdRef.current !== taskId) {
        return null;
      }
      setPreparedMerge(result);
      setMergeCommitMessage(result.commitMessage);
      return result;
    } catch (error) {
      if (selectedTaskIdRef.current === taskId) {
        setMergeError(normalizeDiffError(error));
      }
      return null;
    } finally {
      if (selectedTaskIdRef.current === taskId) {
        setIsMergePreparing(false);
      }
    }
  }

  async function handlePrepareMerge() {
    await loadMergePreparation();
  }

  async function handleOpenMergeDialog() {
    const refreshedDiff = await loadTaskDiff({ reportMergeError: true });
    if (!refreshedDiff) {
      return;
    }

    const preparation = await loadMergePreparation();
    if (!preparation) {
      return;
    }

    if (!preparation.canMerge) {
      setMergeError(t('tasks.execution.mergeBlocked', locale));
      return;
    }

    setMergeDialogOpen(true);
  }

  async function handleConfirmMerge() {
    const taskId = selectedTaskId;
    if (!taskId) {
      setMergeError(t('tasks.execution.mergeNoTask', locale));
      return;
    }
    if (!visibleMerge) {
      setMergeDialogOpen(false);
      setMergeError(t('tasks.execution.mergePrecheckRequired', locale));
      return;
    }
    if (visibleMerge.taskId !== taskId) {
      setMergeDialogOpen(false);
      setMergeError(t('tasks.execution.mergeStale', locale));
      return;
    }

    setIsMergeLoading(true);
    setMergeError(null);
    try {
      const result = await mergeTask({
        taskId,
        targetBranch: visibleMerge.targetBranch,
        commitMessage: mergeCommitMessage,
        confirmed: true,
      });
      if (selectedTaskIdRef.current !== taskId) {
        return;
      }
      setMergeResult(result);
      setMergeDialogOpen(false);
      setPreparedMerge((current) =>
        current
          ? {
              ...current,
              canMerge: false,
              blockers:
                result.status === 'merged'
                  ? [t('tasks.execution.mergeAlreadyMerged', locale)]
                  : [t('tasks.execution.mergeConflictSummary', locale)],
            }
          : current,
      );
      if (result.status === 'conflicted') {
        setMergeError(t('tasks.execution.mergeConflictSummary', locale));
      }
    } catch (error) {
      setMergeError(normalizeDiffError(error));
    } finally {
      setIsMergeLoading(false);
    }
  }

  return (
    <div className="codex-execution-layout">
      <header className="execution-topbar">
        <div className="execution-topbar-title">
          <TerminalSquare className="h-4 w-4" aria-hidden="true" />
          <h3>{taskTitle}</h3>
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
            <h3>{taskTitle}</h3>
            <p>{taskSubtitle}</p>
          </div>
          <button type="button" aria-label={t('tasks.execution.more', locale)}>
            <MoreHorizontal className="h-4 w-4" aria-hidden="true" />
          </button>
        </header>

        <article className="execution-message">
          <p>{taskRecord?.description ?? t('tasks.execution.noTaskBody', locale)}</p>
          {taskError ? (
            <div className="diff-error-banner" role="alert">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{taskError}</span>
            </div>
          ) : null}
          {taskRecord ? (
            <div className="diff-meta-strip">
              <span>
                {t('tasks.execution.taskId', locale)} <strong>{taskRecord.id}</strong>
              </span>
              <span>
                {t('tasks.execution.taskStatus', locale)} <strong>{formatTaskStatus(taskRecord.status, locale)}</strong>
              </span>
              <span>
                {t('tasks.execution.repositoryPath', locale)} <strong>{taskRecord.repositoryPath}</strong>
              </span>
              <span>
                {t('tasks.execution.worktreePath', locale)} <strong>{taskRecord.worktreePath ?? '-'}</strong>
              </span>
              <span>
                {t('tasks.execution.taskBranch', locale)} <strong>{taskRecord.branchName ?? '-'}</strong>
              </span>
              <span>
                {t('tasks.execution.repositoryId', locale)} <strong>{taskRecord.repositoryId}</strong>
              </span>
              <span>
                {t('tasks.execution.targetBranch', locale)} <strong>{taskRecord.targetBranch || '-'}</strong>
              </span>
              <span>
                {t('tasks.execution.agentStage', locale)} <strong>{formatTaskStatus(taskRecord.agentStage, locale)}</strong>
              </span>
              <span>
                {t('tasks.execution.latestValidation', locale)} <strong>{taskRecord.latestValidationStatus}</strong>
              </span>
              <span>
                {t('tasks.execution.latestDiff', locale)} <strong>{taskRecord.latestDiffSummary || '-'}</strong>
              </span>
            </div>
          ) : null}
          {taskRecord ? (
            <section className="agent-cycle-panel" aria-label={t('tasks.execution.agentCycle', locale)}>
              <div className="agent-cycle-heading">
                <span>
                  <CircleDot className="h-4 w-4" aria-hidden="true" />
                  {t('tasks.execution.agentCycle', locale)}
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  onClick={handleRunAgentValidationCycle}
                  disabled={isAgentCycleRunning}
                >
                  <RefreshCw className={cn('h-3.5 w-3.5', isAgentCycleRunning && 'diff-spin')} aria-hidden="true" />
                  {isAgentCycleRunning
                    ? t('tasks.execution.agentCycleRunning', locale)
                    : t('tasks.execution.agentCycleRun', locale)}
                </Button>
              </div>
              <div className="agent-cycle-grid">
                <MetricPill label={t('tasks.execution.agentStage', locale)} value={formatTaskStatus(taskRecord.agentStage, locale)} />
                <MetricPill
                  label={t('tasks.execution.agentPhase', locale)}
                  value={agentCycleResult?.phase ?? taskDetail?.agentSession?.status ?? '-'}
                />
                <MetricPill
                  label={t('tasks.execution.agentIterations', locale)}
                  value={(agentCycleResult?.iterations ?? 0).toString()}
                />
                <MetricPill
                  label={t('tasks.execution.agentRepairRound', locale)}
                  value={formatRepairRound(agentCycleResult?.state.repairRound, agentCycleResult?.state.maxRepairRounds)}
                />
              </div>
              <div className="agent-validation-request">
                <span>{t('tasks.execution.agentValidationRequest', locale)}</span>
                <code>{formatValidationRequest(agentCycleResult?.state.validationRequest)}</code>
              </div>
              {agentCycleError ? (
                <div className="diff-error-banner" role="alert">
                  <AlertCircle className="h-4 w-4" aria-hidden="true" />
                  <span>{agentCycleError}</span>
                </div>
              ) : null}
            </section>
          ) : null}
        </article>

        <section className="execution-section">
          <button type="button" className="execution-collapse">
            <ClipboardCheck className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.todos', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="task-truth-list">
            {visibleTodos.length > 0 ? (
              visibleTodos.map((todo) => (
                <article key={todo.id} className="task-truth-row">
                  <span className={cn('task-truth-dot', `status-${todo.status}`)} />
                  <div>
                    <strong>{todo.title}</strong>
                    <small>{todo.description}</small>
                  </div>
                  <em>{todo.status}</em>
                </article>
              ))
            ) : (
              <div className="delivery-empty-state">{t('tasks.execution.noTodos', locale)}</div>
            )}
          </div>
        </section>

        <section className="execution-section">
          <button type="button" className="execution-collapse">
            <CircleDot className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.timeline', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="task-timeline-list">
            {visibleTimeline.length > 0 ? (
              visibleTimeline.map((event) => (
                <article key={event.eventId} className="task-timeline-row">
                  <span>{event.eventType}</span>
                  <div>
                    <strong>{event.message}</strong>
                    <small>{formatTaskStatus(event.stage, locale)} · {formatTaskTime(event.createdAt)}</small>
                  </div>
                </article>
              ))
            ) : (
              <div className="delivery-empty-state">{t('tasks.execution.noTimeline', locale)}</div>
            )}
          </div>
        </section>

        <section className="execution-section">
          <button type="button" className="execution-collapse">
            <TerminalSquare className="h-4 w-4" aria-hidden="true" />
            {t('tasks.execution.commands', locale)}
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </button>
          <div className="command-run-list">
            {visibleCommandRuns.length > 0 ? (
              visibleCommandRuns.map((run) => (
                <CommandRunCard key={run.runId} run={run} taskId={taskRecord?.id ?? selectedTaskId ?? ''} />
              ))
            ) : (
              <div className="delivery-empty-state">{t('tasks.execution.noValidationRuns', locale)}</div>
            )}
          </div>
        </section>

        <section className="code-change-panel">
          <div className="code-change-heading">
            <div>
              <span>{t('tasks.execution.codeChanges', locale)}</span>
              <strong>{formatDiffStat(visibleDiff?.additions ?? 0, visibleDiff?.deletions ?? 0)}</strong>
            </div>
            <Button type="button" size="sm" variant="secondary" onClick={handleGenerateDiff}>
              <RefreshCw className={cn('h-3.5 w-3.5', isDiffLoading && 'diff-spin')} aria-hidden="true" />
              {isDiffLoading ? t('tasks.execution.generatingDiff', locale) : t('tasks.execution.reviewDiff', locale)}
            </Button>
          </div>

          <div className="diff-meta-strip">
            <span>
              {t('tasks.execution.diffBase', locale)} <strong>{visibleDiff?.baseRef ?? '-'}</strong>
            </span>
            <span>
              {t('tasks.execution.diffFileCount', locale)} <strong>{visibleDiff?.files.length ?? 0}</strong>
            </span>
            <span>
              {t('tasks.execution.diffArtifactPath', locale)} <strong>{visibleDiff?.diffPath ?? '-'}</strong>
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
              {visibleDiff && visibleDiff.files.length > 0 ? (
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
              <strong className={cn('delivery-status-pill', `is-${deliveryStatus}`)}>
                {t(`tasks.execution.deliveryStatus.${deliveryStatus}`, locale)}
              </strong>
            </div>
            <Button type="button" size="sm" variant="secondary" onClick={handleGenerateDelivery}>
              <ClipboardCheck className={cn('h-3.5 w-3.5', isDeliveryLoading && 'diff-spin')} aria-hidden="true" />
              {isDeliveryLoading ? t('tasks.execution.generatingDelivery', locale) : t('tasks.execution.generateDelivery', locale)}
            </Button>
          </div>

          <div className="delivery-meta-strip">
            <span>
              {t('tasks.execution.reportFile', locale)} <strong>{visibleDelivery?.reportPath ?? '-'}</strong>
            </span>
            <span>
              {t('tasks.execution.deliveryFile', locale)} <strong>{visibleDelivery?.deliveryPath ?? '-'}</strong>
            </span>
            <span>
              {t('tasks.execution.deliveryArtifact', locale)} <strong>{visibleDelivery?.artifactId ?? '-'}</strong>
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
              <p>{visibleDelivery?.report.summary ?? t('tasks.execution.deliveryEmpty', locale)}</p>
              <div className="test-report-stats">
                <MetricPill label={t('tasks.execution.reportCommands', locale)} value={(visibleDelivery?.report.commandCount ?? 0).toString()} />
                <MetricPill label={t('tasks.execution.reportPassed', locale)} value={(visibleDelivery?.report.passedCount ?? 0).toString()} />
                <MetricPill label={t('tasks.execution.reportFailed', locale)} value={(visibleDelivery?.report.failedCount ?? 0).toString()} />
              </div>
              <div className="validation-run-table" aria-label={t('tasks.execution.validationRuns', locale)}>
                {visibleCommandRuns.length > 0 ? (
                  visibleCommandRuns.map((run) => <ValidationRunRow key={run.runId} run={run} />)
                ) : (
                  <div className="delivery-empty-state">{t('tasks.execution.noValidationRuns', locale)}</div>
                )}
              </div>
              <div className="validation-round-list" aria-label={t('tasks.execution.validationRounds', locale)}>
                {visibleValidationRounds.length > 0 ? (
                  visibleValidationRounds.map((round) => (
                    <article key={round.id} className="validation-round-row">
                      <strong>
                        {t('tasks.execution.validationRound', locale)} {round.roundIndex} · {round.status}
                      </strong>
                      <span>{round.analysis || round.validationSummary}</span>
                      <small>{round.repairSummary || '-'}</small>
                    </article>
                  ))
                ) : (
                  <div className="delivery-empty-state">{t('tasks.execution.noValidationRounds', locale)}</div>
                )}
              </div>
            </article>

            <article className="delivery-summary-card">
              <header>
                <ClipboardCheck className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.execution.agentDelivery', locale)}</span>
              </header>
              <pre className="delivery-summary-block">
                <code>{visibleDelivery?.summary ?? t('tasks.execution.deliveryEmpty', locale)}</code>
              </pre>
              <div className="commit-message-box">
                <span>{t('tasks.execution.commitMessage', locale)}</span>
                <code>{visibleDelivery?.commitMessage ?? '-'}</code>
              </div>
            </article>
          </div>
        </section>

        <section className="s12-highlight-panel" aria-label={t('tasks.s12.title', locale)}>
          <div className="s12-highlight-heading">
            <div>
              <span>{t('tasks.s12.title', locale)}</span>
              <p>{formatDeliveryReviewSummary(visibleReview, visibleProofPack?.summaryKey, locale)}</p>
            </div>
            <Button type="button" size="sm" variant="secondary" onClick={handleGenerateProofPack}>
              <PackageCheck className={cn('h-3.5 w-3.5', isProofPackLoading && 'diff-spin')} aria-hidden="true" />
              {isProofPackLoading ? t('tasks.s12.generating', locale) : t('tasks.s12.generate', locale)}
            </Button>
          </div>

          {proofPackError ? (
            <div className="diff-error-banner" role="status">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{proofPackError}</span>
            </div>
          ) : null}

          <div className={cn('s12-review-status-strip', `is-${visibleReview?.status ?? 'blocked'}`)}>
            <span>
              {t('tasks.s12.reviewState', locale)}{' '}
              <strong>{t(`tasks.s12.reviewStatus.${visibleReview?.status ?? 'blocked'}`, locale)}</strong>
            </span>
            <span>
              {t('tasks.s12.proofPack.status', locale)}{' '}
              <strong>{t(`tasks.s12.proofPackStatus.${visibleProofPackStatus}`, locale)}</strong>
            </span>
            <span>
              {t('tasks.execution.mergeEvidence', locale)}{' '}
              <strong>{visibleReview?.diffFileCount ?? visibleDiff?.files.length ?? 0} {t('tasks.execution.mergeFiles', locale)}</strong>
            </span>
            <span>
              {t('tasks.execution.mergeValidation', locale)}{' '}
              <strong>{t(`tasks.execution.deliveryStatus.${visibleReview?.validationStatus ?? deliveryStatus}`, locale)}</strong>
            </span>
          </div>

          {visibleReview?.blockers.length ? (
            <div className="s12-review-blockers" role="status">
              <strong>{t('tasks.s12.blockers', locale)}</strong>
              <div>
                {visibleReview.blockers.map((blocker) => (
                  <span key={blocker}>
                    <AlertCircle className="h-3.5 w-3.5" aria-hidden="true" />
                    {formatDeliveryReviewBlocker(blocker, locale)}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          <div className="s12-highlight-grid">
            <article className="s12-proof-pack">
              <header>
                <PackageCheck className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.proofPack.title', locale)}</span>
              </header>
              <code>{visibleProofPackPath ?? '-'}</code>
              <small>{visibleProofPackId ?? t(`tasks.s12.proofPackStatus.${visibleProofPackStatus}`, locale)}</small>
            </article>

            <article className="s12-delivery-score">
              <header>
                <Gauge className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.deliveryScore.title', locale)}</span>
              </header>
              {visibleDeliveryScore ? (
                <>
                  <strong>
                    {visibleDeliveryScore.value}
                    <small>{visibleDeliveryScore.grade}</small>
                  </strong>
                  <p>{formatDeliveryScoreSummary(visibleDeliveryScore, locale)}</p>
                </>
              ) : (
                <p>{t('tasks.s12.empty', locale)}</p>
              )}
            </article>

            <article className="s12-quality-gate">
              <header>
                <ShieldCheck className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.qualityGate.title', locale)}</span>
              </header>
              <div className="s12-check-list">
                {visibleQualityGates.length ? visibleQualityGates.map((gate) => (
                  <span key={gate.id} className={cn('s12-status-pill', `is-${gate.status}`)}>
                    {t(gate.titleKey, locale)}
                    <em>{t(`tasks.s12.status.${gate.status}`, locale)}</em>
                  </span>
                )) : <span>{t('tasks.s12.empty', locale)}</span>}
              </div>
            </article>

            <article className="s12-risk-radar">
              <header>
                <Radar className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.riskRadar.title', locale)}</span>
              </header>
              <div className="s12-risk-list">
                {visibleRiskItems.length ? visibleRiskItems.map((risk) => (
                  <span key={risk.id} className={cn('s12-risk-pill', `risk-${risk.level}`)}>
                    {formatRiskTitle(risk, locale)}
                    <em>{t(`approvals.risk.${risk.level}`, locale)}</em>
                  </span>
                )) : <span>{t('tasks.s12.empty', locale)}</span>}
              </div>
            </article>
          </div>

          <div className="s12-secondary-grid">
            <article className="s12-proposal-cards">
              <header>
                <Layers3 className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.proposals.title', locale)}</span>
              </header>
              <div>
                {visibleProofPack ? visibleProofPack.proposals.map((proposal) => (
                  <section key={proposal.id}>
                    <strong>{t(proposal.titleKey, locale)}</strong>
                    <p>{t(proposal.summaryKey, locale)}</p>
                    <small>
                      {t(`tasks.s12.status.${proposal.status}`, locale)} / {proposal.confidence}%
                    </small>
                  </section>
                )) : <section>{t('tasks.s12.empty', locale)}</section>}
              </div>
            </article>

            <article className="s12-screenshots-panel">
              <header>
                <Camera className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.screenshots.title', locale)}</span>
              </header>
              <div>
                {visibleProofPack ? visibleProofPack.screenshots.map((screenshot) => (
                  <section key={screenshot.id}>
                    <strong>{t(screenshot.titleKey, locale)}</strong>
                    <code>{screenshot.path}</code>
                    <small>
                      {screenshot.capturedAt} / {t(`tasks.s12.status.${screenshot.status}`, locale)}
                    </small>
                  </section>
                )) : <section>{t('tasks.s12.empty', locale)}</section>}
              </div>
            </article>

            <article className="s12-privacy-panel">
              <header>
                <CircleDot className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.privacy.title', locale)}</span>
              </header>
              {visiblePrivacySummary ? (
                <section>
                  <strong>{formatReviewStatus(visiblePrivacySummary.status, locale)}</strong>
                  <p>
                    {visiblePrivacySummary.entryCount} {t('tasks.s12.privacy.entries', locale)}
                    {' / '}
                    {visiblePrivacySummary.blockedCount} {t('tasks.s12.privacy.blocked', locale)}
                    {' / '}
                    {visiblePrivacySummary.redactedCount} {t('tasks.s12.privacy.redacted', locale)}
                  </p>
                  <small>{visiblePrivacySummary.latestEntry ?? t('tasks.s12.empty', locale)}</small>
                </section>
              ) : (
                <section>{t('tasks.s12.empty', locale)}</section>
              )}
            </article>

            <article className="s12-contract-panel">
              <header>
                <ClipboardCheck className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.contract.title', locale)}</span>
              </header>
              {visibleRunContractSummary ? (
                <section>
                  <strong>{formatReviewStatus(visibleRunContractSummary.status, locale)}</strong>
                  <p>
                    {visibleRunContractSummary.mode ?? '-'}
                    {' / '}
                    {visibleRunContractSummary.permissionLevel ?? '-'}
                    {' / '}
                    {visibleRunContractSummary.networkPolicy ?? '-'}
                  </p>
                  <small>
                    {visibleRunContractSummary.unresolvedBreachCount}
                    {' '}
                    {t('tasks.s12.contract.unresolved', locale)}
                  </small>
                </section>
              ) : (
                <section>{t('tasks.s12.empty', locale)}</section>
              )}
            </article>

            <article className="s12-token-budget-panel">
              <header>
                <Gauge className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.tokenBudget.title', locale)}</span>
              </header>
              {visibleTokenBudgetSummary ? (
                <section>
                  <strong>{formatReviewStatus(visibleTokenBudgetSummary.status, locale)}</strong>
                  <p>
                    {visibleTokenBudgetSummary.totalTokensEstimate}
                    {' / '}
                    {visibleTokenBudgetSummary.budgetLimit}
                  </p>
                  <small>
                    {t('tasks.s12.tokenBudget.remaining', locale)}
                    {' '}
                    {visibleTokenBudgetSummary.budgetRemaining}
                  </small>
                </section>
              ) : (
                <section>{t('tasks.s12.empty', locale)}</section>
              )}
            </article>

            <article className="s12-proof-files-panel">
              <header>
                <FileText className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.proofFiles.title', locale)}</span>
              </header>
              <div className="s12-check-list">
                {visibleProofPackFiles.length ? visibleProofPackFiles.map((file) => (
                  <span key={`${file.fileType}-${file.path}`} className={cn('s12-status-pill', `is-${statusTone(file.status)}`)}>
                    {formatProofFileLabel(file.fileType)}
                    <em>{formatBytes(file.sizeBytes)}</em>
                  </span>
                )) : <span>{t('tasks.s12.empty', locale)}</span>}
              </div>
            </article>

            <article className="s12-rules-panel">
              <header>
                <ListFilter className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.rules.title', locale)}</span>
              </header>
              <div className="s12-check-list">
                {visibleRuleHits.length ? visibleRuleHits.map((hit) => (
                  <span key={hit.id} className={cn('s12-status-pill', `is-${statusTone(hit.status)}`)}>
                    {hit.rule}
                    <em>{formatReviewStatus(hit.status, locale)}</em>
                  </span>
                )) : <span>{t('tasks.s12.empty', locale)}</span>}
              </div>
            </article>

            <article className="s12-hooks-panel">
              <header>
                <Command className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.hooks.title', locale)}</span>
              </header>
              <div className="s12-check-list">
                {visibleHookRuns.length ? visibleHookRuns.map((run) => (
                  <span key={run.id} className={cn('s12-status-pill', `is-${statusTone(run.status)}`)}>
                    {run.hook}
                    <em>{formatReviewStatus(run.approvalStatus ?? run.status, locale)}</em>
                  </span>
                )) : <span>{t('tasks.s12.empty', locale)}</span>}
              </div>
            </article>

            <article className="s12-model-arena-panel">
              <header>
                <SlidersHorizontal className="h-4 w-4" aria-hidden="true" />
                <span>{t('tasks.s12.modelArena.title', locale)}</span>
              </header>
              {visibleModelArenaDecision ? (
                <section>
                  <strong>{formatReviewStatus(visibleModelArenaDecision.status, locale)}</strong>
                  <p>{visibleModelArenaDecision.rationale}</p>
                  <small>
                    {(visibleModelArenaDecision.selectedModel ?? t('tasks.s12.modelArena.noSelection', locale))}
                    {' / '}
                    {visibleModelArenaDecision.comparedModels.length} {t('tasks.s12.modelArena.compared', locale)}
                  </small>
                </section>
              ) : (
                <section>{t('tasks.s12.empty', locale)}</section>
              )}
            </article>
          </div>
        </section>

        <section className="merge-panel">
          <div className="merge-heading">
            <div>
              <span>{t('tasks.execution.mergeTitle', locale)}</span>
              <strong className={cn('merge-status-pill', visibleMerge?.canMerge ? 'is-ready' : 'is-blocked')}>
                {visibleMerge?.canMerge
                  ? t('tasks.execution.mergeReady', locale)
                  : t('tasks.execution.mergeBlockedStatus', locale)}
              </strong>
            </div>
            <div className="merge-actions">
              <Button type="button" size="sm" variant="secondary" onClick={handlePrepareMerge}>
                <RefreshCw className={cn('h-3.5 w-3.5', isMergePreparing && 'diff-spin')} aria-hidden="true" />
                {isMergePreparing ? t('tasks.execution.mergePreparing', locale) : t('tasks.execution.mergePrecheck', locale)}
              </Button>
              <Button
                type="button"
                size="sm"
                onClick={handleOpenMergeDialog}
                disabled={isMergeLoading}
              >
                <GitMerge className={cn('h-3.5 w-3.5', isMergeLoading && 'diff-spin')} aria-hidden="true" />
                {isMergeLoading ? t('tasks.execution.merging', locale) : t('tasks.execution.mergeAction', locale)}
              </Button>
            </div>
          </div>

          <div className="merge-meta-strip">
            <span>
              {t('tasks.execution.mergeTarget', locale)} <strong>{visibleMerge?.targetBranch ?? '-'}</strong>
            </span>
            <span>
              {t('tasks.execution.mergeSource', locale)} <strong>{visibleMerge?.sourceBranch ?? '-'}</strong>
            </span>
            <span>
              {t('tasks.execution.mergeDiff', locale)} <strong>{formatDiffStat(visibleMerge?.additions ?? 0, visibleMerge?.deletions ?? 0)}</strong>
            </span>
            <span>
              {t('tasks.execution.mergeValidation', locale)}{' '}
              <strong>{t(`tasks.execution.deliveryStatus.${visibleMerge?.validationStatus ?? 'notRun'}`, locale)}</strong>
            </span>
          </div>

          {mergeError ? (
            <div className="diff-error-banner" role="alert">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{mergeError}</span>
            </div>
          ) : null}

          {mergeResult?.status === 'merged' ? (
            <div className="merge-success-banner" role="status">
              <Check className="h-4 w-4" aria-hidden="true" />
              <span>
                {t('tasks.execution.mergeSuccess', locale)} <strong>{mergeResult.commitSha}</strong>
              </span>
              {mergeResult.mergeRecordPath ? <code>{mergeResult.mergeRecordPath}</code> : null}
            </div>
          ) : null}

          {mergeResult?.status === 'conflicted' ? (
            <div className="merge-conflict-box" role="alert">
              <strong>{t('tasks.execution.mergeConflictFiles', locale)}</strong>
              <div>
                {mergeResult.conflictFiles.map((file) => (
                  <code key={file}>{file}</code>
                ))}
              </div>
              {mergeResult.errorReason ? (
                <pre className="merge-conflict-reason">
                  <strong>{t('tasks.execution.mergeConflictReason', locale)}</strong>
                  <code>{mergeResult.errorReason}</code>
                </pre>
              ) : null}
              {mergeResult.mergeRecordPath ? (
                <code className="merge-record-path">
                  {t('tasks.execution.mergeRecordPath', locale)} {mergeResult.mergeRecordPath}
                </code>
              ) : null}
            </div>
          ) : null}

          {visibleMerge?.blockers.length ? (
            <div className="merge-blocker-list">
              {visibleMerge.blockers.map((blocker) => (
                <span key={blocker}>
                  <AlertCircle className="h-3.5 w-3.5" aria-hidden="true" />
                  {formatMergeBlocker(blocker, locale)}
                </span>
              ))}
            </div>
          ) : null}

          <div className="merge-check-grid">
            <MergeCheckItem
              label={t('tasks.execution.mergeTargetClean', locale)}
              value={visibleMerge ? (visibleMerge.targetDirty ? t('tasks.execution.mergeDirty', locale) : t('tasks.execution.mergeClean', locale)) : '-'}
              good={visibleMerge ? !visibleMerge.targetDirty : false}
            />
            <MergeCheckItem
              label={t('tasks.execution.mergeWorktreeChanges', locale)}
              value={visibleMerge ? (visibleMerge.worktreeDirty ? t('tasks.execution.mergeHasChanges', locale) : t('tasks.execution.mergeNoChanges', locale)) : '-'}
              good={visibleMerge ? visibleMerge.worktreeDirty : false}
            />
            <MergeCheckItem
              label={t('tasks.execution.mergeEvidence', locale)}
              value={`${visibleMerge?.diffFileCount ?? 0} ${t('tasks.execution.mergeFiles', locale)}`}
              good={(visibleMerge?.diffFileCount ?? 0) > 0}
            />
            <MergeCheckItem
              label={t('tasks.execution.mergeValidationSummary', locale)}
              value={visibleMerge ? formatMergeValidationSummary(visibleMerge, locale) : t('tasks.execution.mergePrecheckRequired', locale)}
              good={visibleMerge?.validationStatus === 'passed'}
            />
          </div>

          <div className="merge-commit-preview">
            <span>{t('tasks.execution.mergeCommitMessage', locale)}</span>
            <code>{visibleMerge?.commitMessage ?? '-'}</code>
          </div>

          <Dialog open={mergeDialogOpen} onOpenChange={setMergeDialogOpen}>
            <DialogContent className="merge-confirm-dialog">
              <DialogHeader>
                <DialogTitle>{t('tasks.execution.mergeConfirmTitle', locale)}</DialogTitle>
                <DialogDescription>{t('tasks.execution.mergeConfirmBody', locale)}</DialogDescription>
              </DialogHeader>
              <div className="merge-confirm-summary">
                <span>
                  {t('tasks.execution.mergeTarget', locale)} <strong>{visibleMerge?.targetBranch ?? '-'}</strong>
                </span>
                <span>
                  {t('tasks.execution.mergeSource', locale)} <strong>{visibleMerge?.sourceBranch ?? '-'}</strong>
                </span>
                <span>
                  {t('tasks.execution.mergeDiff', locale)} <strong>{formatDiffStat(visibleMerge?.additions ?? 0, visibleMerge?.deletions ?? 0)}</strong>
                </span>
                <span>
                  {t('tasks.execution.mergeTargetClean', locale)}{' '}
                  <strong>{visibleMerge ? (visibleMerge.targetDirty ? t('tasks.execution.mergeDirty', locale) : t('tasks.execution.mergeClean', locale)) : '-'}</strong>
                </span>
                <span>
                  {t('tasks.execution.mergeValidation', locale)}{' '}
                  <strong>{t(`tasks.execution.deliveryStatus.${visibleMerge?.validationStatus ?? 'notRun'}`, locale)}</strong>
                </span>
              </div>
              <div className="merge-confirm-check-grid">
                <MergeCheckItem
                  label={t('tasks.execution.mergeWorktreeChanges', locale)}
                  value={visibleMerge ? (visibleMerge.worktreeDirty ? t('tasks.execution.mergeHasChanges', locale) : t('tasks.execution.mergeNoChanges', locale)) : '-'}
                  good={visibleMerge ? visibleMerge.worktreeDirty : false}
                />
                <MergeCheckItem
                  label={t('tasks.execution.mergeEvidence', locale)}
                  value={`${visibleMerge?.diffFileCount ?? 0} ${t('tasks.execution.mergeFiles', locale)}`}
                  good={(visibleMerge?.diffFileCount ?? 0) > 0}
                />
                <MergeCheckItem
                  label={t('tasks.execution.mergeValidationSummary', locale)}
                  value={visibleMerge ? formatMergeValidationSummary(visibleMerge, locale) : t('tasks.execution.mergePrecheckRequired', locale)}
                  good={visibleMerge?.validationStatus === 'passed'}
                />
              </div>
              <label className="merge-message-field">
                <span>{t('tasks.execution.mergeCommitMessage', locale)}</span>
                <textarea
                  value={mergeCommitMessage}
                  onChange={(event) => setMergeCommitMessage(event.target.value)}
                  rows={5}
                />
              </label>
              <div className="merge-confirm-actions">
                <Button type="button" variant="ghost" onClick={() => setMergeDialogOpen(false)}>
                  {t('common.cancel', locale)}
                </Button>
                <Button type="button" onClick={handleConfirmMerge} disabled={isMergeLoading}>
                  <GitMerge className={cn('h-4 w-4', isMergeLoading && 'diff-spin')} aria-hidden="true" />
                  {isMergeLoading ? t('tasks.execution.merging', locale) : t('tasks.execution.mergeConfirmAction', locale)}
                </Button>
              </div>
            </DialogContent>
          </Dialog>
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
            <EnvironmentRow icon={Code2} labelKey="tasks.environment.changes" value={formatDiffStat(visibleDiff?.additions ?? 0, visibleDiff?.deletions ?? 0)} accent />
            <EnvironmentRow icon={Laptop} labelKey="tasks.environment.local" value={t('tasks.environment.localMode', locale)} />
            <EnvironmentRow icon={GitBranch} labelKey="tasks.environment.branch" value={visibleDiff?.branchName ?? taskRecord?.branchName ?? '-'} />
            <EnvironmentRow icon={CircleDot} labelKey="tasks.environment.commit" value={t('tasks.environment.commitValue', locale)} />
            <EnvironmentRow icon={Github} labelKey="tasks.environment.github" value={t('tasks.environment.githubValue', locale)} muted />
          </div>
          <div className="environment-source">
            <strong>{t('tasks.environment.sources', locale)}</strong>
            <span>{visibleDiff?.diffPath ?? taskRecord?.worktreePath ?? taskRecord?.repositoryPath ?? t('tasks.execution.diffNoSource', locale)}</span>
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

function MergeCheckItem({ label, value, good }: { label: string; value: string; good: boolean }) {
  return (
    <div className={cn('merge-check-item', good ? 'is-good' : 'is-risk')}>
      <span>{label}</span>
      <strong>{value}</strong>
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

function CommandRunCard({ run, taskId }: { run: CommandRunLike; taskId: string }) {
  const locale = useAppStore((state) => state.locale);
  const statusKey = `tasks.execution.commandStatus.${run.status}`;
  const [expanded, setExpanded] = useState(false);
  const [stdout, setStdout] = useState<CommandLogPage | null>(null);
  const [stderr, setStderr] = useState<CommandLogPage | null>(null);
  const [loadingStream, setLoadingStream] = useState<CommandOutputStream | null>(null);
  const [logError, setLogError] = useState<string | null>(null);

  async function loadLogPage(stream: CommandOutputStream, append = false) {
    if (!taskId) {
      setLogError(t('tasks.execution.commandLogNoTask', locale));
      return;
    }

    const current = stream === 'stdout' ? stdout : stderr;
    const offsetBytes = append ? current?.nextOffsetBytes ?? 0 : 0;

    setLoadingStream(stream);
    setLogError(null);
    try {
      const page = await readTaskCommandLog({
        taskId,
        runId: run.runId,
        stream,
        offsetBytes,
        maxBytes: commandLogPageBytes,
      });
      const nextPage =
        append && current
          ? {
              ...page,
              content: `${current.content}${page.content}`,
              offsetBytes: current.offsetBytes,
            }
          : page;
      if (stream === 'stdout') {
        setStdout(nextPage);
      } else {
        setStderr(nextPage);
      }
    } catch (error) {
      setLogError(normalizeDiffError(error));
    } finally {
      setLoadingStream(null);
    }
  }

  async function toggleExpanded() {
    const nextExpanded = !expanded;
    setExpanded(nextExpanded);
    if (nextExpanded && !stdout && !stderr) {
      await Promise.all([loadLogPage('stdout'), loadLogPage('stderr')]);
    }
  }

  return (
    <article className="command-run-card">
      <button type="button" className="command-run-summary" onClick={toggleExpanded}>
        <Command className="h-4 w-4" aria-hidden="true" />
        {t('tasks.execution.ran', locale)} {run.command}
        <ChevronDown className="h-4 w-4" aria-hidden="true" />
      </button>
      <pre className="command-output-block">
        <code>{`$ ${run.command}\n${run.cwd}\n${t(statusKey, locale)} · ${run.exitCode ?? '-'} · ${commandRunPurposeLabel(run, locale)}`}</code>
      </pre>
      {expanded ? (
        <div className="command-log-streams">
          <CommandLogStream
            stream="stdout"
            page={stdout}
            path={commandRunLogPath(run, 'stdout')}
            loading={loadingStream === 'stdout'}
            onLoadMore={() => loadLogPage('stdout', true)}
          />
          <CommandLogStream
            stream="stderr"
            page={stderr}
            path={commandRunLogPath(run, 'stderr')}
            loading={loadingStream === 'stderr'}
            onLoadMore={() => loadLogPage('stderr', true)}
          />
          {logError ? (
            <div className="diff-error-banner" role="alert">
              <AlertCircle className="h-4 w-4" aria-hidden="true" />
              <span>{logError}</span>
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="command-success">
        <Check className="h-4 w-4" aria-hidden="true" />
        {t(statusKey, locale)}
      </div>
    </article>
  );
}

function CommandLogStream({
  stream,
  page,
  path,
  loading,
  onLoadMore,
}: {
  stream: CommandOutputStream;
  page: CommandLogPage | null;
  path?: string | null;
  loading: boolean;
  onLoadMore: () => void;
}) {
  const locale = useAppStore((state) => state.locale);
  const titleKey =
    stream === 'stdout' ? 'tasks.execution.commandStdout' : 'tasks.execution.commandStderr';

  return (
    <section className="command-log-stream">
      <header>
        <strong>{t(titleKey, locale)}</strong>
        <span>
          {t('tasks.execution.commandLogPath', locale)} <code>{path || '-'}</code>
        </span>
      </header>
      <pre>
        <code>{page?.content || t('tasks.execution.commandLogEmpty', locale)}</code>
      </pre>
      <footer>
        <span>
          {page?.compressed ? t('tasks.execution.commandLogCompressed', locale) : t('tasks.execution.commandLogPlain', locale)}
          {' · '}
          {page?.eof ? t('tasks.execution.commandLogEof', locale) : t('tasks.execution.commandLogMore', locale)}
        </span>
        {page && !page.eof ? (
          <Button type="button" size="sm" variant="ghost" onClick={onLoadMore} disabled={loading}>
            <RefreshCw className={cn('h-3.5 w-3.5', loading && 'diff-spin')} aria-hidden="true" />
            {loading ? t('tasks.execution.commandLogLoading', locale) : t('tasks.execution.commandLogLoadMore', locale)}
          </Button>
        ) : null}
      </footer>
    </section>
  );
}

function commandRunPurposeLabel(run: CommandRunLike, locale: Locale) {
  const purpose = 'purpose' in run ? run.purpose : 'validation';
  const key = `tasks.execution.commandPurpose.${purpose ?? 'validation'}`;
  const label = t(key, locale);
  return label === key ? purpose ?? 'validation' : label;
}

function commandRunLogPath(run: CommandRunLike, stream: CommandOutputStream) {
  if (!('stdoutPath' in run)) {
    return null;
  }

  return stream === 'stdout' ? run.stdoutPath : run.stderrPath;
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

function formatRepairRound(repairRound?: number | null, maxRepairRounds?: number | null) {
  if (repairRound === null || repairRound === undefined) {
    return '-';
  }

  if (maxRepairRounds === null || maxRepairRounds === undefined) {
    return repairRound.toString();
  }

  return `${repairRound}/${maxRepairRounds}`;
}

function formatValidationRequest(
  request?: { command: string; cwd: string; reason?: string; status?: string } | null,
) {
  if (!request) {
    return '-';
  }

  return `${request.command} · ${request.cwd}`;
}

function formatTaskTime(value: string) {
  return value.replace('T', ' ').replace(/\.\d+Z?$/, '').replace(/Z$/, '');
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

function formatTaskStatus(status: string, locale: Locale) {
  const statusKey = `status.${status}`;
  const label = t(statusKey, locale);
  return label === statusKey ? status : label;
}

function formatDeliveryReviewSummary(
  review: DeliveryReviewState | null,
  proofPackSummaryKey: string | undefined,
  locale: Locale,
) {
  if (review) {
    const status = t(`tasks.s12.reviewStatus.${review.status}`, locale);
    const proofStatus = t(`tasks.s12.proofPackStatus.${review.proofPackStatus}`, locale);
    return `${status} / ${proofStatus} / ${review.blockers.length} ${t('tasks.s12.blockerCount', locale)}`;
  }

  return proofPackSummaryKey ? t(proofPackSummaryKey, locale) : t('tasks.s12.empty', locale);
}

function formatDeliveryScoreSummary(score: VisibleDeliveryScore, locale: Locale) {
  if ('summaryKey' in score) {
    return t(score.summaryKey, locale);
  }

  return score.explanation || t('tasks.s12.deliveryScore.summary', locale);
}

function formatRiskTitle(risk: VisibleRiskItem, locale: Locale) {
  if (risk.label) {
    return risk.summaryKey ? `${risk.label}: ${risk.summaryKey}` : risk.label;
  }

  const label = t(risk.titleKey, locale);
  return label === risk.titleKey ? risk.titleKey : label;
}

function statusTone(status: string) {
  if (['passed', 'approved', 'selected', 'notRequired', 'generated'].includes(status)) {
    return 'passed';
  }
  if (['blocked', 'failed', 'rejected', 'approvalRequired', 'missing', 'empty'].includes(status)) {
    return 'blocked';
  }
  return 'warning';
}

function formatReviewStatus(status: string, locale: Locale) {
  const key = `tasks.s12.status.${status}`;
  const label = t(key, locale);
  return label === key ? status : label;
}

function formatProofFileLabel(fileType: string) {
  return fileType.replace(/^proof_/, '').replaceAll('_', ' ');
}

function formatBytes(sizeBytes: number) {
  if (sizeBytes < 1024) {
    return `${sizeBytes} B`;
  }
  if (sizeBytes < 1024 * 1024) {
    return `${(sizeBytes / 1024).toFixed(1)} KB`;
  }
  return `${(sizeBytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatDeliveryReviewBlocker(blocker: string, locale: Locale) {
  const normalized = blocker.toLowerCase();

  if (normalized.includes('validation')) {
    return t('tasks.s12.blocker.validation', locale);
  }
  if (normalized.includes('diff')) {
    return t('tasks.s12.blocker.diff', locale);
  }
  if (normalized.includes('proof pack')) {
    return t('tasks.s12.blocker.proofPack', locale);
  }
  if (normalized.includes('approval')) {
    return t('tasks.s12.blocker.approval', locale);
  }
  if (normalized.includes('high risk')) {
    return t('tasks.s12.blocker.highRisk', locale);
  }
  if (normalized.includes('quality gate')) {
    return t('tasks.s12.blocker.qualityGate', locale);
  }
  if (normalized.includes('rule ')) {
    return t('tasks.s12.blocker.rule', locale);
  }
  if (normalized.includes('hook ')) {
    return t('tasks.s12.blocker.hook', locale);
  }
  if (normalized.includes('privacy ledger')) {
    return t('tasks.s12.blocker.privacy', locale);
  }
  if (normalized.includes('run contract') || normalized.includes('contract breach')) {
    return t('tasks.s12.blocker.contract', locale);
  }
  if (normalized.includes('token budget')) {
    return t('tasks.s12.blocker.tokenBudget', locale);
  }

  return blocker;
}

function formatMergeBlocker(blocker: string, locale: Locale) {
  const normalized = blocker.toLowerCase();

  if (normalized.includes('target')) {
    return t('tasks.execution.mergeBlockerTargetDirty', locale);
  }
  if (normalized.includes('validation')) {
    return t('tasks.execution.mergeBlockerValidation', locale);
  }
  if (normalized.includes('diff')) {
    return t('tasks.execution.mergeBlockerDiff', locale);
  }
  if (normalized.includes('commit')) {
    return t('tasks.execution.mergeBlockerCommit', locale);
  }

  return blocker;
}

function formatMergeValidationSummary(merge: PreparedTaskMerge, locale: Locale) {
  if (merge.validationStatus === 'passed') {
    return `${merge.validationRunCount} ${t('tasks.execution.mergeValidationPassed', locale)}`;
  }
  if (merge.validationStatus === 'failed') {
    return t('tasks.execution.mergeValidationFailed', locale);
  }

  return t('tasks.execution.mergeValidationNotRun', locale);
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
