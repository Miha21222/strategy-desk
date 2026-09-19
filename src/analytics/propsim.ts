/**
 * Prop-firm evaluation simulator (futures + forex families) driven by day-block bootstrap paths.
 * Formulas: docs/analytics.md ("Симулятор пропа").
 */
import type { DayBlock } from './trades';
import { mulberry32, percentile, round, sortAsc } from './rng';
import { defaultSeed } from './bootstrap';
import { isForexAccount, type Account, type AccountRef, type CostRule, type DrawdownRule, type ForexAccount, type FuturesAccount } from './rules';
import type { BacktestUnit, Q3 } from './types';

// ---------------------------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------------------------

export const SIM_PATHS = 2000;
export const RUNWAY_DAYS = 60;
export const PATIENCE_DAYS = 250;
export const PATIENCE_MAX_MICROS = 2;
export const SAFEST_MAX_MICROS = 2;
export const SAFEST_MAX_RISK_PCT = 0.5;
export const HORIZON_DAYS = 250;
export const DEFAULT_SIZES_MICROS: readonly number[] = [1, 2, 5, 10, 20];
export const DEFAULT_RISK_LADDER: readonly number[] = [0.25, 0.5, 1, 2];
export const DEFAULT_RISK_PCT = 1;
export const MICROS_PER_MINI = 10;
export const VALUE_MIN_PASS = 40;
export const FASTEST_MIN_PASS = 50;
export const WORK_DAYS_PER_WEEK = 5 / 7;

export type SimStatus = 'pass' | 'breach_trailing' | 'breach_daily' | 'consistency_stall' | 'no_decision';

export interface SimOutcome {
  status: SimStatus;
  /** Trading days consumed until the decision (or runway length). */
  days: number;
  /** Final equity relative to start ($ for futures, % for forex). */
  finalEq: number;
}

/** Per-1-unit P&L multiplier for a size in micros: micro data → 1, mini data → 0.1. */
export function microMultiplier(unit: BacktestUnit): number {
  return unit === 'micro' ? 1 : 1 / MICROS_PER_MINI;
}

/** Calendar days → trading days. */
export function tradingDaysOf(calendarDays: number): number {
  return Math.round(calendarDays * WORK_DAYS_PER_WEEK);
}

// ---------------------------------------------------------------------------------------------
// Shared day paths
// ---------------------------------------------------------------------------------------------

/** Day-index paths (with replacement) shared across sizes and accounts for a given trade set. */
export function makeDayPaths(nDays: number, paths = SIM_PATHS, seed?: number, horizon = HORIZON_DAYS): Int32Array[] {
  const rng = mulberry32(seed ?? defaultSeed(nDays));
  const out: Int32Array[] = [];
  for (let p = 0; p < paths; p++) {
    const a = new Int32Array(horizon);
    for (let k = 0; k < horizon; k++) a[k] = Math.floor(rng() * nDays);
    out.push(a);
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Futures evaluation
// ---------------------------------------------------------------------------------------------

export interface FuturesSimParams {
  profitTarget: number;
  drawdown: DrawdownRule;
  dailyLossLimit: number | null;
  minTradingDays: number;
  consistencyMaxDayPct: number | null;
}

export function floorOf(hwm: number, dd: DrawdownRule): number {
  const lock = dd.stopsTrailing?.lockAtStartPlus;
  const trailing = hwm - dd.amount;
  return lock == null ? trailing : Math.min(trailing, lock);
}

/** One evaluation attempt on a futures account. `sizeMult` scales per-unit P&L to the size traded. */
export function simulateFuturesPath(
  days: readonly DayBlock[],
  path: ArrayLike<number>,
  rules: FuturesSimParams,
  sizeMult: number,
  runway: number,
): SimOutcome {
  let eq = 0;
  let hwm = 0;
  let bestDay = 0;
  let targetHit = false;
  const dll = rules.dailyLossLimit;
  const intraday = rules.drawdown.type === 'intraday';
  const horizon = Math.min(runway, path.length);
  for (let day = 0; day < horizon; day++) {
    const block = days[path[day]];
    let dayPnl = 0;
    const daysTraded = day + 1;
    for (const t of block.trades) {
      const p = t.pnl * sizeMult;
      eq += p;
      dayPnl += p;
      if (intraday && eq > hwm) hwm = eq;
      if (dll != null && dayPnl <= -dll) return { status: 'breach_daily', days: daysTraded, finalEq: eq };
      if (eq <= floorOf(hwm, rules.drawdown)) return { status: 'breach_trailing', days: daysTraded, finalEq: eq };
      const best = Math.max(bestDay, dayPnl);
      if (eq >= rules.profitTarget && daysTraded >= rules.minTradingDays) {
        targetHit = true;
        const pct = rules.consistencyMaxDayPct;
        const consistent = pct == null || best <= (pct / 100) * eq;
        if (consistent) return { status: 'pass', days: daysTraded, finalEq: eq };
      }
    }
    if (dayPnl > bestDay) bestDay = dayPnl;
    if (!intraday && eq > hwm) hwm = eq;
    if (eq <= floorOf(hwm, rules.drawdown)) return { status: 'breach_trailing', days: daysTraded, finalEq: eq };
  }
  return { status: targetHit ? 'consistency_stall' : 'no_decision', days: horizon, finalEq: eq };
}

// ---------------------------------------------------------------------------------------------
// Forex evaluation (percent based)
// ---------------------------------------------------------------------------------------------

export interface ForexSimParams {
  phases: { targetPct: number; minTradingDays: number }[];
  dailyLossPct: number;
  /** 'equity' → max(balance, equity) at day start; 'balance' → balance at day start. */
  dailyBasis: 'equity' | 'balance';
  maxLossPct: number;
  maxLossType: 'static' | 'trailing';
}

export function dailyBasisOf(basis: string | undefined): 'equity' | 'balance' {
  return basis && /equity/i.test(basis) ? 'equity' : 'balance';
}

export function forexParams(a: ForexAccount): ForexSimParams {
  return {
    phases: a.phases.map((p) => ({ targetPct: p.targetPct, minTradingDays: p.minTradingDays })),
    dailyLossPct: a.dailyLoss.pct,
    dailyBasis: dailyBasisOf(a.dailyLoss.basis),
    maxLossPct: a.maxLoss.pct,
    maxLossType: a.maxLoss.type === 'trailing' ? 'trailing' : 'static',
  };
}

/** Per-trade percent move: R × risk% when R exists, else pnl/accountSize×100 scaled by risk/1%. */
export function tradePct(t: { r: number | null; raw: number }, riskPct: number, accountSize: number): number {
  if (t.r != null) return t.r * riskPct;
  return ((t.raw / accountSize) * 100 * riskPct) / DEFAULT_RISK_PCT;
}

/**
 * Phase-based forex evaluation. Balance is tracked in percent of the phase's starting balance
 * (100 = start). Each phase restarts from a fresh balance; `days` is the sum over phases.
 */
export function simulateForexPath(
  days: readonly DayBlock[],
  path: ArrayLike<number>,
  rules: ForexSimParams,
  riskPct: number,
  accountSize: number,
  runwayPerPhase = RUNWAY_DAYS,
): SimOutcome {
  let cursor = 0;
  let totalDays = 0;
  for (const phase of rules.phases) {
    let bal = 100;
    let hwm = 100;
    let phaseDays = 0;
    let passed = false;
    while (phaseDays < runwayPerPhase && cursor < path.length) {
      const block = days[path[cursor++]];
      phaseDays++;
      totalDays++;
      const basis = bal; // at trade-close granularity balance == equity at day start
      const dayLimit = (rules.dailyLossPct / 100) * (rules.dailyBasis === 'equity' ? Math.max(bal, basis) : basis);
      const floorStatic = 100 - rules.maxLossPct;
      for (const t of block.trades) {
        bal += tradePct(t, riskPct, accountSize);
        if (bal > hwm) hwm = bal;
        if (basis - bal >= dayLimit) return { status: 'breach_daily', days: totalDays, finalEq: bal - 100 };
        const floor = rules.maxLossType === 'trailing' ? hwm - rules.maxLossPct : floorStatic;
        if (bal <= floor) return { status: 'breach_trailing', days: totalDays, finalEq: bal - 100 };
      }
      if (bal - 100 >= phase.targetPct && phaseDays >= phase.minTradingDays) {
        passed = true;
        break;
      }
    }
    if (!passed) return { status: 'no_decision', days: totalDays, finalEq: bal - 100 };
  }
  return { status: 'pass', days: totalDays, finalEq: 0 };
}

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

export interface SizeResult {
  /** Size label: micros for futures, risk % for forex. */
  size: number;
  sizeLabel: string;
  passPct: number;
  breachTrailingPct: number;
  breachDailyPct: number;
  stallPct: number;
  noDecisionPct: number;
  daysToPass: Q3 | null;
  cost: Q3;
  expCost: number | null;
  valueScore: number;
  runway: number;
}

export interface SurvivalResult {
  accountId: string;
  label: string;
  firmId: string;
  family: 'futures' | 'forex';
  paths: number;
  seed: number;
  sizes: SizeResult[];
  recommended: number | null;
  safest: number | null;
  fastest: number | null;
  notes: string[];
}

export interface SurvivalOptions {
  paths?: number;
  seed?: number;
  /** Backtest unit of the normalized trades (futures). */
  unit?: BacktestUnit;
  /** Sizes in micros (futures) or risk % (forex). */
  sizes?: readonly number[];
  /** Account size used for pnl→% when trades lack R (forex). */
  accountSize?: number | null;
  /** Pre-built day paths to share across accounts. */
  dayPaths?: Int32Array[];
}

export function periodsFor(tradeDays: number): number {
  return Math.max(1, Math.ceil(Math.ceil(tradeDays / WORK_DAYS_PER_WEEK) / 30));
}

export function attemptCost(cost: CostRule, tradeDays: number): number {
  if (cost.structure === 'per30d') {
    const renewal = cost.renewal ?? cost.evalFull;
    return cost.evalFull + (periodsFor(tradeDays) - 1) * renewal;
  }
  return cost.evalFull;
}

export function activationFeeOf(account: Account): number {
  const f = (account as FuturesAccount).funded;
  const fee = f && typeof f.activationFee === 'number' ? f.activationFee : 0;
  return fee;
}

export function valueScoreOf(passPct: number, expCost: number | null, medDays: number | null): number {
  if (passPct < VALUE_MIN_PASS || expCost == null || medDays == null) return 0;
  const p = passPct / 100;
  const v = (0.5 * passPct + (0.25 * 10000) / (100 + expCost) + (0.25 * 3000) / (30 + medDays / p)) * p ** 0.75;
  return round(v);
}

function runwayFor(account: Account, size: number, family: 'futures' | 'forex'): number {
  if (family === 'forex') return RUNWAY_DAYS;
  const cost = account.cost;
  if (cost.structure === 'per30d') return tradingDaysOf(cost.windowDays ?? 30);
  return size <= PATIENCE_MAX_MICROS ? PATIENCE_DAYS : RUNWAY_DAYS;
}

export function runSurvival(days: readonly DayBlock[], ref: AccountRef, opts: SurvivalOptions = {}): SurvivalResult {
  const { account, firm } = ref;
  const family: 'futures' | 'forex' = isForexAccount(account) ? 'forex' : 'futures';
  const paths = opts.paths ?? SIM_PATHS;
  const seed = opts.seed ?? defaultSeed(days.length);
  const dayPaths = opts.dayPaths ?? makeDayPaths(days.length, paths, seed, HORIZON_DAYS);
  const notes: string[] = [];
  const results: SizeResult[] = [];

  let sizes: number[];
  if (family === 'forex') sizes = [...(opts.sizes ?? DEFAULT_RISK_LADDER)];
  else {
    const fa = account as FuturesAccount;
    const maxMicros = fa.maxContracts * MICROS_PER_MINI;
    sizes = [...(opts.sizes ?? DEFAULT_SIZES_MICROS)].filter((s) => s <= maxMicros);
    if (sizes.length === 0) sizes = [Math.max(1, maxMicros)];
    if ((fa as FuturesAccount).dailySoftLossLimit) notes.push('Soft daily loss limit is not simulated (not a breach).');
    if (fa.drawdown.includesUnrealized) notes.push('Trailing drawdown counts unrealized highs; trade-close simulation is optimistic.');
  }

  const activation = activationFeeOf(account);
  const retry = account.cost.renewal ?? account.cost.evalFull;
  const accountSize = opts.accountSize ?? account.accountSize;

  for (const size of sizes) {
    const runway = runwayFor(account, size, family);
    const counts: Record<SimStatus, number> = { pass: 0, breach_trailing: 0, breach_daily: 0, consistency_stall: 0, no_decision: 0 };
    const passDays: number[] = [];
    for (const path of dayPaths) {
      let o: SimOutcome;
      if (family === 'forex') {
        const fx = account as ForexAccount;
        o = simulateForexPath(days, path, forexParams(fx), size, accountSize, runway);
      } else {
        const fa = account as FuturesAccount;
        o = simulateFuturesPath(days, path, fa, size * microMultiplier(opts.unit ?? 'mini'), runway);
      }
      counts[o.status]++;
      if (o.status === 'pass') passDays.push(o.days);
    }
    const n = dayPaths.length;
    const pct = (k: number) => round((k / n) * 100);
    const passPct = pct(counts.pass);
    const sorted = sortAsc(passDays);
    const daysToPass: Q3 | null = sorted.length
      ? { p25: round(percentile(sorted, 25)), p50: round(percentile(sorted, 50)), p75: round(percentile(sorted, 75)) }
      : null;
    const costQ: Q3 = {
      p25: round(attemptCost(account.cost, daysToPass?.p25 ?? runway), 2),
      p50: round(attemptCost(account.cost, daysToPass?.p50 ?? runway), 2),
      p75: round(attemptCost(account.cost, daysToPass?.p75 ?? runway), 2),
    };
    const expCost = passPct > 0 ? round(costQ.p50 + (100 / passPct - 1) * retry + activation, 2) : null;
    results.push({
      size,
      sizeLabel: family === 'forex' ? `${size}%` : `${size} micro`,
      passPct,
      breachTrailingPct: pct(counts.breach_trailing),
      breachDailyPct: pct(counts.breach_daily),
      stallPct: pct(counts.consistency_stall),
      noDecisionPct: pct(counts.no_decision),
      daysToPass,
      cost: costQ,
      expCost,
      valueScore: valueScoreOf(passPct, expCost, daysToPass?.p50 ?? null),
      runway,
    });
  }

  let recommended: number | null = null;
  let bestValue = 0;
  for (const r of results) {
    if (r.valueScore > bestValue) {
      bestValue = r.valueScore;
      recommended = r.size;
    }
  }
  const safeLimit = family === 'forex' ? SAFEST_MAX_RISK_PCT : SAFEST_MAX_MICROS;
  let safest: number | null = null;
  let safePass = -1;
  for (const r of results) {
    if (r.size <= safeLimit && r.passPct > safePass) {
      safePass = r.passPct;
      safest = r.size;
    }
  }
  let fastest: number | null = null;
  let fastDays = Infinity;
  for (const r of results) {
    if (r.passPct >= FASTEST_MIN_PASS && r.daysToPass && r.daysToPass.p50 < fastDays) {
      fastDays = r.daysToPass.p50;
      fastest = r.size;
    }
  }

  return {
    accountId: account.id,
    label: account.label,
    firmId: firm.id,
    family,
    paths: dayPaths.length,
    seed,
    sizes: results,
    recommended,
    safest,
    fastest,
    notes,
  };
}
