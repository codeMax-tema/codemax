import { ShieldAlert } from 'lucide-react';

import { t } from '@/i18n';
import { useAppStore } from '@/state/appStore';

export function ApprovalsPage() {
  const locale = useAppStore((state) => state.locale);

  return (
    <section className="empty-panel">
      <ShieldAlert className="h-5 w-5" aria-hidden="true" />
      <div>
        <h3>{t('approvals.emptyTitle', locale)}</h3>
        <p>{t('approvals.emptyBody', locale)}</p>
      </div>
    </section>
  );
}
