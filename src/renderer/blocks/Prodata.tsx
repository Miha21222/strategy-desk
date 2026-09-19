// The prodata card: the model hands over raw trades, the analytics module does
// every calculation here in the renderer, and this only lays the numbers out.
// Nothing shown below is taken from the model's own arithmetic.
import { useEffect, useState } from 'preact/hooks';
import type { Prodata } from '@shared/blocks';
import {
  analyzeProdata,
  AnalyticsError,
  type Prodata as AnalyticsProdata,
  type PropFirmsData,
  type Report,
  type SizeResult,
} from '../../analytics';
import { t } from '../i18n';
import { SignedBars, StatTiles } from './charts';
import { fmtMoney, fmtValue } from './format';

const fmtNum = (v: number, digits = 0) => (Number.isFinite(v) ? v.toFixed(digits) : '—');
const fmtPct = (v: number) => (Number.isFinite(v) ? `${v.toFixed(1)}%` : '—');

let rulesPromise: Promise<PropFirmsData | null> | null = null;

function loadRules(): Promise<PropFirmsData | null> {
  if (!rulesPromise) rulesPromise = window.desk.rules.get().then((r) => (r as PropFirmsData) ?? null);
  return rulesPromise;
}

// The block schema leaves a few fields optional; the analytics module wants
// them decided. Futures default to one mini, forex to lots.
function toAnalytics(d: Prodata): AnalyticsProdata {
  const unit = d.backtestUnit ?? (d.market === 'futures' ? 'mini' : 'lot');
  return { ...d, backtestUnit: unit } as AnalyticsProdata;
}

function scoreClass(letter: string): string {
  const head = letter.charAt(0).toUpperCase();
  return head === 'A' ? 'grade-a' : head === 'B' ? 'grade-b' : head === 'C' ? 'grade-c' : head === 'D' ? 'grade-d' : 'grade-f';
}

function SurvivalTable({ grid }: { grid: readonly SizeResult[] }) {
  return (
    <table class="runs">
      <thead>
        <tr>
          <th>{t('report.size')}</th>
          <th>{t('report.passPct')}</th>
          <th>{t('report.breach')}</th>
          <th>{t('report.days')}</th>
          <th>{t('report.cost')}</th>
        </tr>
      </thead>
      <tbody>
        {grid.map((s) => (
          <tr key={s.sizeLabel}>
            <td>{s.sizeLabel}</td>
            <td class={s.passPct >= 70 ? 'pos' : s.passPct >= 40 ? 'warn' : 'neg'}>{fmtPct(s.passPct)}</td>
            <td class="muted">{fmtPct(s.breachTrailingPct + s.breachDailyPct)}</td>
            <td>{s.daysToPass ? fmtNum(s.daysToPass.p50) : '—'}</td>
            <td>{s.expCost == null ? '—' : fmtMoney(s.expCost)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function ReportView({ report }: { report: Report }) {
  const h = report.header;
  const q = report.quality;
  const tiles = [
    { label: t('report.trades'), value: fmtNum(h.n) },
    { label: t('report.winRate'), value: fmtPct(h.winRate) },
    { label: t('report.profitFactor'), value: fmtNum(h.profitFactor, 2) },
    { label: t('report.net'), value: fmtMoney(h.net) },
    { label: t('report.maxDd'), value: fmtMoney(h.maxDrawdown) },
    { label: t('report.ddP95'), value: fmtMoney(report.bootstrap.maxDD.p95) },
  ];
  return (
    <div class="card-body">
      <div class="report-head">
        {q ? (
          <div class={`grade ${scoreClass(q.score.letter)}`} title={t('report.qualityHint')}>
            <b>{q.score.letter}</b>
            <span>{fmtNum(q.score.score)}</span>
          </div>
        ) : null}
        <div class="report-sub muted">
          {h.from} — {h.to} · {t('report.perUnit', { unit: h.unitLabel })} · {t('report.days')}: {fmtNum(h.days)}
        </div>
      </div>

      <StatTiles items={tiles} />

      {q ? (
        <SignedBars
          format="number"
          items={[
            { label: t('report.edge'), value: q.score.edge },
            { label: t('report.risk'), value: q.score.risk },
            { label: t('report.robust'), value: q.score.robust },
            { label: t('report.consist'), value: q.score.consist },
          ]}
        />
      ) : null}

      {q?.score.caps.length ? (
        <div class="card-note">
          {t('report.caps')}: {q.score.caps.map((c) => `${c.reason} → ${c.cap}`).join(' · ')}
        </div>
      ) : null}

      {q ? (
        <div class="card-note">
          {t('report.trueCost', {
            per: fmtMoney(q.trueCost.costPerTrade),
            after: fmtMoney(q.trueCost.afterNet),
            keep: fmtPct(q.trueCost.keepPct),
          })}
        </div>
      ) : null}

      {report.evalGrid?.length
        ? report.evalGrid.map((acc) => (
            <div class="report-account" key={acc.accountId}>
              <div class="report-account-head">
                <b>{acc.label}</b>
                <span class="muted">{t('report.paths', { n: acc.paths })}</span>
              </div>
              <SurvivalTable grid={acc.sizes} />
              {acc.notes.map((n) => (
                <div class="card-note" key={n}>
                  {n}
                </div>
              ))}
            </div>
          ))
        : null}

      {report.notes.map((n) => (
        <div class="card-note" key={n}>
          {n}
        </div>
      ))}
      {report.simCaveat ? <div class="card-note muted">{report.simCaveat}</div> : null}
    </div>
  );
}

export function ProdataCard({ data }: { data: Prodata }) {
  const [state, setState] = useState<{ report?: Report; error?: string; loading: boolean }>({ loading: true });

  useEffect(() => {
    let alive = true;
    loadRules()
      .then((rules) => {
        if (!alive) return;
        if (!rules) {
          setState({ loading: false, error: t('report.noRules') });
          return;
        }
        try {
          setState({ loading: false, report: analyzeProdata(toAnalytics(data), rules) });
        } catch (e) {
          const msg = e instanceof AnalyticsError || e instanceof Error ? e.message : String(e);
          setState({ loading: false, error: msg });
        }
      })
      .catch((e: unknown) => {
        if (alive) setState({ loading: false, error: e instanceof Error ? e.message : String(e) });
      });
    return () => {
      alive = false;
    };
  }, [data]);

  return (
    <div class="card prodata">
      <div class="card-head">
        <span>
          {t('cards.prodata')} · {data.strategy} <span class="muted">{data.version}</span>
        </span>
        <span class="muted">
          {data.market} · {t('cards.focus')}: {data.focus} · {t('cards.tradesTotal', { n: data.trades.length })}
        </span>
      </div>
      {state.loading ? <div class="card-body muted">{t('report.computing')}</div> : null}
      {state.error ? <div class="card-body card-note neg">{state.error}</div> : null}
      {state.report ? <ReportView report={state.report} /> : null}
    </div>
  );
}
