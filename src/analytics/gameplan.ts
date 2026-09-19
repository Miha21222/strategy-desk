/**
 * Funded-account simulation, scaling phases and plan sliders (common random numbers).
 * Formulas: docs/analytics.md ("Геймплан").
 */
import type { DayBlock, Trade } from './trades';
import { mulberry32, percentile, round, sortAsc } from './rng';
import { defaultSeed } from './bootstrap';
import { floorOf, forexParams, makeDayPaths, microMultiplier, tradePct, tradingDaysOf, HORIZON_DAYS, SIM_PATHS } from './propsim';
import { isForexAccount, type AccountRef, type ForexAccount, type FuturesAccount } from './rules';
import type { BacktestUnit, Q3 } from './types';

// ---------------------------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------------------------

export const GAMEPLAN_PATHS = SIM_PATHS;
export const GAMEPLAN_HORIZON = HORIZON_DAYS;
export const PLAN_REPS = 32;
export const DAYS_PER_WEEK = 5;
export const DAYS_PER_MONTH = 21;
export const BUFFER_EXTRA = 100; // default buffer = drawdown + 100
export const FLOOR_MARGIN = 100; // payout may not bring equity closer than 100 to the floor
export const PLAN_WIN_RATE_RANGE: [number, number] = [30, 80];
export const PLAN_EXEC_RANGE: [number, number] = [50, 100];

// ---------------------------------------------------------------------------------------------
// Funded simulation
// ---------------------------------------------------------------------------------------------

export interface FundedPathOutcome {
  survived: boolean;
  daysToBuffer: number | null;
  daysToFirstPayout: number | null;
  totalPaid: number;
  payouts: number;
  days: number;
}

export interface FundedFuturesRules {
  drawdown: FuturesAccount['drawdown'];
  dailyLossLimit: number | null;
  buffer: number;
  firstPayoutDay: number;
  window: number;
  minPayout: number;
  maxPayoutPct: number;
  payoutCap: number | null;
  maxPayouts: number | null;
}

export function fundedFuturesRules(a: FuturesAccount): FundedFuturesRules {
  const f = a.funded ?? {};
  const dd = f.drawdown ?? a.drawdown;
  return {
    drawdown: dd,
    dailyLossLimit: a.dailyLossLimit,
    buffer: f.bufferAmount ?? dd.amount + BUFFER_EXTRA,
    firstPayoutDay: Math.max(1, f.minDaysToFirstPayout ?? 0),
    window: Math.max(1, tradingDaysOf(f.payoutFrequencyDays ?? 14)),
    minPayout: f.minPayout ?? 0,
    maxPayoutPct: f.maxPayoutPct ?? 100,
    payoutCap: typeof f.payoutCap === 'number' ? f.payoutCap : null,
    maxPayouts: typeof f.maxPayoutsLifetime === 'number' ? f.maxPayoutsLifetime : null,
  };
}

export function simulateFundedFutures(
  days: readonly DayBlock[],
  path: ArrayLike<number>,
  rules: FundedFuturesRules,
  sizeMult: number,
  horizon = GAMEPLAN_HORIZON,
): FundedPathOutcome {
  let eq = 0;
  let hwm = 0;
  let totalPaid = 0;
  let payouts = 0;
  let daysToBuffer: number | null = null;
  let daysToFirstPayout: number | null = null;
  const intraday = rules.drawdown.type === 'intraday';
  const n = Math.min(horizon, path.length);
  for (let day = 0; day < n; day++) {
    const block = days[path[day]];
    const D = day + 1;
    let dayPnl = 0;
    for (const t of block.trades) {
      const p = t.pnl * sizeMult;
      eq += p;
      dayPnl += p;
      if (intraday && eq > hwm) hwm = eq;
      if (rules.dailyLossLimit != null && dayPnl <= -rules.dailyLossLimit) {
        return { survived: false, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
      }
      if (eq <= floorOf(hwm, rules.drawdown)) return { survived: false, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
    }
    if (!intraday && eq > hwm) hwm = eq;
    if (eq <= floorOf(hwm, rules.drawdown)) return { survived: false, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
    if (daysToBuffer == null && eq >= rules.buffer) daysToBuffer = D;
    if (D >= rules.firstPayoutDay && (D - rules.firstPayoutDay) % rules.window === 0) {
      let w = eq - rules.buffer;
      w = w * (rules.maxPayoutPct / 100);
      if (rules.payoutCap != null) w = Math.min(w, rules.payoutCap);
      w = Math.min(w, eq - (floorOf(hwm, rules.drawdown) + FLOOR_MARGIN));
      if (w >= rules.minPayout && w > 0) {
        eq -= w;
        totalPaid += w;
        payouts++;
        if (daysToFirstPayout == null) daysToFirstPayout = D;
        if (rules.maxPayouts != null && payouts >= rules.maxPayouts) {
          return { survived: true, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
        }
      }
    }
  }
  return { survived: true, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: n };
}

export interface FundedForexRules {
  dailyLossPct: number;
  dailyBasis: 'equity' | 'balance';
  maxLossPct: number;
  maxLossType: 'static' | 'trailing';
  splitPct: number;
  cycleName: string;
  firstPayoutDay: number;
  window: number;
}

export function pickSplit(f: ForexAccount['funded']): { name: string; pct: number } {
  const by = f?.profitSplitByCycle ?? {};
  const cycleDays = f?.payoutCycleDays ?? 14;
  const preferred = cycleDays <= 7 ? 'weekly' : cycleDays <= 14 ? 'biweekly' : 'monthly';
  if (typeof by[preferred] === 'number') return { name: preferred, pct: by[preferred] };
  const first = Object.entries(by).find(([, v]) => typeof v === 'number');
  return first ? { name: first[0], pct: first[1] } : { name: 'default', pct: 80 };
}

export function fundedForexRules(a: ForexAccount): FundedForexRules {
  const p = forexParams(a);
  const split = pickSplit(a.funded);
  return {
    dailyLossPct: p.dailyLossPct,
    dailyBasis: p.dailyBasis,
    maxLossPct: p.maxLossPct,
    maxLossType: p.maxLossType,
    splitPct: split.pct,
    cycleName: split.name,
    firstPayoutDay: Math.max(1, tradingDaysOf(a.funded?.firstPayoutDays ?? 14)),
    window: Math.max(1, tradingDaysOf(a.funded?.payoutCycleDays ?? 14)),
  };
}

/** Forex funded: balance in % of start; payouts withdraw the whole profit (trader keeps split%). */
export function simulateFundedForex(
  days: readonly DayBlock[],
  path: ArrayLike<number>,
  rules: FundedForexRules,
  riskPct: number,
  accountSize: number,
  horizon = GAMEPLAN_HORIZON,
): FundedPathOutcome {
  let bal = 100;
  let hwm = 100;
  let totalPaid = 0;
  let payouts = 0;
  let daysToBuffer: number | null = null;
  let daysToFirstPayout: number | null = null;
  const n = Math.min(horizon, path.length);
  for (let day = 0; day < n; day++) {
    const block = days[path[day]];
    const D = day + 1;
    const basis = bal;
    const dayLimit = (rules.dailyLossPct / 100) * basis;
    for (const t of block.trades) {
      bal += tradePct(t, riskPct, accountSize);
      if (bal > hwm) hwm = bal;
      if (basis - bal >= dayLimit) return { survived: false, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
      const floor = rules.maxLossType === 'trailing' ? hwm - rules.maxLossPct : 100 - rules.maxLossPct;
      if (bal <= floor) return { survived: false, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: D };
    }
    if (daysToBuffer == null && bal > 100) daysToBuffer = D;
    if (D >= rules.firstPayoutDay && (D - rules.firstPayoutDay) % rules.window === 0 && bal > 100) {
      const profitPct = bal - 100;
      const pay = (profitPct * rules.splitPct) / 100;
      totalPaid += (pay / 100) * accountSize;
      payouts++;
      if (daysToFirstPayout == null) daysToFirstPayout = D;
      bal = 100;
      hwm = 100;
    }
  }
  return { survived: true, daysToBuffer, daysToFirstPayout, totalPaid, payouts, days: n };
}

// ---------------------------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------------------------

export interface FundedResult {
  survivePct: number;
  payoutPct: number;
  reachBufferPct: number;
  daysToBuffer: Q3 | null;
  daysToFirstPayout: Q3 | null;
  monthlyIncome: Q3;
  totalIncome: Q3;
  paths: number;
  horizon: number;
  notes: string[];
}

function q3of(xs: number[]): Q3 | null {
  if (!xs.length) return null;
  const s = sortAsc(xs);
  return { p25: round(percentile(s, 25)), p50: round(percentile(s, 50)), p75: round(percentile(s, 75)) };
}

export interface FundedOptions {
  paths?: number;
  seed?: number;
  unit?: BacktestUnit;
  /** Size in micros (futures) or risk % (forex). */
  size: number;
  accountSize?: number | null;
  dayPaths?: Int32Array[];
  horizon?: number;
}

export function runFunded(days: readonly DayBlock[], ref: AccountRef, opts: FundedOptions): FundedResult {
  const { account } = ref;
  const paths = opts.paths ?? GAMEPLAN_PATHS;
  const seed = opts.seed ?? defaultSeed(days.length);
  const horizon = opts.horizon ?? GAMEPLAN_HORIZON;
  const dayPaths = opts.dayPaths ?? makeDayPaths(days.length, paths, seed, horizon);
  const notes: string[] = [];
  const outcomes: FundedPathOutcome[] = [];
  if (isForexAccount(account)) {
    const rules = fundedForexRules(account);
    const accountSize = opts.accountSize ?? account.accountSize;
    notes.push(`Profit split ${rules.splitPct}% (${rules.cycleName} cycle).`);
    if (account.funded?.strikeFloatingLossPct != null) notes.push(`Strike rule: floating loss ${account.funded.strikeFloatingLossPct}% (not simulated).`);
    if (account.funded?.onDemandConsistencyMaxDayPct != null) notes.push(`On-demand consistency: best day ≤ ${account.funded.onDemandConsistencyMaxDayPct}% (not simulated).`);
    for (const p of dayPaths) outcomes.push(simulateFundedForex(days, p, rules, opts.size, accountSize, horizon));
  } else {
    const fa = account as FuturesAccount;
    const rules = fundedFuturesRules(fa);
    const mult = opts.size * microMultiplier(opts.unit ?? 'mini');
    if (fa.funded?.consistencyMaxDayPct != null) notes.push(`Funded consistency: best day ≤ ${fa.funded.consistencyMaxDayPct}% of profit (blocks payouts; not simulated).`);
    if (rules.maxPayouts != null) notes.push(`Account concludes after ${rules.maxPayouts} payouts.`);
    for (const p of dayPaths) outcomes.push(simulateFundedFutures(days, p, rules, mult, horizon));
  }
  const n = outcomes.length;
  const months = horizon / DAYS_PER_MONTH;
  const survived = outcomes.filter((o) => o.survived).length;
  const paid = outcomes.filter((o) => o.payouts > 0).length;
  const reached = outcomes.filter((o) => o.daysToBuffer != null).length;
  const incomes = outcomes.map((o) => o.totalPaid);
  const monthly = incomes.map((x) => x / months);
  return {
    survivePct: round((survived / n) * 100),
    payoutPct: round((paid / n) * 100),
    reachBufferPct: round((reached / n) * 100),
    daysToBuffer: q3of(outcomes.map((o) => o.daysToBuffer).filter((x): x is number => x != null)),
    daysToFirstPayout: q3of(outcomes.map((o) => o.daysToFirstPayout).filter((x): x is number => x != null)),
    monthlyIncome: q3of(monthly) ?? { p25: 0, p50: 0, p75: 0 },
    totalIncome: q3of(incomes) ?? { p25: 0, p50: 0, p75: 0 },
    paths: n,
    horizon,
    notes,
  };
}

// ---------------------------------------------------------------------------------------------
// Phases
// ---------------------------------------------------------------------------------------------

export interface Phases {
  proofDays: number;
  cyclesToFull: number;
  fullScaleDays: number;
  weeks: number;
  months: number;
  maxAccounts: number;
  perCycle: number;
  daysToPass: number;
}

export function scalingPhases(daysToPassP50: number, funded: FundedResult, maxAccounts: number, perCycle = 1): Phases {
  const proofDays = Math.max(funded.daysToBuffer?.p50 ?? funded.horizon, funded.daysToFirstPayout?.p50 ?? funded.horizon);
  const accounts = Math.max(1, maxAccounts);
  const cyclesToFull = Math.max(1, Math.ceil((accounts - 1) / Math.max(1, perCycle)));
  const fullScaleDays = daysToPassP50 + cyclesToFull * proofDays;
  return {
    proofDays: round(proofDays),
    cyclesToFull,
    fullScaleDays: round(fullScaleDays),
    weeks: round(fullScaleDays / DAYS_PER_WEEK),
    months: round(fullScaleDays / DAYS_PER_MONTH),
    maxAccounts: accounts,
    perCycle,
    daysToPass: daysToPassP50,
  };
}

// ---------------------------------------------------------------------------------------------
// Plan sliders with common random numbers
// ---------------------------------------------------------------------------------------------

export interface Plan {
  /** Target win rate 30–80 (%); null keeps the recorded outcomes' rate. */
  winRate?: number | null;
  /** Execution discipline 50–100 (% of days traded on plan). */
  exec?: number | null;
  /** Max trades per day; null = unlimited. */
  maxTradesPerDay?: number | null;
  /** Daily loss cap in per-unit P&L terms (positive number); null = none. */
  dailyCap?: number | null;
}

function clampRange(x: number | null | undefined, [lo, hi]: [number, number], dflt: number): number {
  if (x == null || !Number.isFinite(x)) return dflt;
  return Math.min(hi, Math.max(lo, x));
}

/**
 * Replay the recorded days under a plan. Every recorded trade consumes exactly two uniforms
 * (outcome, pick) and every day one uniform (on/off plan), regardless of caps, so two plans
 * that differ only in a slider see the same random stream (common random numbers).
 */
export function replayWithPlan(days: readonly DayBlock[], plan: Plan, seed?: number, reps = PLAN_REPS): DayBlock[] {
  const all: Trade[] = [];
  for (const d of days) all.push(...d.trades);
  const winPile = all.filter((t) => t.pnl > 0).map((t) => t.pnl);
  const losePile = all.filter((t) => t.pnl <= 0).map((t) => t.pnl);
  const recordedWr = all.length ? winPile.length / all.length : 0.5;
  const wr = plan.winRate == null ? recordedWr : clampRange(plan.winRate, PLAN_WIN_RATE_RANGE, recordedWr * 100) / 100;
  const exec = clampRange(plan.exec, PLAN_EXEC_RANGE, 100) / 100;
  const maxTrades = plan.maxTradesPerDay != null && plan.maxTradesPerDay > 0 ? Math.floor(plan.maxTradesPerDay) : null;
  const cap = plan.dailyCap != null && plan.dailyCap > 0 ? plan.dailyCap : null;
  const rng = mulberry32(seed ?? defaultSeed(days.length) + 17);
  const out: DayBlock[] = [];
  for (let rep = 0; rep < reps; rep++) {
    for (const day of days) {
      const onPlan = rng() < exec;
      const trades: Trade[] = [];
      let dayPnl = 0;
      day.trades.forEach((t, j) => {
        const u1 = rng();
        const u2 = rng();
        if (maxTrades != null && j >= maxTrades) return;
        if (cap != null && dayPnl <= -cap) return;
        let pnl: number;
        if (!onPlan) pnl = losePile.length ? losePile[Math.floor(u2 * losePile.length)] : Math.min(t.pnl, 0);
        else if (u1 < wr) pnl = winPile.length ? winPile[Math.floor(u2 * winPile.length)] : t.pnl;
        else pnl = losePile.length ? losePile[Math.floor(u2 * losePile.length)] : t.pnl;
        dayPnl += pnl;
        trades.push({ ...t, pnl, r: null });
      });
      out.push({ d: day.d, trades, net: dayPnl });
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------------
// Gameplan
// ---------------------------------------------------------------------------------------------

export interface GameplanOptions extends Omit<FundedOptions, 'size'> {
  size?: number;
  plan?: Plan | null;
  daysToPassP50: number;
  maxAccounts?: number | null;
  perCycle?: number;
}

export interface GameplanResult {
  accountId: string;
  label: string;
  size: number;
  sizeLabel: string;
  funded: FundedResult;
  phases: Phases;
  plan: Plan | null;
  /** Same simulation on the recorded days without the plan (for side-by-side display). */
  baseline: FundedResult | null;
}

export function runGameplan(days: readonly DayBlock[], ref: AccountRef, opts: GameplanOptions): GameplanResult {
  const forex = isForexAccount(ref.account);
  const size = opts.size ?? (forex ? 1 : 2);
  const unit = opts.unit ?? 'mini';
  let pool: readonly DayBlock[] = days;
  let baseline: FundedResult | null = null;
  const hasPlan = !!opts.plan && Object.values(opts.plan).some((v) => v != null);
  if (hasPlan) {
    const mult = forex ? 1 : size * microMultiplier(unit);
    const planUnit: Plan = { ...opts.plan, dailyCap: opts.plan?.dailyCap != null ? opts.plan.dailyCap / mult : null };
    pool = replayWithPlan(days, planUnit, opts.seed);
    baseline = runFunded(days, ref, { ...opts, size });
  }
  const funded = runFunded(pool, ref, { ...opts, size, dayPaths: undefined });
  const maxAccounts = opts.maxAccounts ?? ref.firm.maxFundedAccounts ?? (ref.account as FuturesAccount).maxFundedAccounts ?? 1;
  const phases = scalingPhases(opts.daysToPassP50, funded, maxAccounts, opts.perCycle ?? 1);
  return {
    accountId: ref.account.id,
    label: ref.account.label,
    size,
    sizeLabel: forex ? `${size}%` : `${size} micro`,
    funded,
    phases,
    plan: hasPlan ? opts.plan ?? null : null,
    baseline,
  };
}
