import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { mulberry32 } from '../rng';
import type { PropFirmsData, FuturesAccount, ForexAccount, AccountRef, Firm } from '../rules';
import type { Prodata, RawTrade } from '../types';
import { groupDays, normalizeTrades, type DayBlock } from '../trades';

export function loadRules(): PropFirmsData {
  const p = resolve(__dirname, '../../../data/prop-firms.json');
  return JSON.parse(readFileSync(p, 'utf8')) as PropFirmsData;
}

/** YYYY-MM-DD for weekday number `i` (skips weekends) starting 2025-01-06 (Monday). */
export function dateAt(i: number): string {
  const base = Date.UTC(2025, 0, 6);
  const weeks = Math.floor(i / 5);
  const dow = i % 5;
  const d = new Date(base + (weeks * 7 + dow) * 86400000);
  return d.toISOString().slice(0, 10);
}

export interface SynthOptions {
  days: number;
  perDay?: number;
  winRate?: number;
  win?: number;
  loss?: number;
  seed?: number;
  timed?: boolean;
}

/** Synthetic raw trades with fixed win/loss sizes and a seeded coin. */
export function synthTrades(o: SynthOptions): RawTrade[] {
  const rng = mulberry32(o.seed ?? 1);
  const perDay = o.perDay ?? 3;
  const out: RawTrade[] = [];
  for (let d = 0; d < o.days; d++) {
    for (let k = 0; k < perDay; k++) {
      const win = rng() < (o.winRate ?? 0.55);
      const pnl = win ? (o.win ?? 100) : -(o.loss ?? 80);
      const hour = 9 + k;
      out.push({ d: dateAt(d), t: o.timed === false ? null : `${String(hour).padStart(2, '0')}:30`, pnl, r: pnl / (o.loss ?? 80) });
    }
  }
  return out;
}

export function prodataOf(trades: RawTrade[], extra: Partial<Prodata> = {}): Prodata {
  return {
    strategy: 'test',
    version: 'v1',
    market: 'futures',
    contractsInBacktest: 1,
    backtestUnit: 'micro',
    pointValue: 5,
    accountSize: 10000,
    trades,
    firms: null,
    focus: 'survival',
    gameplan: null,
    source: 'backtest',
    newsDates: null,
    daily: null,
    ...extra,
  };
}

export function daysOf(pnlsPerDay: number[][]): DayBlock[] {
  const raw: RawTrade[] = [];
  pnlsPerDay.forEach((day, i) => day.forEach((pnl, k) => raw.push({ d: dateAt(i), t: `${String(9 + k).padStart(2, '0')}:00`, pnl })));
  // normalizeTrades needs ≥10 trades; pad requirement by allowing direct construction when short.
  if (raw.length >= 10) return groupDays(normalizeTrades({ trades: raw, market: 'futures', contractsInBacktest: 1 }));
  return pnlsPerDay.map((day, i) => ({
    d: dateAt(i),
    net: day.reduce((a, b) => a + b, 0),
    trades: day.map((pnl, k) => ({ d: dateAt(i), ed: null, t: null, hour: 9 + k, minute: (9 + k) * 60, dow: 1, pnl, r: null, raw: pnl, i: k })),
  }));
}

export const FUTURES_FIRM: Firm = { id: 'testfirm', name: 'Test Firm', family: 'futures', maxFundedAccounts: 5, accounts: [] };
export const FOREX_FIRM: Firm = { id: 'fxfirm', name: 'FX Firm', family: 'forex', maxFundedAccounts: null, accounts: [] };

export function futuresAccount(over: Partial<FuturesAccount> = {}): AccountRef {
  const account: FuturesAccount = {
    id: 'test-50k',
    label: 'Test 50K',
    accountSize: 50000,
    profitTarget: 3000,
    drawdown: { amount: 2000, type: 'intraday', includesUnrealized: true, stopsTrailing: { lockAtStartPlus: 100 } },
    dailyLossLimit: null,
    maxContracts: 5,
    minTradingDays: 1,
    consistencyMaxDayPct: null,
    cost: { structure: 'onetime', evalFull: 100 },
    funded: { bufferAmount: 2100, minDaysToFirstPayout: 5, payoutFrequencyDays: 14, minPayout: 500, maxPayoutPct: 90, activationFee: 0 },
    ...over,
  };
  return { firm: { ...FUTURES_FIRM, accounts: [account] }, account };
}

export function forexAccount(over: Partial<ForexAccount> = {}): AccountRef {
  const account: ForexAccount = {
    id: 'fx-10k',
    label: 'FX 10K',
    accountSize: 10000,
    phases: [
      { name: 'Phase 1', targetPct: 8, minTradingDays: 3 },
      { name: 'Phase 2', targetPct: 5, minTradingDays: 3 },
    ],
    dailyLoss: { pct: 5, basis: 'max(balance,equity) at day start' },
    maxLoss: { pct: 10, type: 'static' },
    consistencyMaxDayPct: null,
    cost: { structure: 'onetime', evalFull: 66 },
    funded: { profitSplitByCycle: { weekly: 60, biweekly: 80 }, firstPayoutDays: 14, payoutCycleDays: 14 },
    ...over,
  };
  return { firm: { ...FOREX_FIRM, accounts: [account] }, account };
}
