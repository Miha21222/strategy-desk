/**
 * Day-block bootstrap and trade-level Monte Carlo.
 *
 * Day-block: resample whole trading days with replacement to the original day count. Trades inside a
 * day keep their order, so intraday clustering (revenge, streaks) survives the resample.
 */
import type { DayBlock } from './trades';
import { mulberry32, percentile, sortAsc } from './rng';
import type { Q5 } from './types';

export const DEFAULT_PATHS = 2000;
export const SEED_BASE = 4242;

export interface BootstrapOptions {
  paths?: number;
  seed?: number;
}

export interface BootstrapResult {
  paths: number;
  seed: number;
  /** Final equity quantiles. */
  final: Q5;
  /** Max drawdown quantiles (peak-to-trough on cumulative equity, ≥ 0). */
  maxDD: { p50: number; p95: number };
  /** Worst single day quantiles (≤ 0). */
  worstDay: { p50: number; p95: number };
  /** Share (0–100) of paths whose max drawdown reached `threshold`. */
  ruinProb: (threshold: number) => number;
  /** Share (0–100) of paths that ended below zero. */
  lossShare: number;
}

export function defaultSeed(nDays: number): number {
  return SEED_BASE + nDays;
}

function summarize(finals: number[], dds: number[], worst: number[], seed: number): BootstrapResult {
  const fs = sortAsc(finals);
  const ds = sortAsc(dds);
  const ws = sortAsc(worst);
  const paths = finals.length;
  let losers = 0;
  for (const f of finals) if (f < 0) losers++;
  return {
    paths,
    seed,
    final: {
      p5: percentile(fs, 5),
      p25: percentile(fs, 25),
      p50: percentile(fs, 50),
      p75: percentile(fs, 75),
      p95: percentile(fs, 95),
    },
    maxDD: { p50: percentile(ds, 50), p95: percentile(ds, 95) },
    worstDay: { p50: percentile(ws, 50), p95: percentile(ws, 5) },
    ruinProb: (threshold: number) => {
      if (!paths) return 0;
      let k = 0;
      for (const d of ds) if (d >= threshold) k++;
      return (k / paths) * 100;
    },
    lossShare: paths ? (losers / paths) * 100 : 0,
  };
}

/** Day-block bootstrap. Accepts DayBlock[] or an array of per-day P&L arrays. */
export function bootstrapDays(days: readonly DayBlock[] | readonly (readonly number[])[], opts: BootstrapOptions = {}): BootstrapResult {
  const dayPnls: number[][] = days.map((d) => (Array.isArray(d) ? [...d] : (d as DayBlock).trades.map((t) => t.pnl)));
  const nDays = dayPnls.length;
  const paths = opts.paths ?? DEFAULT_PATHS;
  const seed = opts.seed ?? defaultSeed(nDays);
  const rng = mulberry32(seed);
  const finals: number[] = [];
  const dds: number[] = [];
  const worst: number[] = [];
  if (nDays === 0) return summarize([0], [0], [0], seed);
  for (let p = 0; p < paths; p++) {
    let eq = 0;
    let peak = 0;
    let dd = 0;
    let w = 0;
    for (let k = 0; k < nDays; k++) {
      const day = dayPnls[Math.floor(rng() * nDays)];
      let dayNet = 0;
      for (const x of day) {
        eq += x;
        dayNet += x;
        if (eq > peak) peak = eq;
        else if (peak - eq > dd) dd = peak - eq;
      }
      if (dayNet < w) w = dayNet;
    }
    finals.push(eq);
    dds.push(dd);
    worst.push(w);
  }
  return summarize(finals, dds, worst, seed);
}

/** Trade-level Monte Carlo: resample n trades with replacement (ignores day structure). */
export function monteCarloTrades(pnls: readonly number[], paths = DEFAULT_PATHS, seed?: number): BootstrapResult {
  const n = pnls.length;
  const s = seed ?? SEED_BASE + n * 7;
  const rng = mulberry32(s);
  const finals: number[] = [];
  const dds: number[] = [];
  const worst: number[] = [];
  if (n === 0) return summarize([0], [0], [0], s);
  for (let p = 0; p < paths; p++) {
    let eq = 0;
    let peak = 0;
    let dd = 0;
    let w = 0;
    for (let k = 0; k < n; k++) {
      const x = pnls[Math.floor(rng() * n)];
      eq += x;
      if (eq > peak) peak = eq;
      else if (peak - eq > dd) dd = peak - eq;
      if (x < w) w = x;
    }
    finals.push(eq);
    dds.push(dd);
    worst.push(w);
  }
  return summarize(finals, dds, worst, s);
}
