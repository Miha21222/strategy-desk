/**
 * Strategy quality score and diagnostic panels.
 * All formulas are documented in docs/analytics.md ("Качество стратегии").
 */
import type { BootstrapResult } from './bootstrap';
import { basicStats, PF_CAP, type Trade } from './trades';
import { clamp, linearFit, mean, median, round, sd } from './rng';
import type { BacktestUnit, DailyBar } from './types';

// ---------------------------------------------------------------------------------------------
// Constants (thresholds)
// ---------------------------------------------------------------------------------------------

export const SQN_SAMPLE_CAP = 100;
export const RISK_RATIO_FULL = 4; // net / maxDD.p95 = 4 → risk 100
export const TOP_SHARE_FREE = 15; // % of gross win the best trade may hold without penalty
export const TOP_SHARE_PENALTY_PER_PCT = 2;
export const DIES_TOP1_PENALTY = 45;
export const DIES_TOP3_PENALTY = 30;
export const DIES_TOP5_PENALTY = 20;
export const OOS_WEIGHT = 0.35;
export const RETENTION_FLOOR = 0.3;
export const WEIGHTS = { edge: 0.35, risk: 0.25, robust: 0.25, consist: 0.15 } as const;
export const SAMPLE_CAPS: readonly [number, number][] = [
  [20, 45],
  [30, 60],
  [80, 82],
];
export const TRADING_DAYS_CAP: [number, number] = [15, 70];
export const VARIANT_CAPS: readonly [number, number][] = [
  [8, 55],
  [5, 65],
  [3, 78],
  [2, 85],
];
export const LETTERS: readonly [number, string][] = [
  [90, 'A+'],
  [80, 'A'],
  [68, 'B'],
  [55, 'C'],
  [40, 'D'],
];
export const KILLZONE_MIN_TIMED = 20;
export const CONCENTRATION_MIN_N = 1;
export const REGIME_MIN_BARS = 40;
export const REGIME_WINDOW = 20;
export const REGIME_ER_TREND = 0.3;
export const REGIME_MIN_ROW = 10;
export const ROLLING_PROFITABLE_SHARE = 0.75;

// ---------------------------------------------------------------------------------------------
// SQN and edge
// ---------------------------------------------------------------------------------------------

/** R-multiples relative to the average losing trade: R_i = pnl_i / avgLoss. */
export function rMultiples(pnls: readonly number[]): { r: number[]; avgLoss: number } {
  const st = basicStats(pnls);
  // No losing trade at all: fall back to mean |pnl| so R stays finite (documented deviation).
  const avgLoss = st.avgLoss > 0 ? st.avgLoss : mean(pnls.map((p) => Math.abs(p))) || 1;
  return { r: pnls.map((p) => p / avgLoss), avgLoss };
}

/** SQN = sqrt(min(n, 100)) × mean(R) / sd(R). sd = 0 → ±10 by sign of the mean. */
export function sqn(r: readonly number[]): number {
  const n = r.length;
  if (n < 2) return 0;
  const m = mean(r);
  const s = sd(r);
  if (s === 0) return m > 0 ? 10 : m < 0 ? -10 : 0;
  return (Math.sqrt(Math.min(n, SQN_SAMPLE_CAP)) * m) / s;
}

/** Piecewise edge score (0–100) from SQN. */
export function edgeFromSqn(s: number): number {
  if (s <= 0) return Math.max(0, 20 + 10 * s);
  if (s < 1.6) return (s / 1.6) * 40;
  if (s < 2) return 40 + ((s - 1.6) / 0.4) * 15;
  if (s < 2.5) return 55 + ((s - 2) / 0.5) * 15;
  if (s < 3) return 70 + ((s - 2.5) / 0.5) * 12;
  if (s < 5) return 82 + ((s - 3) / 2) * 13;
  return Math.min(100, 95 + (s - 5));
}

export function letterFor(score: number): string {
  for (const [min, l] of LETTERS) if (score >= min) return l;
  return 'F';
}

// ---------------------------------------------------------------------------------------------
// Outliers
// ---------------------------------------------------------------------------------------------

export interface Outliers {
  net: number;
  netWithoutTop1: number;
  netWithoutTop3: number;
  netWithoutTop5: number;
  /** Best trade share of gross win, %. */
  topShare: number;
  topTrades: number[];
}

export function outliers(pnls: readonly number[]): Outliers {
  const st = basicStats(pnls);
  const wins = [...pnls].filter((p) => p > 0).sort((a, b) => b - a);
  const without = (k: number) => st.net - wins.slice(0, k).reduce((a, b) => a + b, 0);
  return {
    net: st.net,
    netWithoutTop1: without(1),
    netWithoutTop3: without(3),
    netWithoutTop5: without(5),
    topShare: st.grossWin > 0 ? (st.maxWin / st.grossWin) * 100 : 0,
    topTrades: wins.slice(0, 5),
  };
}

// ---------------------------------------------------------------------------------------------
// Quality score
// ---------------------------------------------------------------------------------------------

export interface QualityOptions {
  /** Number of parameter variants tried (selection luck). */
  variants?: number | null;
  /** In-sample / out-of-sample profit factors for the retention term. */
  oos?: { isPF: number; oosPF: number } | null;
  /** Number of distinct trading days (for the <15 days cap). */
  tradingDays?: number;
  /** Skip the sample-size and variants caps ("no shield", used by the portfolio scoreboard). */
  noCaps?: boolean;
}

export interface QualityResult {
  score: number;
  letter: string;
  edge: number;
  risk: number;
  robust: number;
  consist: number;
  sqn: number;
  n: number;
  tradingDays: number;
  net: number;
  rawScore: number;
  caps: { reason: string; cap: number }[];
  topShare: number;
  retention: number | null;
}

export function qualityScore(pnls: readonly number[], mc: BootstrapResult, opts: QualityOptions = {}): QualityResult {
  const n = pnls.length;
  const st = basicStats(pnls);
  const { r } = rMultiples(pnls);
  const s = sqn(r);
  const edge = edgeFromSqn(s);

  const ddP95 = mc.maxDD.p95;
  const risk = ddP95 > 0 ? clamp((st.net / ddP95 / RISK_RATIO_FULL) * 100, 0, 100) : st.net > 0 ? 100 : 0;

  const out = outliers(pnls);
  const topShare = out.topShare;
  let robust = 100 - Math.max(0, topShare - TOP_SHARE_FREE) * TOP_SHARE_PENALTY_PER_PCT;
  if (out.netWithoutTop1 <= 0) robust -= DIES_TOP1_PENALTY;
  else if (out.netWithoutTop3 <= 0) robust -= DIES_TOP3_PENALTY;
  else if (out.netWithoutTop5 <= 0) robust -= DIES_TOP5_PENALTY;
  robust = clamp(robust, 0, 100);
  let retention: number | null = null;
  if (opts.oos && opts.oos.isPF > 0) {
    retention = opts.oos.oosPF / opts.oos.isPF;
    const oosScore = clamp(((retention - RETENTION_FLOOR) / (1 - RETENTION_FLOOR)) * 100, 0, 100);
    robust = (1 - OOS_WEIGHT) * robust + OOS_WEIGHT * oosScore;
  }

  let consist = 0;
  if (st.net > 0) {
    const cum: number[] = [];
    let e = 0;
    for (const p of pnls) {
      e += p;
      cum.push(e);
    }
    consist = linearFit(cum).r2 * 100;
  }

  const raw = WEIGHTS.edge * edge + WEIGHTS.risk * risk + WEIGHTS.robust * robust + WEIGHTS.consist * consist;
  let score = raw;
  const caps: { reason: string; cap: number }[] = [];
  const tradingDays = opts.tradingDays ?? n;
  if (!opts.noCaps) {
    for (const [minN, cap] of SAMPLE_CAPS) {
      if (n < minN) {
        caps.push({ reason: `n<${minN}`, cap });
        break;
      }
    }
    if (tradingDays < TRADING_DAYS_CAP[0]) caps.push({ reason: `tradingDays<${TRADING_DAYS_CAP[0]}`, cap: TRADING_DAYS_CAP[1] });
    const v = opts.variants ?? 0;
    for (const [minV, cap] of VARIANT_CAPS) {
      if (v >= minV) {
        caps.push({ reason: `variants>=${minV}`, cap });
        break;
      }
    }
    for (const c of caps) score = Math.min(score, c.cap);
  }
  score = clamp(score, 0, 100);
  return {
    score: round(score),
    letter: letterFor(score),
    edge: round(edge),
    risk: round(risk),
    robust: round(robust),
    consist: round(consist),
    sqn: round(s, 2),
    n,
    tradingDays,
    net: round(st.net, 2),
    rawScore: round(raw),
    caps,
    topShare: round(topShare),
    retention: retention == null ? null : round(retention, 2),
  };
}

// ---------------------------------------------------------------------------------------------
// Rolling windows
// ---------------------------------------------------------------------------------------------

export interface RollingSegment {
  index: number;
  from: string;
  to: string;
  n: number;
  net: number;
  profitFactor: number;
  sqn: number;
}

export interface RollingWindows {
  segments: RollingSegment[];
  profitableSegs: number;
  netWithoutBestSeg: number;
  travels: boolean;
  oneLuckyStretch: boolean;
}

export function segmentCount(n: number): number {
  return n >= 120 ? 6 : n >= 60 ? 5 : 4;
}

export function rollingWindows(trades: readonly Trade[]): RollingWindows {
  const n = trades.length;
  const segs = segmentCount(n);
  const { r } = rMultiples(trades.map((t) => t.pnl));
  const segments: RollingSegment[] = [];
  for (let k = 0; k < segs; k++) {
    const a = Math.floor((k * n) / segs);
    const b = Math.floor(((k + 1) * n) / segs);
    const slice = trades.slice(a, b);
    const pn = slice.map((t) => t.pnl);
    const st = basicStats(pn);
    segments.push({
      index: k + 1,
      from: slice[0]?.d ?? '',
      to: slice[slice.length - 1]?.d ?? '',
      n: slice.length,
      net: round(st.net, 2),
      profitFactor: round(Math.min(PF_CAP, st.profitFactor), 2),
      sqn: round(sqn(r.slice(a, b)), 2),
    });
  }
  const profitable = segments.filter((s) => s.net > 0).length;
  const total = segments.reduce((a, s) => a + s.net, 0);
  const best = Math.max(...segments.map((s) => s.net));
  const netWithoutBest = total - best;
  return {
    segments,
    profitableSegs: profitable,
    netWithoutBestSeg: round(netWithoutBest, 2),
    travels: profitable >= Math.ceil(ROLLING_PROFITABLE_SHARE * segs) && netWithoutBest > 0,
    oneLuckyStretch: netWithoutBest <= 0 || profitable <= Math.floor(segs / 2),
  };
}

// ---------------------------------------------------------------------------------------------
// Concentration (hour / weekday) and killzone
// ---------------------------------------------------------------------------------------------

export interface BucketRow {
  key: string;
  n: number;
  net: number;
  winRate: number;
}

export interface Concentration {
  byHour: BucketRow[];
  byWeekday: BucketRow[];
  timedShare: number;
}

const DOW_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

function bucketize(trades: readonly Trade[], keyOf: (t: Trade) => string | null): BucketRow[] {
  const map = new Map<string, { n: number; net: number; wins: number }>();
  for (const t of trades) {
    const k = keyOf(t);
    if (k == null) continue;
    let b = map.get(k);
    if (!b) {
      b = { n: 0, net: 0, wins: 0 };
      map.set(k, b);
    }
    b.n++;
    b.net += t.pnl;
    if (t.pnl > 0) b.wins++;
  }
  return [...map.entries()]
    .filter(([, b]) => b.n >= CONCENTRATION_MIN_N)
    .map(([key, b]) => ({ key, n: b.n, net: round(b.net, 2), winRate: round((b.wins / b.n) * 100) }))
    .sort((a, b) => (a.key < b.key ? -1 : 1));
}

export function concentration(trades: readonly Trade[]): Concentration {
  const timed = trades.filter((t) => t.hour != null).length;
  const byHour = bucketize(trades, (t) => (t.hour == null ? null : String(t.hour).padStart(2, '0') + ':00'));
  const byWeekday = bucketize(trades, (t) => DOW_NAMES[t.dow]).sort((a, b) => DOW_NAMES.indexOf(a.key) - DOW_NAMES.indexOf(b.key));
  return { byHour, byWeekday, timedShare: trades.length ? round((timed / trades.length) * 100) : 0 };
}

export interface KillzoneCell {
  cell: number;
  dow: number;
  hour: number;
  n: number;
  net: number;
  winRate: number;
}

export interface Killzone {
  cells: KillzoneCell[];
  best: KillzoneCell | null;
  worst: KillzoneCell | null;
  timed: number;
}

export function killzone(trades: readonly Trade[]): Killzone | null {
  const timed = trades.filter((t) => t.hour != null);
  if (timed.length < KILLZONE_MIN_TIMED) return null;
  const map = new Map<number, { n: number; net: number; wins: number }>();
  for (const t of timed) {
    const cell = t.dow * 24 + (t.hour as number);
    let b = map.get(cell);
    if (!b) {
      b = { n: 0, net: 0, wins: 0 };
      map.set(cell, b);
    }
    b.n++;
    b.net += t.pnl;
    if (t.pnl > 0) b.wins++;
  }
  const cells: KillzoneCell[] = [...map.entries()]
    .map(([cell, b]) => ({
      cell,
      dow: Math.floor(cell / 24),
      hour: cell % 24,
      n: b.n,
      net: round(b.net, 2),
      winRate: round((b.wins / b.n) * 100),
    }))
    .sort((a, b) => a.cell - b.cell);
  const eligible = cells.filter((c) => c.n >= 3);
  let best: KillzoneCell | null = null;
  let worst: KillzoneCell | null = null;
  for (const c of eligible) {
    if (!best || c.net > best.net) best = c;
    if (!worst || c.net < worst.net) worst = c;
  }
  return { cells, best, worst, timed: timed.length };
}

// ---------------------------------------------------------------------------------------------
// True cost
// ---------------------------------------------------------------------------------------------

export interface FuturesCost {
  commissionRT: number;
  slipTicks: number;
  tickValue: number;
}
export interface ForexCost {
  commissionRT: number;
  spreadPips: number;
  pipValue: number;
}
export const COST_MINI: FuturesCost = { commissionRT: 4, slipTicks: 1, tickValue: 5 };
export const COST_MICRO: FuturesCost = { commissionRT: 1.5, slipTicks: 1, tickValue: 0.5 };
export const COST_FOREX_LOT: ForexCost = { commissionRT: 7, spreadPips: 1, pipValue: 10 };

export function costPerTrade(c: FuturesCost | ForexCost): number {
  if ('slipTicks' in c) return c.commissionRT + 2 * c.slipTicks * c.tickValue;
  return c.commissionRT + c.spreadPips * c.pipValue;
}

export interface TrueCost {
  unit: BacktestUnit;
  costPerTrade: number;
  totalCost: number;
  net: number;
  afterNet: number;
  keepPct: number;
  /** Micros at which the total cost equals one mini's cost (futures only). */
  crossoverMicros: number | null;
}

export function trueCost(
  pnls: readonly number[],
  unit: BacktestUnit,
  overrides: Partial<FuturesCost & ForexCost> = {},
): TrueCost {
  const n = pnls.length;
  const net = pnls.reduce((a, b) => a + b, 0);
  let cost: number;
  let crossover: number | null;
  if (unit === 'lot') {
    cost = costPerTrade({ ...COST_FOREX_LOT, ...overrides });
    crossover = null;
  } else {
    const mini = costPerTrade({ ...COST_MINI, ...overrides });
    const micro = costPerTrade(unit === 'micro' ? { ...COST_MICRO, ...overrides } : COST_MICRO);
    cost = unit === 'micro' ? micro : mini;
    crossover = micro > 0 ? Math.ceil(mini / micro) : null;
  }
  const total = cost * n;
  const after = net - total;
  return {
    unit,
    costPerTrade: cost,
    totalCost: round(total, 2),
    net: round(net, 2),
    afterNet: round(after, 2),
    keepPct: net > 0 ? round((after / net) * 100) : 0,
    crossoverMicros: crossover,
  };
}

// ---------------------------------------------------------------------------------------------
// Regime
// ---------------------------------------------------------------------------------------------

export type RegimeLabel = 'volatile trend' | 'volatile chop' | 'quiet trend' | 'quiet chop';

export interface RegimeRow {
  label: RegimeLabel;
  n: number;
  net: number;
  winRate: number;
  profitFactor: number;
  days: number;
}

export interface Regime {
  rows: RegimeRow[];
  labelsByDate: Record<string, RegimeLabel>;
  medianVol: number;
}

/** Label each daily bar (from index REGIME_WINDOW on) by volatility and efficiency ratio. */
export function labelRegimes(daily: readonly DailyBar[]): { labels: Map<string, RegimeLabel>; medianVol: number } {
  const w = REGIME_WINDOW;
  const closes = daily.map((b) => b.c);
  const logRet: number[] = [];
  for (let i = 1; i < closes.length; i++) logRet.push(Math.log(closes[i] / closes[i - 1]));
  const vol: (number | null)[] = [];
  const er: (number | null)[] = [];
  for (let i = 0; i < closes.length; i++) {
    if (i < w) {
      vol.push(null);
      er.push(null);
      continue;
    }
    vol.push(sd(logRet.slice(i - w, i)));
    let path = 0;
    for (let j = i - w + 1; j <= i; j++) path += Math.abs(closes[j] - closes[j - 1]);
    er.push(path > 0 ? Math.abs(closes[i] - closes[i - w]) / path : 0);
  }
  const vols = vol.filter((v): v is number => v != null);
  const medianVol = vols.length ? median(vols) : 0;
  const labels = new Map<string, RegimeLabel>();
  for (let i = 0; i < daily.length; i++) {
    const v = vol[i];
    const e = er[i];
    if (v == null || e == null) continue;
    const a = v > medianVol ? 'volatile' : 'quiet';
    const b = e >= REGIME_ER_TREND ? 'trend' : 'chop';
    labels.set(daily[i].d, `${a} ${b}` as RegimeLabel);
  }
  return { labels, medianVol };
}

export function regime(trades: readonly Trade[], daily: readonly DailyBar[] | null | undefined): Regime | null {
  if (!daily || daily.length < REGIME_MIN_BARS) return null;
  const sorted = [...daily].sort((a, b) => (a.d < b.d ? -1 : a.d > b.d ? 1 : 0));
  const { labels, medianVol } = labelRegimes(sorted);
  const groups = new Map<RegimeLabel, { pn: number[]; days: Set<string> }>();
  for (const t of trades) {
    const l = labels.get(t.d);
    if (!l) continue;
    let g = groups.get(l);
    if (!g) {
      g = { pn: [], days: new Set() };
      groups.set(l, g);
    }
    g.pn.push(t.pnl);
    g.days.add(t.d);
  }
  const rows: RegimeRow[] = [];
  for (const [label, g] of groups) {
    if (g.pn.length < REGIME_MIN_ROW) continue;
    const st = basicStats(g.pn);
    rows.push({
      label,
      n: st.n,
      net: round(st.net, 2),
      winRate: round(st.winRate * 100),
      profitFactor: round(st.profitFactor, 2),
      days: g.days.size,
    });
  }
  rows.sort((a, b) => b.net - a.net);
  const labelsByDate: Record<string, RegimeLabel> = {};
  for (const [d, l] of labels) labelsByDate[d] = l;
  return { rows, labelsByDate, medianVol };
}
