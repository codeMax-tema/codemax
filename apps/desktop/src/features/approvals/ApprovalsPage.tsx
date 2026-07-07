import { useEffect, useMemo, useState } from 'react';
import {
  AlertCircle,
  CheckCircle2,
  Clock3,
  RefreshCw,
  RotateCcw,
  ShieldAlert,
  XCircle,
} from 'lucide-react';

import { decideApproval, listPendingApprovals } from '@/api/tauriClient';
import { Button } from '@/components/ui/button';
import { t } from '@/i18n';
import { cn } from '@/lib/utils';
import { useAppStore } from '@/state/appStore';
import type { ApprovalDecision, ApprovalSummary } from '@/types/domain';

export function ApprovalsPage() {
  const locale = useAppStore((state) => state.locale);
  const [approvals, setApprovals] = useState<ApprovalSummary[]>([]);
  const [commentById, setCommentById] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [decidingId, setDecidingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const pendingCount = approvals.length;
  const highRiskCount = useMemo(
    () => approvals.filter((approval) => approval.riskLevel === 'high').length,
    [approvals],
  );

  useEffect(() => {
    void loadApprovals();
  }, []);

  async function loadApprovals() {
    setLoading(true);
    setError(null);
    try {
      setApprovals(await listPendingApprovals());
    } catch (loadError) {
      setError(normalizeApprovalError(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function handleDecision(approval: ApprovalSummary, decision: ApprovalDecision) {
    setDecidingId(approval.id);
    setError(null);
    try {
      await decideApproval({
        approvalId: approval.id,
        decision,
        comment: commentById[approval.id]?.trim() || undefined,
      });
      setApprovals((current) => current.filter((item) => item.id !== approval.id));
      setCommentById((current) => {
        const next = { ...current };
        delete next[approval.id];
        return next;
      });
    } catch (decisionError) {
      setError(normalizeApprovalError(decisionError));
    } finally {
      setDecidingId(null);
    }
  }

  return (
    <div className="approval-center-page">
      <section className="approval-center-header">
        <div>
          <span className="approval-eyebrow">
            <ShieldAlert className="h-4 w-4" aria-hidden="true" />
            {t('approvals.eyebrow', locale)}
          </span>
          <h3>{t('approvals.title', locale)}</h3>
          <p>{t('approvals.body', locale)}</p>
        </div>
        <Button type="button" variant="secondary" onClick={loadApprovals} disabled={loading}>
          <RefreshCw className={cn('h-4 w-4', loading && 'diff-spin')} aria-hidden="true" />
          {loading ? t('approvals.refreshing', locale) : t('approvals.refresh', locale)}
        </Button>
      </section>

      <section className="approval-metrics" aria-label={t('approvals.metrics', locale)}>
        <ApprovalMetric label={t('approvals.pendingCount', locale)} value={pendingCount.toString()} />
        <ApprovalMetric label={t('approvals.highRiskCount', locale)} value={highRiskCount.toString()} />
        <ApprovalMetric label={t('approvals.policy', locale)} value={t('approvals.policyValue', locale)} />
      </section>

      {error ? (
        <div className="approval-error" role="alert">
          <AlertCircle className="h-4 w-4" aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {approvals.length === 0 && !loading ? (
        <section className="empty-panel">
          <ShieldAlert className="h-5 w-5" aria-hidden="true" />
          <div>
            <h3>{t('approvals.emptyTitle', locale)}</h3>
            <p>{t('approvals.emptyBody', locale)}</p>
          </div>
        </section>
      ) : (
        <section className="approval-list" aria-label={t('approvals.pendingList', locale)}>
          {approvals.map((approval) => (
            <article className="approval-card" key={approval.id}>
              <header className="approval-card-header">
                <div>
                  <span className={cn('approval-risk-pill', `risk-${approval.riskLevel}`)}>
                    {t(`approvals.risk.${approval.riskLevel}`, locale)}
                  </span>
                  <h4>{approval.content.split('\n')[0] ?? approval.content}</h4>
                </div>
                <time>
                  <Clock3 className="h-3.5 w-3.5" aria-hidden="true" />
                  {formatTimestamp(approval.createdAt)}
                </time>
              </header>

              <dl className="approval-detail-grid">
                <div>
                  <dt>{t('approvals.taskId', locale)}</dt>
                  <dd>{approval.taskId}</dd>
                </div>
                <div>
                  <dt>{t('approvals.type', locale)}</dt>
                  <dd>{approval.approvalType ?? 'command'}</dd>
                </div>
                <div className="wide">
                  <dt>{t('approvals.operation', locale)}</dt>
                  <dd>
                    <code>{approval.content}</code>
                  </dd>
                </div>
                <div className="wide">
                  <dt>{t('approvals.reason', locale)}</dt>
                  <dd>{approval.reason ?? t('approvals.noReason', locale)}</dd>
                </div>
              </dl>

              <label className="approval-comment">
                <span>{t('approvals.comment', locale)}</span>
                <textarea
                  value={commentById[approval.id] ?? ''}
                  onChange={(event) =>
                    setCommentById((current) => ({
                      ...current,
                      [approval.id]: event.target.value,
                    }))
                  }
                  placeholder={t('approvals.commentPlaceholder', locale)}
                />
              </label>

              <footer className="approval-actions">
                <Button
                  type="button"
                  variant="secondary"
                  disabled={decidingId === approval.id}
                  onClick={() => handleDecision(approval, 'revise')}
                >
                  <RotateCcw className="h-4 w-4" aria-hidden="true" />
                  {t('approvals.revise', locale)}
                </Button>
                <Button
                  type="button"
                  variant="destructive"
                  disabled={decidingId === approval.id}
                  onClick={() => handleDecision(approval, 'rejected')}
                >
                  <XCircle className="h-4 w-4" aria-hidden="true" />
                  {t('approvals.reject', locale)}
                </Button>
                <Button
                  type="button"
                  disabled={decidingId === approval.id}
                  onClick={() => handleDecision(approval, 'approved')}
                >
                  <CheckCircle2 className="h-4 w-4" aria-hidden="true" />
                  {t('approvals.approve', locale)}
                </Button>
              </footer>
            </article>
          ))}
        </section>
      )}
    </div>
  );
}

function ApprovalMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="approval-metric">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function normalizeApprovalError(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'title' in error) {
    return String((error as { title: unknown }).title);
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

function formatTimestamp(value: string) {
  const seconds = Number(value);
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return value;
  }

  return new Date(seconds * 1000).toLocaleString();
}
