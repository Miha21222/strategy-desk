import { describe, expect, it } from 'vitest';
import { attemptCost, makeDayPaths, periodsFor, runSurvival, simulateForexPath, simulateFuturesPath, forexParams, valueScoreOf } from '../propsim';
import { daysOf, forexAccount, futuresAccount, prodataOf, synthTrades } from './helpers';
import { groupDays, normalizeTrades } from '../trades';
import type { ForexAccount, FuturesAccount } from '../rules';

const seq = (n: number, i = 0) => Array.from({ length: n }, () => i);

describe('futures evaluation', () => {
  it('100% winners pass on every path without a breach', () => {
    const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 20, perDay: 2, winRate: 1, win: 150 }))));
    const ref = futuresAccount();
    const r = runSurvival(days, ref, { paths: 200, unit: 'micro', sizes: [5, 10] });
    for (const s of r.sizes) {
      expect(s.passPct).toBe(100);
      expect(s.breachTrailingPct).toBe(0);
      expect(s.breachDailyPct).toBe(0);
      expect(s.daysToPass?.p50).toBe(Math.ceil(3000 / (150 * 2 * s.size)));
    }
    expect(r.recommended).not.toBeNull();
  });

  it('crafted losing streak breaches the trailing drawdown on the expected day', () => {
    const days = daysOf([[-100]]);
    const acc = futuresAccount({ drawdown: { amount: 1500, type: 'intraday', stopsTrailing: null } }).account as FuturesAccount;
    const o = simulateFuturesPath(days, seq(60), acc, 1, 60);
    expect(o.status).toBe('breach_trailing');
    expect(o.days).toBe(15);
    expect(o.finalEq).toBe(-1500);
  });

  it('trailing floor locks at start+lock once the hwm is high enough', () => {
    // +2000 day then a slow bleed: floor = min(hwm-2000, 100) = 0 after day 1.
    const days = daysOf([[2000], [-100]]);
    const acc = futuresAccount({ profitTarget: 99999 }).account as FuturesAccount;
    const path = [0, ...seq(60, 1)];
    const o = simulateFuturesPath(days, path, acc, 1, 60);
    expect(o.status).toBe('breach_trailing');
    expect(o.days).toBe(21); // 2000 → 0 over 20 losing days
  });

  it('breaches the daily loss limit inside a day', () => {
    const days = daysOf([[-300, -300, 500]]);
    const acc = futuresAccount({ dailyLossLimit: 500 }).account as FuturesAccount;
    const o = simulateFuturesPath(days, seq(5), acc, 1, 60);
    expect(o.status).toBe('breach_daily');
    expect(o.days).toBe(1);
  });

  it('stalls on the consistency rule when the best day dominates', () => {
    const days = daysOf([[900], [20]]);
    const acc = futuresAccount({ profitTarget: 1000, consistencyMaxDayPct: 50, cost: { structure: 'per30d', windowDays: 30, evalFull: 100, renewal: 100 } }).account as FuturesAccount;
    const path = [0, ...seq(40, 1)];
    const o = simulateFuturesPath(days, path, acc, 1, 21);
    expect(o.status).toBe('consistency_stall');
    // Same rules without the consistency clause pass on day 6 (900 + 5×20 = 1000).
    const o2 = simulateFuturesPath(days, path, { ...acc, consistencyMaxDayPct: null }, 1, 21);
    expect(o2.status).toBe('pass');
    expect(o2.days).toBe(6);
  });

  it('respects minTradingDays and EOD trailing', () => {
    const days = daysOf([[3000]]);
    const acc = futuresAccount({ minTradingDays: 3, drawdown: { amount: 2000, type: 'eod', stopsTrailing: null } }).account as FuturesAccount;
    const o = simulateFuturesPath(days, seq(10), acc, 1, 60);
    expect(o.status).toBe('pass');
    expect(o.days).toBe(3);
  });
});

describe('forex evaluation', () => {
  it('passes a 2-step account phase 1 → phase 2', () => {
    const days = daysOf([[2]]).map((d) => ({ ...d, trades: d.trades.map((t) => ({ ...t, r: 2 })) }));
    const acc = forexAccount().account as ForexAccount;
    const o = simulateForexPath(days, seq(30), forexParams(acc), 1, 10000);
    expect(o.status).toBe('pass');
    expect(o.days).toBe(7); // 4 days to +8%, then 3 days to +6% ≥ 5%
  });

  it('breaches the daily % loss on the equity basis', () => {
    const days = daysOf([[-3, -3]]).map((d) => ({ ...d, trades: d.trades.map((t) => ({ ...t, r: -3 })) }));
    const acc = forexAccount().account as ForexAccount;
    const o = simulateForexPath(days, seq(5), forexParams(acc), 1, 10000);
    expect(o.status).toBe('breach_daily');
    expect(o.days).toBe(1);
    // Half the risk stays inside the 5% limit for that day.
    const o2 = simulateForexPath(days, seq(1), forexParams(acc), 0.5, 10000);
    expect(o2.status).toBe('no_decision');
  });

  it('breaches static max loss over several days and trailing max loss from the hwm', () => {
    const lose = daysOf([[-4]]).map((d) => ({ ...d, trades: d.trades.map((t) => ({ ...t, r: -4 })) }));
    const acc = forexAccount().account as ForexAccount;
    const o = simulateForexPath(lose, seq(10), forexParams(acc), 1, 10000);
    expect(o.status).toBe('breach_trailing');
    expect(o.days).toBe(3);
    const mix = daysOf([[6], [-4]]).map((d) => ({ ...d, trades: d.trades.map((t) => ({ ...t, r: t.pnl })) }));
    const trailing = forexParams({ ...acc, maxLoss: { pct: 10, type: 'trailing' } });
    const o2 = simulateForexPath(mix, [0, 1, 1, 1, 1], trailing, 1, 10000);
    expect(o2.status).toBe('breach_trailing');
    expect(o2.days).toBe(4); // hwm 106 → floor 96; 106 − 4·3 = 94 ≤ 96 on the third losing day
  });
});

describe('driver', () => {
  it('is deterministic under the seed and shares paths across sizes', () => {
    const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 40, perDay: 3, winRate: 0.55, win: 120, loss: 100, seed: 3 }))));
    const ref = futuresAccount();
    const a = runSurvival(days, ref, { paths: 300, unit: 'micro' });
    const b = runSurvival(days, ref, { paths: 300, unit: 'micro' });
    expect(a).toEqual(b);
    expect(a.seed).toBe(4242 + 40);
    const c = runSurvival(days, ref, { paths: 300, unit: 'micro', seed: 99 });
    expect(c.sizes.map((s) => s.passPct)).not.toEqual(a.sizes.map((s) => s.passPct));
    expect(makeDayPaths(40, 2, 5)[0]).toEqual(makeDayPaths(40, 2, 5)[0]);
  });

  it('computes per30d cost periods and expected cost', () => {
    expect(periodsFor(10)).toBe(1);
    expect(periodsFor(21)).toBe(1);
    expect(periodsFor(22)).toBe(2);
    expect(attemptCost({ structure: 'per30d', windowDays: 30, evalFull: 100, renewal: 50 }, 30)).toBe(150);
    expect(attemptCost({ structure: 'onetime', evalFull: 100 }, 300)).toBe(100);
    expect(valueScoreOf(30, 100, 10)).toBe(0);
    expect(valueScoreOf(100, 100, 10)).toBeCloseTo(0.5 * 100 + 2500 / 200 + 750 / 40, 0);
  });

  it('caps runway at 21 days for per30d accounts and uses the patience path for ≤2 micros', () => {
    const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 30, perDay: 1, winRate: 1, win: 10 }))));
    const windowed = runSurvival(days, futuresAccount({ cost: { structure: 'per30d', windowDays: 30, evalFull: 100, renewal: 100 } }), { paths: 50, unit: 'micro' });
    expect(windowed.sizes.every((s) => s.runway === 21)).toBe(true);
    const onetime = runSurvival(days, futuresAccount(), { paths: 50, unit: 'micro' });
    expect(onetime.sizes.find((s) => s.size === 1)?.runway).toBe(250);
    expect(onetime.sizes.find((s) => s.size === 5)?.runway).toBe(60);
  });
});
