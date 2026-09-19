// Right-side report drawer: the selected stress report in full plus the
// chat's report history (newest first) with delete.
import type { StoredReport } from '@shared/api';
import { GradeBadge, StressReportView } from '../blocks';
import { t } from '../i18n';

export interface ReportDrawerProps {
  open: boolean;
  reports: StoredReport[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onClose: () => void;
}

function fmtDate(ts: number): string {
  const d = new Date(ts);
  return `${d.toLocaleDateString('ru-RU')} ${d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}`;
}

export function ReportDrawer(p: ReportDrawerProps) {
  const sorted = [...p.reports].sort((a, b) => b.createdAt - a.createdAt);
  const selected = sorted.find((r) => r.id === p.selectedId) ?? sorted[0] ?? null;
  const versions = new Set(p.reports.map((r) => r.version)).size;
  return (
    <aside class={`drawer${p.open ? ' open' : ''}`} aria-hidden={!p.open}>
      <header class="drawer-head">
        <span class="drawer-title">
          {t('drawer.title')}
          {p.reports.length ? (
            <span class="muted">
              {' '}
              · {p.reports.length} · {versions} {t('drawer.versions')}
            </span>
          ) : null}
        </span>
        <button type="button" class="small" onClick={p.onClose} title={t('drawer.close')}>
          ✕
        </button>
      </header>
      <div class="drawer-body">
        {!sorted.length ? <div class="empty">{t('drawer.empty')}</div> : null}
        {sorted.length ? (
          <div class="report-list">
            {sorted.map((r) => (
              <button type="button" key={r.id} class={`report-item${selected?.id === r.id ? ' on' : ''}`} onClick={() => p.onSelect(r.id)}>
                <GradeBadge grade={r.grade} small />
                <span class="report-item-title">
                  {r.strategy} <span class="muted">{r.version}</span>
                </span>
                <span class="muted report-item-date">{fmtDate(r.createdAt)}</span>
              </button>
            ))}
          </div>
        ) : null}
        {selected ? (
          <div class="drawer-report">
            <StressReportView data={selected.data} compact />
            <button type="button" class="small danger-text" onClick={() => p.onDelete(selected.id)}>
              {t('drawer.delete')}
            </button>
          </div>
        ) : null}
      </div>
    </aside>
  );
}
