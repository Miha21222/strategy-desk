/**
 * My Trading Score — behaviour score over the user's own trade history (journal / healthdata).
 * Formulas: docs/analytics.md ("Мой торговый счёт").
 */
import { monteCarloTrades } from './bootstrap';
import { basicStats, groupDays, normalizeTrades, type DayBlock, type Trade } from './trades';
import { rMultiples, sqn } from './quality';
import { band, linearFit, maxDrawdown, mean, median, percentile, round, sd, sortAsc } from './rng';

// ---------------------------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------------------------

export const SCORE_MIN_TRADES = 30;
export const SCORE_MIN_DAYS = 10;
export const SCORE_WEIGHTS = { edge: 0.3, risk: 0.25, consistency: 0.25, discipline: 0.2 } as const;
export const SCORE_LETTERS: readonly [number, string][] = [
  [85, 'A'],
  [70, 'B'],
  [55, 'C'],
  [40, 'D'],
];
export const TIER_DEVELOPING = 100;
export const TIER_ESTABLISHED = 300;

type Anchors = readonly (readonly [number, number])[];

export const EDGE_SQN: Anchors = [[-1, 0], [0, 10], [1, 45], [1.6, 60], [2, 70], [2.5, 80], [3, 90], [5, 100]];
export const EDGE_PF: Anchors = [[0.5, 0], [1, 35], [1.3, 55], [1.5, 65], [2, 80], [2.6, 100]];
export const EDGE_EXPECTANCY: Anchors = [[-0.5, 0], [0, 20], [0.2, 50], [0.5, 75], [1, 100]];
export const EDGE_WEIGHTS = { sqn: 0.35, pf: 0.25, pfTop3: 0.15, expectancy: 0.25 } as const;
export const EDGE_BOOT_PATHS = 2000;
export const EDGE_BOOT_LOSS_MAX = 10; // % of paths ending negative
export const EDGE_BOOT_CAP = 60;
export const EDGE_SHRINK_N = 100;

export const RISK_LOSS_TAIL: Anchors = [[2, 100], [12, 0]];
export const RISK_WORST_DAY: Anchors = [[1, 100], [3, 60], [6, 20], [10, 0]];
export const RISK_RECOVERY: Anchors = [[0, 0], [1, 40], [2, 70], [3.5, 100]];
export const RISK_UNDERWATER: Anchors = [[0.3, 100], [0.9, 5]];
export const RISK_WEIGHTS = { lossTail: 0.3, worstDay: 0.25, recovery: 0.25, underwater: 0.2 } as const;

export const CONS_BEST_DAY: Anchors = [[0.2, 100], [1.0, 0]];
export const CONS_GREEN: Anchors = [[0.3, 0], [0.45, 40], [0.6, 75], [0.75, 100]];
export const CONS_EFFECTIVE: Anchors = [[0.2, 0], [0.7, 100]];
export const CONS_DAILY_VAR: Anchors = [[0.05, 100], [0.5, 0]];
export const CONS_CURVE_FLAT = 5;
export const CONS_WEIGHTS = { bestDay: 0.3, curve: 0.25, green: 0.2, effective: 0.15, dailyVar: 0.1 } as const;

export const DISC_REVENGE: Anchors = [[0.1, 100], [0.6, 0]];
export const DISC_OVERTRADING: Anchors = [[0, 100], [0.4, 0]];
export const DISC_ESCALATION: Anchors = [[0.8, 100], [1.0, 70], [1.5, 0]];
export const DISC_STREAK: Anchors = [[1, 100], [2, 0]];
export const DISC_PAST_STOP: Anchors = [[0, 100], [0.6, 0]];
export const DISC_WEIGHTS = { revenge: 0.3, overtrading: 0.25, streaks: 0.2, pastStop: 0.25 } as const;
export const REVENGE_MIN_OPPORTUNITIES = 10;
export const REVENGE_MIN_TIMED_SHARE = 0.5;
export const REVENGE_DECILE = 10;
export const OVERTRADING_MIN_N = 20;
export const ESCALATION_MIN_PAIRS = 5;
export const PAST_STOP_QUANTILES = [0.3, 0.5, 0.7, 0.85] as const;
export const PAST_STOP_ROUND = 25;
export const PAST_STOP_COLLAPSE_P = 0.2;
export const PAST_STOP_MIN_DAYS = 5;
export const PAST_STOP_MIN_RED = 5;

export const BRIEF_HOUR_MIN_SHOW = 5;
export const BRIEF_HOUR_MIN_RANK = 20;
export const BRIEF_AFTER_LOSS_MIN = 20;
export const BRIEF_LEAKS = 3;

// ---------------------------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------------------------

export interface HistoryTrade {
  d: string;
  t?: string | null;
  pnl: number;
  ed?: string | null;
}

export interface Part {
  value: number | null;
  score: number | null;
  weight: number;
  note?: string;
}

export interface Pillar {
  score: number;
  parts: Record<string, Part>;
  notes: string[];
}

export interface HourBin {
  hour: number;
  n: number;
  net: number;
  winRate: number;
}
export interface AfterLossRow {
  k: string;
  n: number;
  net: number;
  winRate: number;
}
export interface Leak {
  name: string;
  dollars: number;
  n: number;
}

export interface Briefing {
  hourBins: HourBin[];
  bestHour: HourBin | null;
  worstHour: HourBin | null;
  afterLosses: AfterLossRow[];
  giveBack: { days: number; keptHalfPct: number; medianPeak: number } | null;
  leaks: Leak[];
}

export type TradingScore =
  | { status: 'collecting'; n: number; days: number; needTrades: number; needDays: number }
  | {
      status: 'ready';
      overall: number;
      letter: string;
      tier: 'Provisional' | 'Developing' | 'Established';
      n: number;
      days: number;
      net: number;
      edge: Pillar;
      risk: Pillar;
      consistency: Pillar;
      discipline: Pillar;
      briefing: Briefing;
    };

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

function weighted(parts: Record<string, Part>): number {
  let s = 0;
  let w = 0;
  for (const p of Object.values(parts)) {
    if (p.score == null) continue;
    s += p.score * p.weight;
    w += p.weight;
  }
  return w > 0 ? s / w : 0;
}

function part(value: number | null, anchors: Anchors, weight: number, note?: string): Part {
  return { value: value == null ? null : round(value, 3), score: value == null ? null : round(band(value, anchors)), weight, note };
}

function dayStats(days: readonly DayBlock[]) {
  const nets = days.map((d) => d.net);
  const green = nets.filter((x) => x > 0);
  return { nets, green, red: nets.filter((x) => x < 0) };
}

/** Running minimum (low) and close of each day, plus per-trade cumulative. */
function dayProfile(day: DayBlock): { low: number; close: number; peak: number; cum: number[] } {
  let c = 0;
  let low = 0;
  let peak = 0;
  const cum: number[] = [];
  for (const t of day.trades) {
    c += t.pnl;
    cum.push(c);
    if (c < low) low = c;
    if (c > peak) peak = c;
  }
  return { low, close: c, peak, cum };
}

export function scoreLetter(x: number): string {
  for (const [min, l] of SCORE_LETTERS) if (x >= min) return l;
  return 'F';
}

export function scoreTier(n: number): 'Provisional' | 'Developing' | 'Established' {
  return n < TIER_DEVELOPING ? 'Provisional' : n < TIER_ESTABLISHED ? 'Developing' : 'Established';
}

// ---------------------------------------------------------------------------------------------
// Pillars
// ---------------------------------------------------------------------------------------------

export function edgePillar(pnls: readonly number[]): Pillar {
  const st = basicStats(pnls);
  const { r, avgLoss } = rMultiples(pnls);
  const s = sqn(r);
  const wins = [...pnls].filter((p) => p > 0).sort((a, b) => b - a);
  const top3 = new Set<number>();
  const withoutTop3 = [...pnls];
  for (const w of wins.slice(0, 3)) {
    const idx = withoutTop3.indexOf(w);
    if (idx >= 0) {
      withoutTop3.splice(idx, 1);
      top3.add(w);
    }
  }
  const pfTop3 = basicStats(withoutTop3).profitFactor;
  const expectancyRatio = avgLoss > 0 ? st.expectancy / avgLoss : 0;
  const parts: Record<string, Part> = {
    sqn: part(s, EDGE_SQN, EDGE_WEIGHTS.sqn),
    pf: part(st.profitFactor, EDGE_PF, EDGE_WEIGHTS.pf),
    pfWithoutTop3: part(pfTop3, EDGE_PF, EDGE_WEIGHTS.pfTop3),
    expectancyRatio: part(expectancyRatio, EDGE_EXPECTANCY, EDGE_WEIGHTS.expectancy),
  };
  const notes: string[] = [];
  let edge = weighted(parts);
  const boot = monteCarloTrades(pnls, EDGE_BOOT_PATHS);
  const bootLossShare = boot.lossShare;
  parts.bootLossShare = { value: round(bootLossShare), score: null, weight: 0 };
  if (bootLossShare > EDGE_BOOT_LOSS_MAX && edge > EDGE_BOOT_CAP) {
    edge = EDGE_BOOT_CAP;
    notes.push(`Bootstrap: ${round(bootLossShare)}% of resampled histories end negative → edge capped at ${EDGE_BOOT_CAP}.`);
  }
  const shrink = Math.min(1, pnls.length / EDGE_SHRINK_N);
  edge = 50 + (edge - 50) * shrink;
  if (shrink < 1) notes.push(`Sample shrink ×${round(shrink, 2)} towards 50 (n=${pnls.length}).`);
  return { score: round(edge), parts, notes };
}

export function riskPillar(pnls: readonly number[], days: readonly DayBlock[]): Pillar {
  const st = basicStats(pnls);
  const { green, nets } = dayStats(days);
  const worstDay = Math.min(0, ...nets);
  const avgGreen = green.length ? mean(green) : 0;
  const lossTail = st.avgLoss > 0 ? st.maxLoss / st.avgLoss : 0;
  const worstDayDamage = worstDay >= 0 ? 0 : avgGreen > 0 ? -worstDay / avgGreen : RISK_WORST_DAY[RISK_WORST_DAY.length - 1][0];
  const dd = maxDrawdown(pnls);
  const recovery = dd > 0 ? st.net / dd : st.net > 0 ? RISK_RECOVERY[RISK_RECOVERY.length - 1][0] : 0;
  let under = 0;
  let eq = 0;
  let peak = 0;
  for (const p of pnls) {
    eq += p;
    if (eq > peak) peak = eq;
    else if (eq < peak) under++;
  }
  const underwaterShare = pnls.length ? under / pnls.length : 0;
  const parts: Record<string, Part> = {
    lossTail: part(lossTail, RISK_LOSS_TAIL, RISK_WEIGHTS.lossTail),
    worstDayDamage: part(worstDayDamage, RISK_WORST_DAY, RISK_WEIGHTS.worstDay),
    recovery: part(recovery, RISK_RECOVERY, RISK_WEIGHTS.recovery),
    underwaterShare: part(underwaterShare, RISK_UNDERWATER, RISK_WEIGHTS.underwater),
  };
  return { score: round(weighted(parts)), parts, notes: [] };
}

export function consistencyPillar(days: readonly DayBlock[]): Pillar {
  const { nets, green } = dayStats(days);
  const net = nets.reduce((a, b) => a + b, 0);
  const cum: number[] = [];
  let e = 0;
  for (const x of nets) {
    e += x;
    cum.push(e);
  }
  const fit = linearFit(cum);
  const curveScore = fit.slope <= 0 ? CONS_CURVE_FLAT : round(fit.r2 * 100);
  const greenRate = days.length ? green.length / days.length : 0;
  const notes: string[] = [];
  if (net <= 0) {
    const parts: Record<string, Part> = {
      curve: { value: round(fit.r2, 3), score: curveScore, weight: 0.5 },
      greenRate: part(greenRate, CONS_GREEN, 0.5),
    };
    notes.push('Net ≤ 0: consistency uses only curve and green-day rate.');
    return { score: round(weighted(parts)), parts, notes };
  }
  const bestDay = Math.max(...nets);
  const bestDayShare = bestDay / net;
  const sumGreen = green.reduce((a, b) => a + b, 0);
  let hhi = 0;
  for (const g of green) hhi += (g / sumGreen) ** 2;
  const effective = hhi > 0 ? 1 / hhi : 0;
  const effectiveRatio = green.length ? effective / green.length : 0;
  const dailyVar = sd(nets) / Math.abs(net);
  const parts: Record<string, Part> = {
    bestDayShare: part(bestDayShare, CONS_BEST_DAY, CONS_WEIGHTS.bestDay),
    curve: { value: round(fit.r2, 3), score: curveScore, weight: CONS_WEIGHTS.curve },
    greenRate: part(greenRate, CONS_GREEN, CONS_WEIGHTS.green),
    effectiveDays: part(effectiveRatio, CONS_EFFECTIVE, CONS_WEIGHTS.effective),
    dailyVar: part(dailyVar, CONS_DAILY_VAR, CONS_WEIGHTS.dailyVar),
  };
  return { score: round(weighted(parts)), parts, notes };
}

interface RevengeInfo {
  rate: number | null;
  opportunities: number;
  revengeNet: number;
  revengeN: number;
  note?: string;
}

export function revengeAnalysis(days: readonly DayBlock[]): RevengeInfo {
  const all: Trade[] = [];
  for (const d of days) all.push(...d.trades);
  const timed = all.filter((t) => t.minute != null).length;
  if (!all.length || timed / all.length < REVENGE_MIN_TIMED_SHARE) {
    return { rate: null, opportunities: 0, revengeNet: 0, revengeN: 0, note: 'Fewer than 50% of trades have a time stamp.' };
  }
  const gaps: number[] = [];
  for (const d of days) {
    for (let i = 1; i < d.trades.length; i++) {
      const a = d.trades[i - 1].minute;
      const b = d.trades[i].minute;
      if (a != null && b != null && b >= a) gaps.push(b - a);
    }
  }
  if (!gaps.length) return { rate: null, opportunities: 0, revengeNet: 0, revengeN: 0, note: 'No intraday gaps to measure.' };
  const medGap = median(gaps);
  const losses = sortAsc(all.filter((t) => t.pnl < 0).map((t) => t.pnl));
  if (!losses.length) return { rate: null, opportunities: 0, revengeNet: 0, revengeN: 0, note: 'No losing trades.' };
  const threshold = percentile(losses, REVENGE_DECILE);
  let opp = 0;
  let rev = 0;
  let revNet = 0;
  for (const d of days) {
    for (let i = 0; i + 1 < d.trades.length; i++) {
      const t = d.trades[i];
      const nx = d.trades[i + 1];
      if (t.pnl > threshold || t.minute == null || nx.minute == null) continue;
      opp++;
      if (nx.minute - t.minute <= medGap / 2) {
        rev++;
        revNet += nx.pnl;
      }
    }
  }
  if (opp < REVENGE_MIN_OPPORTUNITIES) {
    return { rate: null, opportunities: opp, revengeNet: revNet, revengeN: rev, note: `Only ${opp} worst-decile losses with a follow-up trade (need ${REVENGE_MIN_OPPORTUNITIES}).` };
  }
  return { rate: rev / opp, opportunities: opp, revengeNet: revNet, revengeN: rev };
}

interface OvertradingInfo {
  slot: number | null;
  shareBeyond: number;
  netBeyond: number;
  nBeyond: number;
}

export function overtradingAnalysis(days: readonly DayBlock[]): OvertradingInfo {
  const maxSlot = Math.max(0, ...days.map((d) => d.trades.length));
  let slot: number | null = null;
  for (let k = 1; k <= maxSlot; k++) {
    const xs: number[] = [];
    for (const d of days) if (d.trades.length >= k) xs.push(d.trades[k - 1].pnl);
    if (xs.length >= OVERTRADING_MIN_N && mean(xs) < 0) {
      slot = k;
      break;
    }
  }
  if (slot == null) return { slot: null, shareBeyond: 0, netBeyond: 0, nBeyond: 0 };
  let n = 0;
  let nb = 0;
  let net = 0;
  for (const d of days) {
    d.trades.forEach((t, i) => {
      n++;
      if (i + 1 >= (slot as number)) {
        nb++;
        net += t.pnl;
      }
    });
  }
  return { slot, shareBeyond: n ? nb / n : 0, netBeyond: net, nBeyond: nb };
}

interface StreakInfo {
  escalation: number | null;
  worstStreak: number;
  expStreak: number | null;
  streakRatio: number | null;
}

export function streakAnalysis(pnls: readonly number[]): StreakInfo {
  const ratios: number[] = [];
  let worst = 0;
  let cur = 0;
  for (let i = 0; i < pnls.length; i++) {
    if (pnls[i] < 0) {
      cur++;
      if (cur > worst) worst = cur;
      if (i > 0 && pnls[i - 1] < 0) ratios.push(Math.abs(pnls[i]) / Math.abs(pnls[i - 1]));
    } else cur = 0;
  }
  const st = basicStats(pnls);
  const wr = st.winRate;
  const expStreak = wr > 0 && wr < 1 && pnls.length > 1 ? Math.log(pnls.length) / Math.log(1 / (1 - wr)) : null;
  return {
    escalation: ratios.length >= ESCALATION_MIN_PAIRS ? median(ratios) : null,
    worstStreak: worst,
    expStreak,
    streakRatio: expStreak != null && expStreak > 0 ? worst / expStreak : null,
  };
}

interface PastStopInfo {
  levels: { x: number; days: number; pGreen: number }[];
  collapseX: number | null;
  shareWorse: number | null;
  damage: number;
  worseDays: number;
}

export function pastStopAnalysis(days: readonly DayBlock[]): PastStopInfo {
  const profiles = days.map(dayProfile);
  const red = sortAsc(profiles.filter((p) => p.close < 0).map((p) => -p.close));
  if (red.length < PAST_STOP_MIN_RED) return { levels: [], collapseX: null, shareWorse: null, damage: 0, worseDays: 0 };
  const xs = [...new Set(PAST_STOP_QUANTILES.map((q) => Math.max(PAST_STOP_ROUND, Math.round(percentile(red, q * 100) / PAST_STOP_ROUND) * PAST_STOP_ROUND)))].sort((a, b) => a - b);
  const levels: { x: number; days: number; pGreen: number }[] = [];
  let collapseX: number | null = null;
  for (const x of xs) {
    const hit = profiles.filter((p) => p.low <= -x);
    if (hit.length < PAST_STOP_MIN_DAYS) continue;
    const pGreen = hit.filter((p) => p.close > 0).length / hit.length;
    levels.push({ x, days: hit.length, pGreen: round(pGreen, 3) });
    if (collapseX == null && pGreen <= PAST_STOP_COLLAPSE_P) collapseX = x;
  }
  if (collapseX == null) return { levels, collapseX: null, shareWorse: null, damage: 0, worseDays: 0 };
  let worse = 0;
  let total = 0;
  let damage = 0;
  for (const p of profiles) {
    if (p.low > -collapseX) continue;
    total++;
    const cross = p.cum.findIndex((c) => c <= -collapseX);
    if (cross < 0 || cross === p.cum.length - 1) continue; // stopped at the crossing
    const atCross = p.cum[cross];
    if (p.close < atCross) {
      worse++;
      damage += p.close - atCross;
    }
  }
  return { levels, collapseX, shareWorse: total ? worse / total : 0, damage, worseDays: worse };
}

export function disciplinePillar(days: readonly DayBlock[], pnls: readonly number[]) {
  const notes: string[] = [];
  const rev = revengeAnalysis(days);
  const over = overtradingAnalysis(days);
  const streak = streakAnalysis(pnls);
  const past = pastStopAnalysis(days);

  const parts: Record<string, Part> = {};
  parts.revenge = part(rev.rate, DISC_REVENGE, DISC_WEIGHTS.revenge, rev.note);
  if (rev.note) notes.push(`Revenge: ${rev.note}`);
  parts.overtrading = over.slot == null
    ? { value: 0, score: 100, weight: DISC_WEIGHTS.overtrading, note: 'No intraday slot with n≥20 and negative mean.' }
    : part(over.shareBeyond, DISC_OVERTRADING, DISC_WEIGHTS.overtrading, `Slot ${over.slot}+ loses on average.`);
  const streakScores: number[] = [];
  if (streak.escalation != null) streakScores.push(band(streak.escalation, DISC_ESCALATION));
  if (streak.streakRatio != null) streakScores.push(band(streak.streakRatio, DISC_STREAK));
  parts.streaks = {
    value: streak.streakRatio == null ? null : round(streak.streakRatio, 3),
    score: streakScores.length ? round(mean(streakScores)) : null,
    weight: DISC_WEIGHTS.streaks,
    note: `escalation=${streak.escalation == null ? 'n/a' : round(streak.escalation, 2)}, worst streak ${streak.worstStreak} vs expected ${streak.expStreak == null ? 'n/a' : round(streak.expStreak, 1)}`,
  };
  if (past.collapseX == null) {
    parts.pastStop = { value: null, score: past.levels.length ? 100 : null, weight: DISC_WEIGHTS.pastStop, note: past.levels.length ? 'No collapse level (recovery odds stay above 20%).' : 'Too few red days.' };
  } else {
    parts.pastStop = part(past.shareWorse, DISC_PAST_STOP, DISC_WEIGHTS.pastStop, `Collapse at −${past.collapseX}: ${past.worseDays} days kept trading and got worse.`);
  }
  const pillar: Pillar = { score: round(weighted(parts)), parts, notes };
  return { pillar, rev, over, streak, past };
}

// ---------------------------------------------------------------------------------------------
// Briefing
// ---------------------------------------------------------------------------------------------

export function briefing(days: readonly DayBlock[], pnls: readonly number[], extra: ReturnType<typeof disciplinePillar>): Briefing {
  const all: Trade[] = [];
  for (const d of days) all.push(...d.trades);
  const byHour = new Map<number, { n: number; net: number; wins: number }>();
  for (const t of all) {
    if (t.hour == null) continue;
    let b = byHour.get(t.hour);
    if (!b) {
      b = { n: 0, net: 0, wins: 0 };
      byHour.set(t.hour, b);
    }
    b.n++;
    b.net += t.pnl;
    if (t.pnl > 0) b.wins++;
  }
  const hourBins: HourBin[] = [...byHour.entries()]
    .filter(([, b]) => b.n >= BRIEF_HOUR_MIN_SHOW)
    .map(([hour, b]) => ({ hour, n: b.n, net: round(b.net, 2), winRate: round((b.wins / b.n) * 100) }))
    .sort((a, b) => a.hour - b.hour);
  const ranked = hourBins.filter((h) => h.n >= BRIEF_HOUR_MIN_RANK);
  let bestHour: HourBin | null = null;
  let worstHour: HourBin | null = null;
  for (const h of ranked) {
    if (!bestHour || h.net > bestHour.net) bestHour = h;
    if (!worstHour || h.net < worstHour.net) worstHour = h;
  }

  const after = new Map<string, { n: number; net: number; wins: number }>();
  const bump = (k: string, p: number) => {
    let b = after.get(k);
    if (!b) {
      b = { n: 0, net: 0, wins: 0 };
      after.set(k, b);
    }
    b.n++;
    b.net += p;
    if (p > 0) b.wins++;
  };
  let streak = 0;
  for (const p of pnls) {
    if (streak >= 3) bump('3+', p);
    else if (streak > 0) bump(String(streak), p);
    streak = p < 0 ? streak + 1 : 0;
  }
  const afterLosses: AfterLossRow[] = ['1', '2', '3+']
    .map((k) => ({ k, b: after.get(k) }))
    .filter((x): x is { k: string; b: { n: number; net: number; wins: number } } => !!x.b && x.b.n >= BRIEF_AFTER_LOSS_MIN)
    .map(({ k, b }) => ({ k, n: b.n, net: round(b.net, 2), winRate: round((b.wins / b.n) * 100) }));

  const profiles = days.map(dayProfile);
  const peaks = profiles.filter((p) => p.peak > 0).map((p) => p.peak);
  let giveBack: Briefing['giveBack'] = null;
  if (peaks.length) {
    const medPeak = median(peaks);
    const big = profiles.filter((p) => p.peak > 0 && p.peak >= medPeak);
    const kept = big.filter((p) => p.close >= p.peak / 2).length;
    giveBack = { days: big.length, keptHalfPct: big.length ? round((kept / big.length) * 100) : 0, medianPeak: round(medPeak, 2) };
  }

  const leaks: Leak[] = [];
  if (worstHour && worstHour.net < 0) leaks.push({ name: `Hour ${String(worstHour.hour).padStart(2, '0')}:00`, dollars: worstHour.net, n: worstHour.n });
  else {
    const w = hourBins.filter((h) => h.net < 0).sort((a, b) => a.net - b.net)[0];
    if (w) leaks.push({ name: `Hour ${String(w.hour).padStart(2, '0')}:00`, dollars: w.net, n: w.n });
  }
  if (extra.over.slot != null && extra.over.netBeyond < 0) leaks.push({ name: `Trades from slot ${extra.over.slot} on`, dollars: round(extra.over.netBeyond, 2), n: extra.over.nBeyond });
  if (extra.rev.revengeN > 0 && extra.rev.revengeNet < 0) leaks.push({ name: 'Revenge re-entries', dollars: round(extra.rev.revengeNet, 2), n: extra.rev.revengeN });
  const a3 = after.get('3+');
  if (a3 && a3.net < 0) leaks.push({ name: 'After 3+ losses', dollars: round(a3.net, 2), n: a3.n });
  if (extra.past.damage < 0) leaks.push({ name: `Past −${extra.past.collapseX} and kept going`, dollars: round(extra.past.damage, 2), n: extra.past.worseDays });
  leaks.sort((a, b) => a.dollars - b.dollars);

  return { hourBins, bestHour, worstHour, afterLosses, giveBack, leaks: leaks.slice(0, BRIEF_LEAKS) };
}

// ---------------------------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------------------------

export function computeTradingScore(history: readonly HistoryTrade[]): TradingScore {
  const raw = history.filter((h) => h && typeof h.d === 'string' && Number.isFinite(Number(h.pnl)));
  const dayCount = new Set(raw.map((h) => h.d)).size;
  if (raw.length < SCORE_MIN_TRADES || dayCount < SCORE_MIN_DAYS) {
    return { status: 'collecting', n: raw.length, days: dayCount, needTrades: SCORE_MIN_TRADES, needDays: SCORE_MIN_DAYS };
  }
  const trades = normalizeTrades({ trades: raw.map((h) => ({ d: h.d, t: h.t ?? null, pnl: Number(h.pnl), ed: h.ed ?? null })), market: 'futures', contractsInBacktest: 1 });
  const days = groupDays(trades);
  const pnls = trades.map((t) => t.pnl);
  const edge = edgePillar(pnls);
  const risk = riskPillar(pnls, days);
  const consistency = consistencyPillar(days);
  const disc = disciplinePillar(days, pnls);
  const overall = Math.round(
    SCORE_WEIGHTS.edge * edge.score + SCORE_WEIGHTS.risk * risk.score + SCORE_WEIGHTS.consistency * consistency.score + SCORE_WEIGHTS.discipline * disc.pillar.score,
  );
  return {
    status: 'ready',
    overall,
    letter: scoreLetter(overall),
    tier: scoreTier(trades.length),
    n: trades.length,
    days: days.length,
    net: round(pnls.reduce((a, b) => a + b, 0), 2),
    edge,
    risk,
    consistency,
    discipline: disc.pillar,
    briefing: briefing(days, pnls, disc),
  };
}
