// Hand-rolled chart primitives shared by the cards. Everything is plain SVG
// or CSS classes; sizes and colours come from computed attributes and CSS
// variables, never from inline `style=` strings built from model data.
import type { ChartFormat } from '@shared/blocks';
import { t } from '../i18n';
import { donutDash, fmtDistance, fmtPrice, fmtValue, isNegativeText, levelsLadder, positiveBar, signedBar } from './format';

export function StatTiles({ items }: { items: { label: string; value: string | number }[] }) {
  return (
    <div class="stat-tiles">
      {items.map((it, i) => (
        <div class={`stat-tile${isNegativeText(it.value) ? ' neg' : ''}`} key={i}>
          <div class="stat-value">{typeof it.value === 'number' ? fmtValue(it.value) : it.value}</div>
          <div class="stat-label">{it.label}</div>
        </div>
      ))}
    </div>
  );
}

const BAR_W = 240;
const BAR_H = 16;
const ROW_H = 22;

/** Horizontal signed bars: negatives grow left from the centre line. */
export function SignedBars({ items, format }: { items: { label: string; value: number }[]; format: ChartFormat }) {
  const maxAbs = Math.max(...items.map((i) => Math.abs(i.value)), 0);
  const height = items.length * ROW_H;
  return (
    <div class="bars">
      <div class="bars-labels">
        {items.map((it, i) => (
          <div class="bar-label" key={i} title={it.label}>
            {it.label}
          </div>
        ))}
      </div>
      <svg class="bars-svg" viewBox={`0 0 ${BAR_W} ${height}`} width="100%" height={height} preserveAspectRatio="none" role="img">
        <line class="bar-axis" x1={BAR_W / 2} x2={BAR_W / 2} y1={0} y2={height} />
        {items.map((it, i) => {
          const g = signedBar(it.value, maxAbs, BAR_W);
          return <rect key={i} class={`bar ${it.value < 0 ? 'neg' : 'pos'}`} x={g.x} y={i * ROW_H + (ROW_H - BAR_H) / 2} width={g.w} height={BAR_H} rx={2} />;
        })}
      </svg>
      <div class="bars-values">
        {items.map((it, i) => (
          <div class={`bar-value ${it.value < 0 ? 'neg' : 'pos'}`} key={i}>
            {fmtValue(it.value, format)}
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * Positive-scale bars (profit factor, drawdown) with a per-bar colour class and
 * an optional reference line (e.g. PF = 1.0).
 */
export function ScaleBars({
  items,
  max,
  reference,
  format,
}: {
  items: { label: string; value: number; cls: string; text?: string }[];
  max: number;
  reference?: number;
  format?: (v: number) => string;
}) {
  const height = items.length * ROW_H;
  const refX = reference !== undefined ? positiveBar(reference, max, BAR_W) : null;
  return (
    <div class="bars">
      <div class="bars-labels">
        {items.map((it, i) => (
          <div class="bar-label" key={i} title={it.label}>
            {it.label}
          </div>
        ))}
      </div>
      <svg class="bars-svg" viewBox={`0 0 ${BAR_W} ${height}`} width="100%" height={height} preserveAspectRatio="none" role="img">
        {items.map((it, i) => (
          <rect key={i} class={`bar ${it.cls}`} x={0} y={i * ROW_H + (ROW_H - BAR_H) / 2} width={positiveBar(it.value, max, BAR_W)} height={BAR_H} rx={2} />
        ))}
        {refX !== null ? <line class="bar-ref" x1={refX} x2={refX} y1={0} y2={height} /> : null}
      </svg>
      <div class="bars-values">
        {items.map((it, i) => (
          <div class={`bar-value ${it.cls}`} key={i}>
            {it.text ?? (format ? format(it.value) : fmtValue(it.value))}
          </div>
        ))}
      </div>
    </div>
  );
}

const DONUT_R = 34;

export function Donut({ pct, label, sub }: { pct: number; label?: string; sub?: string }) {
  const { circ, dash } = donutDash(pct, DONUT_R);
  return (
    <div class="donut">
      <svg viewBox="0 0 100 100" width={96} height={96} role="img">
        <circle class="donut-track" cx={50} cy={50} r={DONUT_R} fill="none" stroke-width={12} />
        <circle class="donut-arc" cx={50} cy={50} r={DONUT_R} fill="none" stroke-width={12} stroke-dasharray={dash} stroke-dashoffset={circ / 4} stroke-linecap="butt" />
        <text class="donut-text" x={50} y={55} text-anchor="middle">
          {`${Math.round(pct)}%`}
        </text>
      </svg>
      <div class="donut-meta">
        {label ? <div class="donut-label">{label}</div> : null}
        {sub ? <div class="donut-sub">{sub}</div> : null}
      </div>
    </div>
  );
}

const HIST_H = 80;
const HIST_COL = 28;

export function Hist({ bins }: { bins: { label: string; count: number }[] }) {
  const max = Math.max(...bins.map((b) => b.count), 0);
  const width = bins.length * HIST_COL;
  return (
    <div class="hist">
      <svg viewBox={`0 0 ${width} ${HIST_H}`} width={width} height={HIST_H} role="img">
        {bins.map((b, i) => {
          const h = max > 0 ? (b.count / max) * (HIST_H - 4) : 0;
          return <rect key={i} class="hist-bar" x={i * HIST_COL + 3} y={HIST_H - h} width={HIST_COL - 6} height={h} rx={2} />;
        })}
      </svg>
      <div class="hist-labels">
        {bins.map((b, i) => (
          <div class="hist-label" key={i} title={`${b.label}: ${b.count}`}>
            {b.label}
          </div>
        ))}
      </div>
    </div>
  );
}

export function LevelsLadder({ items, last }: { items: { label: string; price: number; note?: string }[]; last?: number }) {
  const rows = levelsLadder(items, last);
  return (
    <div class="ladder">
      {rows.map((r, i) =>
        r.kind === 'now' ? (
          <div class="ladder-row now" key={i}>
            <span class="ladder-label">— {t('cards.priceNow')}</span>
            <span class="ladder-price">{fmtPrice(r.price)}</span>
            <span class="ladder-dist" />
          </div>
        ) : (
          <div class={`ladder-row ${r.side}`} key={i} title={r.note ?? ''}>
            <span class="ladder-label">
              {r.label}
              {r.note ? <small class="muted"> · {r.note}</small> : null}
            </span>
            <span class="ladder-price">{fmtPrice(r.price)}</span>
            <span class="ladder-dist">{r.distance === null ? '' : fmtDistance(r.distance)}</span>
          </div>
        ),
      )}
    </div>
  );
}
