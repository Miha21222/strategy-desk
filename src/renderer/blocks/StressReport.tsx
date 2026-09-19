// stressreport: grade badge, headline, trades-weighted win-rate donut, PF /
// net / drawdown bars, runs table, notes and the after-costs footer.
import { useContext } from 'preact/hooks';
import type { StressReport } from '@shared/blocks';
import { costPerTrade, netAfterCosts, weightedWinRate } from '@shared/blocks';
import { t } from '../i18n';
import { Donut, ScaleBars, SignedBars } from './charts';
import { CardActionsContext } from './context';
import { ddClass, fmtMoney, fmtValue, gradeClass, pfClass, signClass, totals } from './format';

export function GradeBadge({ grade, small }: { grade: StressReport['grade']; small?: boolean }) {
  return <span class={`grade-badge ${gradeClass(grade)}${small ? ' small' : ''}`}>{grade}</span>;
}

export function StressReportView({ data, compact }: { data: StressReport; compact?: boolean }) {
  const actions = useContext(CardActionsContext);
  const wr = weightedWinRate(data.runs);
  const tot = totals(data.runs);
  const maxPf = Math.max(2, ...data.runs.map((r) => r.profitFactor));
  const maxDd = Math.max(25, ...data.runs.map((r) => r.maxDrawdownPct));
  const costs = data.costs ?? null;
  const perTrade = costs ? costPerTrade(costs) : null;

  return (
    <div class="card stressreport">
      <div class="card-head report-head">
        <GradeBadge grade={data.grade} />
        <div class="report-title">
          <div class="report-name">
            {data.strategy} <span class="muted">{data.version}</span>
            {data.unit ? <span class="muted"> · {data.unit}</span> : null}
          </div>
          <div class="report-headline">{data.headline}</div>
        </div>
        {!compact ? (
          <button type="button" class="small" onClick={() => actions.openReport(data)}>
            {t('cards.openReport')}
          </button>
        ) : null}
      </div>
      <div class="card-body">
        {data.changes ? (
          <div class="report-changes">
            <span class="muted">{t('cards.changes')}:</span> {data.changes}
          </div>
        ) : null}

        <div class="report-grid">
          <Donut pct={wr} label={t('cards.winRate')} sub={t('cards.tradesTotal', { n: tot.trades })} />
          <section class="chart">
            <div class="chart-label">{t('cards.profitFactor')}</div>
            <ScaleBars items={data.runs.map((r) => ({ label: r.label, value: r.profitFactor, cls: pfClass(r.profitFactor), text: r.profitFactor.toFixed(2) }))} max={maxPf} reference={1} />
          </section>
        </div>

        <section class="chart">
          <div class="chart-label">{t('cards.netProfit')}</div>
          <SignedBars items={data.runs.map((r) => ({ label: r.label, value: r.netProfit }))} format="money" />
        </section>

        <section class="chart">
          <div class="chart-label">{t('cards.maxDd')}</div>
          <ScaleBars items={data.runs.map((r) => ({ label: r.label, value: r.maxDrawdownPct, cls: ddClass(r.maxDrawdownPct), text: fmtValue(r.maxDrawdownPct, 'percent') }))} max={maxDd} />
        </section>

        <table class="runs">
          <thead>
            <tr>
              <th>{t('cards.run')}</th>
              <th>{t('cards.trades')}</th>
              <th>WR</th>
              <th>PF</th>
              <th>{t('cards.net')}</th>
              <th>DD</th>
            </tr>
          </thead>
          <tbody>
            {data.runs.map((r, i) => (
              <tr key={i} class={r.role}>
                <td>
                  {r.label}
                  {r.role === 'oos' ? <span class="tag oos">OOS</span> : r.role === 'baseline' ? <span class="tag base">{t('cards.baseline')}</span> : null}
                </td>
                <td>{r.trades}</td>
                <td>{fmtValue(r.winRate, 'percent')}</td>
                <td class={pfClass(r.profitFactor)}>{r.profitFactor.toFixed(2)}</td>
                <td class={signClass(r.netProfit)}>{fmtMoney(r.netProfit)}</td>
                <td class={ddClass(r.maxDrawdownPct)}>{fmtValue(r.maxDrawdownPct, 'percent')}</td>
              </tr>
            ))}
          </tbody>
        </table>

        {data.notes.length ? (
          <ul class="report-notes">
            {data.notes.map((n, i) => (
              <li key={i}>{n}</li>
            ))}
          </ul>
        ) : null}

        {costs && perTrade !== null ? (
          <div class="report-footer">
            <span class="muted">{t('cards.afterCosts', { cost: fmtMoney(perTrade) })}:</span>{' '}
            {data.runs.map((r, i) => {
              const after = netAfterCosts(r, costs) ?? 0;
              return (
                <span class={`after ${signClass(after)}`} key={i} title={r.label}>
                  {r.role === 'baseline' ? t('cards.baseline') : r.label} {fmtMoney(after)}
                  {i < data.runs.length - 1 ? ' · ' : ''}
                </span>
              );
            })}
          </div>
        ) : null}
      </div>
    </div>
  );
}
