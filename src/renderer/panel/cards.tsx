// Presentational cards for the «День» tab. Every one of them renders from the
// raw JSON a tvmcp tool returned — no model in the loop — and every one has an
// honest "не проверено" state, because a card that quietly shows nothing is the
// one wrong answer a risk check must never give.
import type { SentinelEvent, SentinelState } from '@shared/panel';
import { LevelsLadder } from '../blocks/charts';
import { fmtPrice } from '../blocks/format';
import { t } from '../i18n';

/** One deterministic call's lifecycle, as the panel holds it. */
export interface Slot<T = unknown> {
  status: 'idle' | 'loading' | 'ok' | 'error';
  data: T | null;
  error: string;
  warnings: string[];
  at: number;
}

export const emptySlot = <T,>(): Slot<T> => ({ status: 'idle', data: null, error: '', warnings: [], at: 0 });

export function Card({ title, slot, onRefresh, children }: { title: string; slot: Slot; onRefresh?: () => void; children?: preact.ComponentChildren }) {
  return (
    <section class={`card panel-card ${slot.status}`}>
      <div class="card-head">
        <span>{title}</span>
        <span class="spacer" />
        {slot.status === 'loading' ? <span class="muted small">{t('day.loading')}</span> : null}
        {onRefresh ? (
          <button type="button" class="small" disabled={slot.status === 'loading'} onClick={onRefresh} title={t('day.refresh')}>
            ↻
          </button>
        ) : null}
      </div>
      <div class="card-body">
        {slot.status === 'idle' ? <div class="muted">{t('day.notChecked')}</div> : null}
        {slot.status === 'error' ? (
          <div class="panel-error">
            <b>{t('day.notChecked')}</b> — {slot.error}
          </div>
        ) : null}
        {slot.status === 'ok' ? children : null}
        {slot.warnings.length ? (
          <ul class="panel-warnings">
            {slot.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        ) : null}
      </div>
    </section>
  );
}

// ---------- levels ----------

interface ScanLevel {
  name: string;
  price?: number | null;
  high?: number | null;
  low?: number | null;
  source?: string;
  note?: string | null;
}

export function LevelsBody({ data, watched }: { data: unknown; watched: Set<string> }) {
  const d = (data ?? {}) as { levels?: ScanLevel[]; adr?: { value?: number; n_used?: number; basis?: string } | null; opening_range?: Record<string, unknown> | null };
  const all = Array.isArray(d.levels) ? d.levels : [];
  const priced = all.filter((l) => typeof l.price === 'number' && Number.isFinite(l.price));
  const shown = watched.size ? priced.filter((l) => watched.has(l.name)) : priced;
  const items = (shown.length ? shown : priced).slice(0, 24).map((l) => ({ label: l.name, price: l.price as number, note: l.note ?? undefined }));
  const or = d.opening_range as { high?: number; low?: number; mid?: number; size?: number; size_vs_adr?: number; complete?: boolean } | null;
  return (
    <>
      {d.adr && typeof d.adr.value === 'number' ? (
        <div class="panel-stat-row">
          <span>
            ADR({d.adr.n_used ?? '?'}) <b>{fmtPrice(d.adr.value)}</b>
          </span>
          {d.adr.basis ? <span class="muted small">{d.adr.basis}</span> : null}
        </div>
      ) : null}
      {or && typeof or.high === 'number' && typeof or.low === 'number' ? (
        <div class="panel-stat-row">
          <span>
            {t('day.openingRange')} <b>{fmtPrice(or.low)}</b> – <b>{fmtPrice(or.high)}</b>
          </span>
          {typeof or.size === 'number' ? <span class="muted small">{t('day.size')} {fmtPrice(or.size)}{typeof or.size_vs_adr === 'number' ? ` · ${Math.round(or.size_vs_adr * 100)}% ADR` : ''}</span> : null}
          {or.complete === false ? <span class="tag warn">{t('day.orForming')}</span> : null}
        </div>
      ) : null}
      {items.length ? <LevelsLadder items={items} /> : <div class="muted">{t('day.noLevels')}</div>}
    </>
  );
}

// ---------- calendar ----------

interface CalEvent {
  time_utc?: string | null;
  time_raw?: string | null;
  country?: string;
  title?: string;
  impact?: string;
  source?: string;
}

const hhmm = (iso: string | null | undefined): string => {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? String(iso).slice(11, 16) : d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};

export function CalendarBody({ data }: { data: unknown }) {
  const d = (data ?? {}) as { events?: CalEvent[]; holidays?: { name?: string; date?: string; kind?: string }[]; rollover?: Record<string, unknown> | null; degraded?: boolean };
  const events = Array.isArray(d.events) ? d.events : [];
  const holidays = Array.isArray(d.holidays) ? d.holidays : [];
  return (
    <>
      {d.degraded ? <div class="tag warn">{t('day.calendarDegraded')}</div> : null}
      {holidays.length ? (
        <div class="panel-stat-row">
          {holidays.map((h, i) => (
            <span class="tag" key={i}>
              {h.kind === 'half_day' ? t('day.halfDay') : t('day.holiday')}: {h.name ?? h.date}
            </span>
          ))}
        </div>
      ) : null}
      {events.length ? (
        <table class="panel-table">
          <tbody>
            {events.slice(0, 20).map((e, i) => (
              <tr key={i} class={String(e.impact).toLowerCase() === 'high' ? 'high-impact' : ''}>
                <td class="nowrap">{hhmm(e.time_utc)}</td>
                <td class="nowrap">{e.country ?? ''}</td>
                <td>{e.title ?? ''}</td>
                <td class="nowrap muted">{e.impact ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div class="muted">{t('day.calendarEmpty')}</div>
      )}
    </>
  );
}

// ---------- risk ----------

interface RiskCheck {
  id?: string;
  name?: string;
  status?: string;
  detail?: string;
  numbers?: Record<string, unknown>;
}

export function RiskBody({ data }: { data: unknown }) {
  const d = (data ?? {}) as { status?: string; checks?: RiskCheck[]; fail_closed?: boolean; problems?: unknown[]; warnings?: string[] };
  const checks = Array.isArray(d.checks) ? d.checks : [];
  const status = String(d.status ?? 'unknown');
  return (
    <>
      <div class="panel-stat-row">
        <span class={`verdict risk-${status}`}>{t(`day.risk.${status}`)}</span>
        {d.fail_closed ? <span class="tag bad">{t('day.failClosed')}</span> : null}
      </div>
      {checks.length ? (
        <table class="panel-table">
          <tbody>
            {checks.map((c, i) => (
              <tr key={i} class={`risk-${String(c.status ?? 'unknown')}`}>
                <td class="nowrap">
                  <span class={`dot ${c.status === 'ok' ? 'on' : c.status === 'stop' ? 'off' : 'unknown'}`} />
                  {c.name ?? c.id}
                </td>
                <td>{c.detail ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div class="muted">{t('day.riskEmpty')}</div>
      )}
    </>
  );
}

// ---------- journal ----------

export function JournalBody({ data }: { data: unknown }) {
  const d = (data ?? {}) as {
    stats?: Record<string, unknown>;
    total_count?: number;
    fail_closed?: boolean;
    problems?: unknown[];
  };
  const s = (d.stats ?? {}) as Record<string, number | null | string>;
  const cell = (label: string, value: unknown, digits = 2) => (
    <div class="stat-tile">
      <div class="stat-value">{typeof value === 'number' ? value.toFixed(digits) : value == null || value === '' ? '—' : String(value)}</div>
      <div class="stat-label">{label}</div>
    </div>
  );
  return (
    <>
      {d.fail_closed ? <div class="tag bad">{t('day.failClosed')}</div> : null}
      <div class="stat-tiles">
        {cell(t('day.jTrades'), s.trades, 0)}
        {cell(t('day.jWinRate'), s.win_rate_pct, 1)}
        {cell(t('day.jTotalR'), s.total_r)}
        {cell(t('day.jExpR'), s.expectancy_r)}
        {cell(t('day.jPf'), s.profit_factor_r ?? s.profit_factor)}
        {cell(t('day.jDays'), s.trading_days, 0)}
      </div>
      <div class="muted small">
        {t('day.jRange', { from: String(s.first_date ?? '—'), to: String(s.last_date ?? '—') })}
      </div>
    </>
  );
}

// ---------- the sentinel run ----------

const PAYLOAD_KEYS = ['high', 'low', 'size', 'size_vs_adr', 'side', 'level_name', 'level', 'boundary', 'price', 'close', 'name', 'wick_beyond', 'closed_back', 'age_seconds', 'reason'];

/** The numbers that matter from an event payload, as one short line. */
export function payloadLine(ev: SentinelEvent): string {
  const p = ev.payload ?? {};
  const parts: string[] = [];
  for (const k of PAYLOAD_KEYS) {
    const v = (p as Record<string, unknown>)[k];
    if (v === undefined || v === null || v === '') continue;
    parts.push(`${k} ${typeof v === 'number' ? fmtPrice(v) : String(v).slice(0, 80)}`);
  }
  return parts.join(' · ');
}

export function EventFeed({ events, asked }: { events: SentinelEvent[]; asked: Set<number> }) {
  if (!events.length) return <div class="muted">{t('day.feedEmpty')}</div>;
  return (
    <div class="feed">
      {[...events].reverse().map((e) => (
        <div class={`feed-row ev-${e.type}`} key={e.seq}>
          <span class="feed-time">{hhmm(e.ts_utc)}</span>
          <span class="feed-type">{e.type}</span>
          <span class="feed-payload">{payloadLine(e)}</span>
          {asked.has(e.seq) ? (
            <span class="tag base" title={t('day.askedHint')}>
              {t('day.asked')}
            </span>
          ) : null}
        </div>
      ))}
    </div>
  );
}

/** One line describing where the run stands, in the user's words. */
export function runPhaseLabel(state: SentinelState | null): string {
  if (!state) return t('day.phase.none');
  if (state.status === 'stopped') return t('day.phase.stopped');
  const waiting = String(state.waiting_for ?? '');
  const phase = String(state.phase ?? 'pending');
  if (phase === 'pending' || waiting === 'session_open') return t('day.phase.beforeOpen');
  if (phase === 'session_open' || phase === 'range_forming' || waiting === 'range_close') return t('day.phase.rangeForming');
  if (waiting === 'confirmation') return t('day.phase.breaking');
  if (waiting === 'retest') return t('day.phase.retest');
  if (phase === 'range_closed') return t('day.phase.rangeClosed');
  if (phase === 'session_closed') return t('day.phase.sessionClosed');
  return phase;
}

export function RunSummary({ state }: { state: SentinelState | null }) {
  if (!state) return null;
  const r = (state.range ?? {}) as { high?: number | null; low?: number | null; size?: number | null; closed?: boolean };
  return (
    <div class="panel-stat-row">
      <span class="tag base">{runPhaseLabel(state)}</span>
      {typeof r.high === 'number' && typeof r.low === 'number' ? (
        <span>
          {t('day.range')} <b>{fmtPrice(r.low)}</b> – <b>{fmtPrice(r.high)}</b>
          {typeof r.size === 'number' ? <span class="muted small"> · {fmtPrice(r.size)}</span> : null}
        </span>
      ) : null}
      <span class="muted small">
        {t('day.bars')} {state.bars_seen ?? 0} · seq {state.last_seq ?? 0}
      </span>
    </div>
  );
}
