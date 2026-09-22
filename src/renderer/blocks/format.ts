// Pure helpers for the cards: number formatting, colour classes and the
// levels ladder. No DOM, so they are unit-tested in vitest.
import type { ChartFormat, Grade, StressReport, StressRun } from '@shared/blocks';

const nf0 = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 0 });
const nf2 = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 2 });

/** Value with unit for bars/tiles. Negative numbers keep a leading minus. */
export function fmtValue(v: number, format: ChartFormat = 'number'): string {
  if (!Number.isFinite(v)) return '—';
  switch (format) {
    case 'money': {
      const abs = Math.abs(v);
      const body = abs >= 100 ? nf0.format(abs) : nf2.format(abs);
      return `${v < 0 ? '-' : ''}$${body}`;
    }
    case 'percent':
      return `${nf2.format(v)}%`;
    case 'points':
      return `${nf2.format(v)} pt`;
    default:
      return Math.abs(v) >= 1000 ? nf0.format(v) : nf2.format(v);
  }
}

export function fmtMoney(v: number): string {
  return fmtValue(v, 'money');
}

export function fmtPrice(v: number): string {
  return nf2.format(v);
}

/** Stat tiles: a leading `-` or `(` marks a negative value. */
export function isNegativeText(value: string | number): boolean {
  const s = String(value).trim();
  return s.startsWith('-') || s.startsWith('(') || s.startsWith('−');
}

export function gradeClass(grade: Grade): string {
  return `grade-${grade}`;
}

/** Profit factor colour: ≥1.2 good, ≥1 warn, else bad. */
export function pfClass(pf: number): 'good' | 'warn' | 'bad' {
  return pf >= 1.2 ? 'good' : pf >= 1 ? 'warn' : 'bad';
}

/** Drawdown colour: ≤10% good, ≤20% warn, else bad. */
export function ddClass(dd: number): 'good' | 'warn' | 'bad' {
  return dd <= 10 ? 'good' : dd <= 20 ? 'warn' : 'bad';
}

export function signClass(v: number): 'pos' | 'neg' | 'zero' {
  return v > 0 ? 'pos' : v < 0 ? 'neg' : 'zero';
}

// ---------- levels ladder ----------

export type LadderRow =
  | { kind: 'level'; label: string; price: number; note?: string; distance: number | null; side: 'above' | 'below' | 'at' }
  | { kind: 'now'; price: number };

/** Two prices count as the same level below this relative difference. */
const SAME_PRICE_REL = 1e-6;

export function samePrice(a: number, b: number): boolean {
  return Math.abs(a - b) <= Math.max(Math.abs(b), 1) * SAME_PRICE_REL;
}

/**
 * Levels sorted top-down with the current price inserted at its place.
 *
 * A level sitting exactly at the current price is dropped: models like to
 * repeat the last price as an item on top of passing it in `last`, which drew
 * the same number twice — once as a colourless level with distance 0, once as
 * the highlighted "price now" row. The "price now" row stands for that price.
 */
export function levelsLadder(items: { label: string; price: number; note?: string }[], last: number | undefined): LadderRow[] {
  const own = last === undefined ? items : items.filter((it) => !samePrice(it.price, last));
  const sorted = [...own].sort((a, b) => b.price - a.price);
  const rows: LadderRow[] = [];
  let nowInserted = last === undefined;
  for (const it of sorted) {
    if (!nowInserted && last !== undefined && it.price < last) {
      rows.push({ kind: 'now', price: last });
      nowInserted = true;
    }
    const distance = last === undefined ? null : it.price - last;
    const side = distance === null || distance === 0 ? 'at' : distance > 0 ? 'above' : 'below';
    rows.push({ kind: 'level', label: it.label, price: it.price, note: it.note, distance, side });
  }
  if (!nowInserted && last !== undefined) rows.push({ kind: 'now', price: last });
  return rows;
}

export function fmtDistance(d: number): string {
  const s = nf2.format(Math.abs(d));
  return d > 0 ? `+${s}` : d < 0 ? `-${s}` : '0';
}

// ---------- SVG geometry (attributes only; no inline styles) ----------

/** Donut arc: circumference-based dash values for a given percentage. */
export function donutDash(pct: number, r: number): { circ: number; dash: string } {
  const circ = 2 * Math.PI * r;
  const clamped = Math.max(0, Math.min(100, pct));
  const on = (circ * clamped) / 100;
  return { circ, dash: `${on.toFixed(2)} ${(circ - on).toFixed(2)}` };
}

/** Horizontal signed bar geometry inside a track of `width` px around a centre line. */
export function signedBar(value: number, maxAbs: number, width: number): { x: number; w: number; centre: number } {
  const centre = width / 2;
  if (maxAbs <= 0 || !Number.isFinite(value)) return { x: centre, w: 0, centre };
  const w = (Math.abs(value) / maxAbs) * (width / 2);
  return { x: value < 0 ? centre - w : centre, w, centre };
}

/** Bar geometry for a strictly positive scale (profit factor, drawdown). */
export function positiveBar(value: number, max: number, width: number): number {
  if (max <= 0 || value <= 0) return 0;
  return Math.min(width, (value / max) * width);
}

// ---------- stress report footer ----------

export function totals(runs: StressRun[]): { trades: number; net: number } {
  return runs.reduce((acc, r) => ({ trades: acc.trades + r.trades, net: acc.net + r.netProfit }), { trades: 0, net: 0 });
}

export function reportTitle(r: Pick<StressReport, 'strategy' | 'version'>): string {
  return `${r.strategy} ${r.version}`.trim();
}
