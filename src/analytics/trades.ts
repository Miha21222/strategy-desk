/**
 * Trade normalization and day blocks.
 *
 * Futures: `pnl` is divided by `contractsInBacktest`, so every downstream number is per ONE unit of
 * `backtestUnit` (mini or micro). Forex / CFD: `r` is used when present, otherwise `pnl` as-is.
 */
import type { Prodata, RawTrade } from './types';

export const MIN_TRADES = 10;

export interface Trade {
  /** Exit date YYYY-MM-DD. */
  d: string;
  /** Entry date or null. */
  ed: string | null;
  /** Entry time HH:MM or null. */
  t: string | null;
  /** Entry hour 0–23 or null. */
  hour: number | null;
  /** Minute of day (hour*60+min) or null. */
  minute: number | null;
  /** Weekday of exit date, 0 = Sunday … 6 = Saturday. */
  dow: number;
  /** Normalized P&L (per 1 unit for futures; R or raw pnl for forex/cfd). */
  pnl: number;
  /** R-multiple when supplied. */
  r: number | null;
  /** Original pnl as emitted. */
  raw: number;
  /** Position in the original list. */
  i: number;
}

export interface DayBlock {
  d: string;
  trades: Trade[];
  net: number;
}

export class AnalyticsError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AnalyticsError';
  }
}

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
const TIME_RE = /^(\d{1,2}):(\d{2})/;

export function weekday(d: string): number {
  const dt = new Date(`${d}T00:00:00Z`);
  const w = dt.getUTCDay();
  return Number.isNaN(w) ? 0 : w;
}

export function parseTime(t: string | null | undefined): { hour: number; minute: number } | null {
  if (!t) return null;
  const m = TIME_RE.exec(t.trim());
  if (!m) return null;
  const h = Number(m[1]);
  const mi = Number(m[2]);
  if (!Number.isFinite(h) || !Number.isFinite(mi) || h < 0 || h > 23 || mi < 0 || mi > 59) return null;
  return { hour: h, minute: h * 60 + mi };
}

/** Normalize the raw trade list from a prodata block. Throws AnalyticsError on bad input. */
export function normalizeTrades(prodata: Pick<Prodata, 'trades' | 'market' | 'contractsInBacktest'>): Trade[] {
  const raw = Array.isArray(prodata.trades) ? prodata.trades : [];
  if (raw.length < MIN_TRADES) {
    throw new AnalyticsError(`Need at least ${MIN_TRADES} trades, got ${raw.length}`);
  }
  const contracts = prodata.contractsInBacktest > 0 ? prodata.contractsInBacktest : 1;
  const useR = prodata.market === 'forex' || prodata.market === 'cfd';
  const out: Trade[] = [];
  raw.forEach((rt: RawTrade, i) => {
    if (!rt || typeof rt.d !== 'string' || !DATE_RE.test(rt.d)) {
      throw new AnalyticsError(`Trade #${i + 1}: missing or invalid exit date`);
    }
    const pnlRaw = Number(rt.pnl);
    if (!Number.isFinite(pnlRaw)) throw new AnalyticsError(`Trade #${i + 1}: pnl is not a number`);
    const r = rt.r == null || !Number.isFinite(Number(rt.r)) ? null : Number(rt.r);
    let pnl: number;
    if (useR) pnl = r ?? pnlRaw;
    else pnl = pnlRaw / contracts;
    const tm = parseTime(rt.t);
    out.push({
      d: rt.d,
      ed: typeof rt.ed === 'string' && DATE_RE.test(rt.ed) ? rt.ed : null,
      t: tm ? rt.t!.trim() : null,
      hour: tm ? tm.hour : null,
      minute: tm ? tm.minute : null,
      dow: weekday(rt.d),
      pnl,
      r,
      raw: pnlRaw,
      i,
    });
  });
  // Stable sort by exit date, then entry minute, then original order.
  out.sort((a, b) => (a.d < b.d ? -1 : a.d > b.d ? 1 : (a.minute ?? -1) - (b.minute ?? -1) || a.i - b.i));
  return out;
}

/** Group trades into day blocks keyed by exit date (input order preserved inside a day). */
export function groupDays(trades: readonly Trade[]): DayBlock[] {
  const map = new Map<string, DayBlock>();
  for (const t of trades) {
    let b = map.get(t.d);
    if (!b) {
      b = { d: t.d, trades: [], net: 0 };
      map.set(t.d, b);
    }
    b.trades.push(t);
    b.net += t.pnl;
  }
  return [...map.values()].sort((a, b) => (a.d < b.d ? -1 : a.d > b.d ? 1 : 0));
}

/** Basic descriptive statistics of a P&L series. */
export interface BasicStats {
  n: number;
  net: number;
  grossWin: number;
  grossLoss: number;
  wins: number;
  losses: number;
  winRate: number;
  profitFactor: number;
  avgWin: number;
  avgLoss: number;
  expectancy: number;
  maxWin: number;
  maxLoss: number;
}

export const PF_CAP = 99;

export function basicStats(pnls: readonly number[]): BasicStats {
  let net = 0;
  let gw = 0;
  let gl = 0;
  let wins = 0;
  let losses = 0;
  let maxWin = 0;
  let maxLoss = 0;
  for (const p of pnls) {
    net += p;
    if (p > 0) {
      gw += p;
      wins++;
      if (p > maxWin) maxWin = p;
    } else if (p < 0) {
      gl += -p;
      losses++;
      if (-p > maxLoss) maxLoss = -p;
    }
  }
  const n = pnls.length;
  return {
    n,
    net,
    grossWin: gw,
    grossLoss: gl,
    wins,
    losses,
    winRate: n ? wins / n : 0,
    profitFactor: gl > 0 ? Math.min(PF_CAP, gw / gl) : gw > 0 ? PF_CAP : 0,
    avgWin: wins ? gw / wins : 0,
    avgLoss: losses ? gl / losses : 0,
    expectancy: n ? net / n : 0,
    maxWin,
    maxLoss,
  };
}
