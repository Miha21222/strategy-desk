/**
 * Analytics entry point: `analyzeProdata(prodata, rules, opts)` → typed Report, and
 * `computeTradingScore(history)`. Pure TypeScript, no DOM; deterministic under a seed.
 */
import { bootstrapDays, type BootstrapResult } from './bootstrap';
import { runGameplan, type GameplanResult, type Plan } from './gameplan';
import { accountSurvival, overnightGap, type AccountSurvival, type OvernightGap } from './portfolio';
import { runSurvival, makeDayPaths, HORIZON_DAYS, SIM_PATHS, type SurvivalResult } from './propsim';
import {
  concentration,
  killzone,
  outliers,
  qualityScore,
  regime,
  rollingWindows,
  trueCost,
  type Concentration,
  type Killzone,
  type Outliers,
  type QualityResult,
  type Regime,
  type RollingWindows,
  type TrueCost,
  type FuturesCost,
  type ForexCost,
} from './quality';
import { findAccount, listAccounts, type Family, type PropFirmsData } from './rules';
import { basicStats, groupDays, normalizeTrades, AnalyticsError, type DayBlock, type Trade } from './trades';
import { maxDrawdown, round } from './rng';
import { defaultSeed } from './bootstrap';
import type { Focus, Prodata, Q5 } from './types';

export * from './types';
export * from './rng';
export * from './trades';
export * from './bootstrap';
export * from './quality';
export * from './rules';
export * from './propsim';
export * from './gameplan';
export * from './score';
export * from './portfolio';

// ---------------------------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------------------------

export interface Header {
  n: number;
  days: number;
  from: string;
  to: string;
  net: number;
  profitFactor: number;
  winRate: number;
  expectancy: number;
  avgWin: number;
  avgLoss: number;
  maxDrawdown: number;
  bestDay: number;
  worstDay: number;
  tradesPerDay: number;
  /** Unit every number is expressed in (per 1 mini / micro contract, R or raw pnl). */
  unitLabel: string;
}

export interface BootstrapSummary {
  paths: number;
  seed: number;
  final: Q5;
  maxDD: { p50: number; p95: number };
  worstDay: { p50: number; p95: number };
  lossShare: number;
}

export interface QualitySection {
  score: QualityResult;
  rolling: RollingWindows;
  outliers: Outliers;
  concentration: Concentration;
  killzone: Killzone | null;
  trueCost: TrueCost;
  regime: Regime | null;
  overnightGap: OvernightGap | null;
  accountSurvival: AccountSurvival | null;
}

export interface Report {
  strategy: string;
  version: string | null;
  market: Prodata['market'];
  unit: Prodata['backtestUnit'];
  family: Family;
  focus: Focus;
  header: Header;
  bootstrap: BootstrapSummary;
  evalGrid: SurvivalResult[] | null;
  gameplan: GameplanResult | null;
  quality: QualitySection | null;
  notes: string[];
  simCaveat: string | null;
}

export interface AnalyzeOptions {
  paths?: number;
  seed?: number;
  /** Sizes for the eval grid (micros for futures, risk % for forex). */
  sizes?: readonly number[];
  /** Size for the gameplan (defaults to the recommended size of the gameplan account). */
  gameplanSize?: number;
  plan?: Plan | null;
  accountsPerCycle?: number;
  costOverrides?: Partial<FuturesCost & ForexCost>;
  /** Cap the number of accounts in the eval grid (0 = no cap). */
  maxAccounts?: number;
}

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

export function familyOf(market: Prodata['market']): Family {
  return market === 'futures' ? 'futures' : 'forex';
}

export function unitLabelOf(p: Pick<Prodata, 'market' | 'backtestUnit' | 'trades'>): string {
  if (p.market === 'futures') return `per 1 ${p.backtestUnit}`;
  const hasR = p.trades.some((t) => t.r != null);
  return hasR ? 'R' : 'account currency';
}

export function buildHeader(trades: readonly Trade[], days: readonly DayBlock[], unitLabel: string): Header {
  const pnls = trades.map((t) => t.pnl);
  const st = basicStats(pnls);
  const nets = days.map((d) => d.net);
  return {
    n: st.n,
    days: days.length,
    from: days[0]?.d ?? '',
    to: days[days.length - 1]?.d ?? '',
    net: round(st.net, 2),
    profitFactor: round(st.profitFactor, 2),
    winRate: round(st.winRate * 100),
    expectancy: round(st.expectancy, 2),
    avgWin: round(st.avgWin, 2),
    avgLoss: round(st.avgLoss, 2),
    maxDrawdown: round(maxDrawdown(pnls), 2),
    bestDay: round(Math.max(0, ...nets), 2),
    worstDay: round(Math.min(0, ...nets), 2),
    tradesPerDay: days.length ? round(st.n / days.length, 2) : 0,
    unitLabel,
  };
}

export function summarizeBootstrap(b: BootstrapResult): BootstrapSummary {
  return {
    paths: b.paths,
    seed: b.seed,
    final: { p5: round(b.final.p5, 2), p25: round(b.final.p25, 2), p50: round(b.final.p50, 2), p75: round(b.final.p75, 2), p95: round(b.final.p95, 2) },
    maxDD: { p50: round(b.maxDD.p50, 2), p95: round(b.maxDD.p95, 2) },
    worstDay: { p50: round(b.worstDay.p50, 2), p95: round(b.worstDay.p95, 2) },
    lossShare: round(b.lossShare),
  };
}

function oosSplit(trades: readonly Trade[], oosStart: string | null | undefined): { isPF: number; oosPF: number } | null {
  if (!oosStart) return null;
  const is = trades.filter((t) => t.d < oosStart).map((t) => t.pnl);
  const oos = trades.filter((t) => t.d >= oosStart).map((t) => t.pnl);
  if (is.length < 10 || oos.length < 10) return null;
  return { isPF: basicStats(is).profitFactor, oosPF: basicStats(oos).profitFactor };
}

// ---------------------------------------------------------------------------------------------
// analyzeProdata
// ---------------------------------------------------------------------------------------------

export function analyzeProdata(prodata: Prodata, rules: PropFirmsData, opts: AnalyzeOptions = {}): Report {
  if (!prodata || typeof prodata !== 'object') throw new AnalyticsError('prodata block is missing');
  const trades = normalizeTrades(prodata);
  const days = groupDays(trades);
  const pnls = trades.map((t) => t.pnl);
  const family = familyOf(prodata.market);
  const focus: Focus = prodata.focus === 'gameplan' || prodata.focus === 'full' ? prodata.focus : 'survival';
  const paths = opts.paths ?? SIM_PATHS;
  const seed = opts.seed ?? defaultSeed(days.length);
  const notes: string[] = [];
  const unit = prodata.backtestUnit ?? 'mini';

  const header = buildHeader(trades, days, unitLabelOf(prodata));
  const boot = bootstrapDays(days, { paths, seed });

  // Shared day paths for every account and size.
  const dayPaths = makeDayPaths(days.length, paths, seed, HORIZON_DAYS);

  let evalGrid: SurvivalResult[] | null = null;
  const wantGrid = focus === 'survival' || focus === 'full';
  if (wantGrid) {
    let refs = listAccounts(rules, { ids: prodata.firms ?? null, family });
    if (opts.maxAccounts && opts.maxAccounts > 0) refs = refs.slice(0, opts.maxAccounts);
    if (!refs.length) notes.push(`No ${family} accounts matched the requested firms.`);
    evalGrid = refs.map((ref) =>
      runSurvival(days, ref, { paths, seed, unit, sizes: opts.sizes, accountSize: prodata.accountSize ?? null, dayPaths }),
    );
  }

  let gameplan: GameplanResult | null = null;
  if (focus === 'gameplan' || focus === 'full') {
    const id = prodata.gameplan?.firm;
    const ref = id ? findAccount(rules, id) : null;
    if (!ref) {
      if (id) notes.push(`Gameplan account "${id}" not found in the rules file.`);
      else if (focus === 'gameplan') notes.push('Gameplan needs "gameplan.firm" (an account id).');
    } else if (ref.firm.family !== family) {
      notes.push(`Gameplan account "${id}" is a ${ref.firm.family} account but the data is ${prodata.market}.`);
    } else {
      const survival =
        evalGrid?.find((g) => g.accountId === ref.account.id) ??
        runSurvival(days, ref, { paths, seed, unit, sizes: opts.sizes, accountSize: prodata.accountSize ?? null, dayPaths });
      const size = opts.gameplanSize ?? survival.recommended ?? survival.safest ?? survival.sizes[0]?.size ?? 1;
      const sizeRow = survival.sizes.find((s) => s.size === size) ?? survival.sizes[0];
      const daysToPassP50 = sizeRow?.daysToPass?.p50 ?? sizeRow?.runway ?? 60;
      if (!sizeRow?.daysToPass) notes.push('No path passed the evaluation at the gameplan size; phases use the runway length instead.');
      gameplan = runGameplan(days, ref, {
        paths,
        seed,
        unit,
        size,
        accountSize: prodata.accountSize ?? null,
        plan: opts.plan ?? null,
        daysToPassP50,
        perCycle: opts.accountsPerCycle ?? 1,
      });
      if (focus === 'gameplan' && !evalGrid) evalGrid = [survival];
    }
  }

  let quality: QualitySection | null = null;
  if (focus === 'full') {
    const mc = bootstrapDays(days, { paths, seed });
    const score = qualityScore(pnls, mc, {
      variants: prodata.variants ?? null,
      oos: oosSplit(trades, prodata.oosStart),
      tradingDays: days.length,
    });
    const pointValue = prodata.pointValue ?? (unit === 'micro' ? 5 : 50);
    quality = {
      score,
      rolling: rollingWindows(trades),
      outliers: outliers(pnls),
      concentration: concentration(trades),
      killzone: killzone(trades),
      trueCost: trueCost(pnls, unit, opts.costOverrides),
      regime: regime(trades, prodata.daily),
      overnightGap: overnightGap(prodata.daily, pointValue, trades),
      accountSurvival: prodata.accountSize && family === 'futures' ? accountSurvival(days, prodata.accountSize, unit, { paths, seed }) : null,
    };
  }

  return {
    strategy: prodata.strategy,
    version: prodata.version ?? null,
    market: prodata.market,
    unit,
    family,
    focus,
    header,
    bootstrap: summarizeBootstrap(boot),
    evalGrid,
    gameplan,
    quality,
    notes,
    simCaveat: rules.simCaveat ?? null,
  };
}
