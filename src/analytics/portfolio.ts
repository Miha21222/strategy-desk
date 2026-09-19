/**
 * Multi-strategy tools: scoreboard, portfolio survival, health check, account survival, overnight gap.
 * Formulas: docs/analytics.md ("Портфель").
 */
import { bootstrapDays, monteCarloTrades } from './bootstrap';
import { qualityScore, trueCost } from './quality';
import { makeDayPaths, microMultiplier, runSurvival, type SurvivalResult } from './propsim';
import type { AccountRef } from './rules';
import { basicStats, groupDays, normalizeTrades, type DayBlock, type Trade } from './trades';
import { mean, median, mulberry32, pearson, percentile, round, sortAsc } from './rng';
import type { DailyBar, Prodata } from './types';

// ---------------------------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------------------------

export const SCOREBOARD_MIN_TRADES = 10;
export const SCOREBOARD_MC_PATHS = 3000;
export const CORR_MIN_SHARED_DAYS = 15;
export const CORR_FLAG = 0.7;
export const PORTFOLIO_SIZES: readonly number[] = [1, 2, 5, 10];
export const DIVERSIFYING_MIN = 25;
export const HEALTH_PATHS = 4000;
export const HEALTH_MIN_RECENT = 5;
export const HEALTH_MIN_BASELINE = 20;
export const HEALTH_INVESTIGATE = 5;
export const HEALTH_WATCH = 20;
export const HEALTH_HOT = 95;
export const ACCOUNT_RISK_LADDER: readonly number[] = [0.5, 1, 2];
export const ACCOUNT_PATHS = 2000;
export const ACCOUNT_HORIZON = 250;
export const RUIN_LEVELS = [10, 20] as const;
export const GAP_MIN_BARS = 30;

// ---------------------------------------------------------------------------------------------
// Scoreboard
// ---------------------------------------------------------------------------------------------

export interface StrategyInput {
  name: string;
  prodata: Prodata;
}

export interface ScoreboardRow {
  name: string;
  n: number;
  days: number;
  score: number;
  letter: string;
  profitFactor: number;
  expectancy: number;
  net: number;
  ddP95: number;
  keepPct: number;
}

export interface CorrelationRow {
  a: string;
  b: string;
  rho: number;
  sharedDays: number;
  flag: boolean;
}

export interface Scoreboard {
  rows: ScoreboardRow[];
  correlations: CorrelationRow[];
  skipped: { name: string; reason: string }[];
}

interface Prepared {
  name: string;
  trades: Trade[];
  days: DayBlock[];
  unit: Prodata['backtestUnit'];
}

function prepare(s: StrategyInput): Prepared | { name: string; reason: string } {
  try {
    const trades = normalizeTrades(s.prodata);
    return { name: s.name, trades, days: groupDays(trades), unit: s.prodata.backtestUnit };
  } catch (e) {
    return { name: s.name, reason: e instanceof Error ? e.message : String(e) };
  }
}

function isPrepared(x: Prepared | { name: string; reason: string }): x is Prepared {
  return 'trades' in x;
}

export function dailyPnlMap(days: readonly DayBlock[]): Map<string, number> {
  const m = new Map<string, number>();
  for (const d of days) m.set(d.d, d.net);
  return m;
}

export function scoreboard(strategies: readonly StrategyInput[]): Scoreboard {
  const prepared = strategies.map(prepare);
  const ok = prepared.filter(isPrepared).filter((p) => p.trades.length >= SCOREBOARD_MIN_TRADES);
  const skipped = prepared.filter((p) => !isPrepared(p)) as { name: string; reason: string }[];
  for (const p of prepared) if (isPrepared(p) && p.trades.length < SCOREBOARD_MIN_TRADES) skipped.push({ name: p.name, reason: `fewer than ${SCOREBOARD_MIN_TRADES} trades` });
  const rows: ScoreboardRow[] = ok.map((p) => {
    const pnls = p.trades.map((t) => t.pnl);
    const mc = monteCarloTrades(pnls, SCOREBOARD_MC_PATHS);
    const q = qualityScore(pnls, mc, { tradingDays: p.days.length, noCaps: true });
    const st = basicStats(pnls);
    return {
      name: p.name,
      n: pnls.length,
      days: p.days.length,
      score: q.score,
      letter: q.letter,
      profitFactor: round(st.profitFactor, 2),
      expectancy: round(st.expectancy, 2),
      net: round(st.net, 2),
      ddP95: round(mc.maxDD.p95, 2),
      keepPct: trueCost(pnls, p.unit).keepPct,
    };
  });
  rows.sort((a, b) => b.score - a.score);
  const correlations: CorrelationRow[] = [];
  for (let i = 0; i < ok.length; i++) {
    for (let j = i + 1; j < ok.length; j++) {
      const ma = dailyPnlMap(ok[i].days);
      const mb = dailyPnlMap(ok[j].days);
      const xa: number[] = [];
      const xb: number[] = [];
      for (const [d, v] of ma) {
        const w = mb.get(d);
        if (w != null) {
          xa.push(v);
          xb.push(w);
        }
      }
      if (xa.length < CORR_MIN_SHARED_DAYS) continue;
      const rho = pearson(xa, xb);
      correlations.push({ a: ok[i].name, b: ok[j].name, rho: round(rho, 2), sharedDays: xa.length, flag: Math.abs(rho) > CORR_FLAG });
    }
  }
  return { rows, correlations, skipped };
}

// ---------------------------------------------------------------------------------------------
// Portfolio survival + diversification
// ---------------------------------------------------------------------------------------------

export interface PortfolioSurvival {
  window: { from: string; to: string } | null;
  strategies: string[];
  days: number;
  survival: SurvivalResult | null;
  diversification: number;
  diversifying: boolean;
  combinedP95: number;
  separateP95: number[];
}

/** Scale every trade to per-micro P&L and clip to the shared date window; returns merged day blocks. */
export function mergePortfolio(strategies: readonly StrategyInput[]): { merged: DayBlock[]; parts: DayBlock[][]; window: { from: string; to: string } | null; names: string[] } {
  const prepared = strategies.map(prepare).filter(isPrepared);
  if (!prepared.length) return { merged: [], parts: [], window: null, names: [] };
  let from = '';
  let to = '￿';
  for (const p of prepared) {
    const f = p.days[0].d;
    const l = p.days[p.days.length - 1].d;
    if (f > from) from = f;
    if (l < to) to = l;
  }
  if (from > to) return { merged: [], parts: [], window: null, names: prepared.map((p) => p.name) };
  const parts: DayBlock[][] = [];
  const byDate = new Map<string, Trade[]>();
  for (const p of prepared) {
    const mult = microMultiplier(p.unit);
    const scaled: Trade[] = p.trades.filter((t) => t.d >= from && t.d <= to).map((t) => ({ ...t, pnl: t.pnl * mult }));
    parts.push(groupDays(scaled));
    for (const t of scaled) {
      let arr = byDate.get(t.d);
      if (!arr) {
        arr = [];
        byDate.set(t.d, arr);
      }
      arr.push(t);
    }
  }
  const merged: DayBlock[] = [...byDate.entries()]
    .sort(([a], [b]) => (a < b ? -1 : 1))
    .map(([d, trades]) => {
      const sorted = [...trades].sort((a, b) => (a.minute ?? -1) - (b.minute ?? -1));
      return { d, trades: sorted, net: sorted.reduce((s, t) => s + t.pnl, 0) };
    });
  return { merged, parts, window: { from, to }, names: prepared.map((p) => p.name) };
}

export function portfolioSurvival(strategies: readonly StrategyInput[], ref: AccountRef | null, opts: { paths?: number; seed?: number } = {}): PortfolioSurvival {
  const { merged, parts, window, names } = mergePortfolio(strategies);
  if (!merged.length) return { window, strategies: names, days: 0, survival: null, diversification: 0, diversifying: false, combinedP95: 0, separateP95: [] };
  const combined = bootstrapDays(merged, { paths: opts.paths, seed: opts.seed }).maxDD.p95;
  const separate = parts.map((p) => bootstrapDays(p, { paths: opts.paths, seed: opts.seed }).maxDD.p95);
  const sumSep = separate.reduce((a, b) => a + b, 0);
  const diversification = sumSep > 0 ? Math.max(0, (1 - combined / sumSep) * 100) : 0;
  const survival = ref ? runSurvival(merged, ref, { paths: opts.paths, seed: opts.seed, unit: 'micro', sizes: PORTFOLIO_SIZES }) : null;
  return {
    window,
    strategies: names,
    days: merged.length,
    survival,
    diversification: round(diversification),
    diversifying: diversification >= DIVERSIFYING_MIN,
    combinedP95: round(combined, 2),
    separateP95: separate.map((x) => round(x, 2)),
  };
}

// ---------------------------------------------------------------------------------------------
// Health check
// ---------------------------------------------------------------------------------------------

export type HealthVerdict = 'investigate' | 'watch' | 'normal' | 'hot' | 'insufficient';

export interface HealthCheck {
  verdict: HealthVerdict;
  percentile: number | null;
  actual: number;
  expected: number;
  k: number;
  baselineN: number;
  band: { p5: number; p50: number; p95: number } | null;
}

export function healthCheck(baselinePnls: readonly number[], recentPnls: readonly number[], opts: { paths?: number; seed?: number } = {}): HealthCheck {
  const k = recentPnls.length;
  const actual = recentPnls.reduce((a, b) => a + b, 0);
  if (k < HEALTH_MIN_RECENT || baselinePnls.length < HEALTH_MIN_BASELINE) {
    return { verdict: 'insufficient', percentile: null, actual: round(actual, 2), expected: round(mean(baselinePnls) * k, 2), k, baselineN: baselinePnls.length, band: null };
  }
  const paths = opts.paths ?? HEALTH_PATHS;
  const rng = mulberry32(opts.seed ?? 4242 + baselinePnls.length * 13 + k);
  const sums: number[] = [];
  const n = baselinePnls.length;
  let below = 0;
  for (let p = 0; p < paths; p++) {
    let s = 0;
    for (let i = 0; i < k; i++) s += baselinePnls[Math.floor(rng() * n)];
    sums.push(s);
    if (s <= actual) below++;
  }
  const pct = (below / paths) * 100;
  const sorted = sortAsc(sums);
  let verdict: HealthVerdict = 'normal';
  if (pct < HEALTH_INVESTIGATE) verdict = 'investigate';
  else if (pct < HEALTH_WATCH) verdict = 'watch';
  else if (pct > HEALTH_HOT) verdict = 'hot';
  return {
    verdict,
    percentile: round(pct),
    actual: round(actual, 2),
    expected: round(mean(baselinePnls) * k, 2),
    k,
    baselineN: n,
    band: { p5: round(percentile(sorted, 5), 2), p50: round(percentile(sorted, 50), 2), p95: round(percentile(sorted, 95), 2) },
  };
}

// ---------------------------------------------------------------------------------------------
// Account survival (own capital)
// ---------------------------------------------------------------------------------------------

export interface AccountSurvivalRow {
  riskPct: number;
  contracts: number;
  ruin10Pct: number;
  ruin20Pct: number;
  medianReturnPct: number;
}

export interface AccountSurvival {
  accountSize: number;
  avgLossPerMicro: number;
  rows: AccountSurvivalRow[];
}

export function accountSurvival(
  days: readonly DayBlock[],
  accountSize: number,
  unit: Prodata['backtestUnit'],
  opts: { paths?: number; seed?: number; risks?: readonly number[] } = {},
): AccountSurvival {
  const mult = microMultiplier(unit);
  const pnls: number[] = [];
  for (const d of days) for (const t of d.trades) pnls.push(t.pnl * mult);
  const st = basicStats(pnls);
  const avgLoss = st.avgLoss > 0 ? st.avgLoss : mean(pnls.map((x) => Math.abs(x))) || 1;
  const paths = opts.paths ?? ACCOUNT_PATHS;
  const dayPaths = makeDayPaths(days.length, paths, opts.seed, ACCOUNT_HORIZON);
  const rows: AccountSurvivalRow[] = [];
  for (const risk of opts.risks ?? ACCOUNT_RISK_LADDER) {
    const contracts = Math.floor(((accountSize * risk) / 100) / avgLoss);
    if (contracts < 1) {
      rows.push({ riskPct: risk, contracts: 0, ruin10Pct: 0, ruin20Pct: 0, medianReturnPct: 0 });
      continue;
    }
    let ruin10 = 0;
    let ruin20 = 0;
    const finals: number[] = [];
    for (const path of dayPaths) {
      let eq = 0;
      let peak = 0;
      let dd = 0;
      for (let k = 0; k < path.length; k++) {
        for (const t of days[path[k]].trades) {
          eq += t.pnl * mult * contracts;
          if (eq > peak) peak = eq;
          else if (peak - eq > dd) dd = peak - eq;
        }
      }
      if (dd >= (accountSize * RUIN_LEVELS[0]) / 100) ruin10++;
      if (dd >= (accountSize * RUIN_LEVELS[1]) / 100) ruin20++;
      finals.push((eq / accountSize) * 100);
    }
    rows.push({
      riskPct: risk,
      contracts,
      ruin10Pct: round((ruin10 / dayPaths.length) * 100),
      ruin20Pct: round((ruin20 / dayPaths.length) * 100),
      medianReturnPct: round(median(finals)),
    });
  }
  return { accountSize, avgLossPerMicro: round(avgLoss, 2), rows };
}

// ---------------------------------------------------------------------------------------------
// Overnight gap
// ---------------------------------------------------------------------------------------------

export interface OvernightGap {
  bars: number;
  gapPoints: { p50: number; p95: number; max: number };
  gapDollars: { p50: number; p95: number; max: number };
  overnight: { n: number; net: number };
  intraday: { n: number; net: number };
}

export function overnightGap(daily: readonly DailyBar[] | null | undefined, pointValue: number, trades: readonly Trade[]): OvernightGap | null {
  if (!daily || daily.length < GAP_MIN_BARS) return null;
  const sorted = [...daily].sort((a, b) => (a.d < b.d ? -1 : a.d > b.d ? 1 : 0));
  const gaps: number[] = [];
  for (let i = 1; i < sorted.length; i++) gaps.push(Math.abs(sorted[i].o - sorted[i - 1].c));
  const gs = sortAsc(gaps);
  const p50 = percentile(gs, 50);
  const p95 = percentile(gs, 95);
  const max = gs[gs.length - 1];
  const over = trades.filter((t) => t.ed != null && t.ed < t.d);
  const intra = trades.filter((t) => !(t.ed != null && t.ed < t.d));
  return {
    bars: sorted.length,
    gapPoints: { p50: round(p50, 2), p95: round(p95, 2), max: round(max, 2) },
    gapDollars: { p50: round(p50 * pointValue, 2), p95: round(p95 * pointValue, 2), max: round(max * pointValue, 2) },
    overnight: { n: over.length, net: round(over.reduce((a, t) => a + t.pnl, 0), 2) },
    intraday: { n: intra.length, net: round(intra.reduce((a, t) => a + t.pnl, 0), 2) },
  };
}
