import { describe, expect, it } from 'vitest';
import { fundedFuturesRules, replayWithPlan, runFunded, runGameplan, scalingPhases, simulateFundedFutures, simulateFundedForex, fundedForexRules, PLAN_REPS } from '../gameplan';
import { groupDays, normalizeTrades } from '../trades';
import { daysOf, forexAccount, futuresAccount, prodataOf, synthTrades } from './helpers';
import type { ForexAccount, FuturesAccount } from '../rules';

const seq = (n: number, i = 0) => Array.from({ length: n }, () => i);

describe('funded futures', () => {
  it('derives buffer, window and first payout day from the rules', () => {
    const acc = futuresAccount().account as FuturesAccount;
    const r = fundedFuturesRules(acc);
    expect(r.buffer).toBe(2100);
    expect(r.window).toBe(10); // 14 calendar days × 5/7
    expect(r.firstPayoutDay).toBe(5);
    const noBuffer = fundedFuturesRules({ ...acc, funded: { ...acc.funded, bufferAmount: null } });
    expect(noBuffer.buffer).toBe(2100); // drawdown 2000 + 100
  });

  it('pays out once the buffer is cleared and stops at the lifetime cap', () => {
    const days = daysOf([[500]]);
    const acc = futuresAccount({ funded: { bufferAmount: 2000, minDaysToFirstPayout: 5, payoutFrequencyDays: 7, minPayout: 500, maxPayoutPct: 100, payoutCap: 1000, maxPayoutsLifetime: 2 } }).account as FuturesAccount;
    const o = simulateFundedFutures(days, seq(250), fundedFuturesRules(acc), 1);
    expect(o.survived).toBe(true);
    expect(o.daysToBuffer).toBe(4);
    expect(o.daysToFirstPayout).toBe(5); // eq 2500 → w = 500 ≥ minPayout
    expect(o.payouts).toBe(2);
    expect(o.totalPaid).toBe(1500); // 500 on day 5, then capped 1000 on day 10
    expect(o.days).toBe(10);
  });

  it('a losing account breaches the funded floor', () => {
    const days = daysOf([[-250]]);
    const acc = futuresAccount().account as FuturesAccount;
    const o = simulateFundedFutures(days, seq(250), fundedFuturesRules(acc), 1);
    expect(o.survived).toBe(false);
    expect(o.days).toBe(8);
    expect(o.totalPaid).toBe(0);
  });

  it('runFunded aggregates deterministically', () => {
    const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 30, perDay: 2, winRate: 0.6, win: 150, loss: 100, seed: 8 }))));
    const ref = futuresAccount();
    const a = runFunded(days, ref, { paths: 200, unit: 'micro', size: 10 });
    const b = runFunded(days, ref, { paths: 200, unit: 'micro', size: 10 });
    expect(a).toEqual(b);
    expect(a.survivePct).toBeGreaterThan(0);
    expect(a.monthlyIncome.p50).toBeGreaterThanOrEqual(0);
  });
});

describe('funded forex', () => {
  it('withdraws profit at the cycle with the split for that cycle', () => {
    const days = daysOf([[1]]).map((d) => ({ ...d, trades: d.trades.map((t) => ({ ...t, r: 1 })) }));
    const acc = forexAccount().account as ForexAccount;
    const rules = fundedForexRules(acc);
    expect(rules.splitPct).toBe(80);
    expect(rules.window).toBe(10);
    const o = simulateFundedForex(days, seq(20), rules, 1, 10000);
    expect(o.survived).toBe(true);
    expect(o.daysToFirstPayout).toBe(10);
    expect(o.payouts).toBe(2);
    // 10 days × 1% = 10% profit → 80% of $1,000 = $800 per cycle.
    expect(o.totalPaid).toBeCloseTo(1600, 6);
  });
});

describe('phases', () => {
  it('computes proof days, cycles and full-scale timeline', () => {
    const funded = { survivePct: 80, payoutPct: 70, reachBufferPct: 70, daysToBuffer: { p25: 10, p50: 20, p75: 30 }, daysToFirstPayout: { p25: 15, p50: 25, p75: 40 }, monthlyIncome: { p25: 0, p50: 0, p75: 0 }, totalIncome: { p25: 0, p50: 0, p75: 0 }, paths: 1, horizon: 250, notes: [] };
    const p = scalingPhases(12, funded, 5, 1);
    expect(p.proofDays).toBe(25);
    expect(p.cyclesToFull).toBe(4);
    expect(p.fullScaleDays).toBe(12 + 4 * 25);
    expect(p.weeks).toBe(112 / 5);
    expect(p.months).toBeCloseTo(112 / 21, 1);
    expect(scalingPhases(12, funded, 1, 1).cyclesToFull).toBe(1);
    expect(scalingPhases(12, funded, 5, 2).cyclesToFull).toBe(2);
  });
});

describe('plan sliders (common random numbers)', () => {
  const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 12, perDay: 4, winRate: 0.5, win: 100, loss: 80, seed: 11 }))));

  it('is deterministic and produces PLAN_REPS × days blocks', () => {
    const a = replayWithPlan(days, { winRate: 60 });
    const b = replayWithPlan(days, { winRate: 60 });
    expect(a).toEqual(b);
    expect(a).toHaveLength(PLAN_REPS * days.length);
  });

  it('a trade cap only removes trades — the kept ones are identical (CRN)', () => {
    const full = replayWithPlan(days, { winRate: 55 });
    const capped = replayWithPlan(days, { winRate: 55, maxTradesPerDay: 2 });
    for (let i = 0; i < full.length; i++) {
      expect(capped[i].trades).toHaveLength(2);
      expect(capped[i].trades.map((t) => t.pnl)).toEqual(full[i].trades.slice(0, 2).map((t) => t.pnl));
    }
  });

  it('moves the realized win rate with the slider and exec pushes days off plan', () => {
    const wr = (blocks: ReturnType<typeof replayWithPlan>) => {
      let w = 0;
      let n = 0;
      for (const b of blocks) for (const t of b.trades) {
        n++;
        if (t.pnl > 0) w++;
      }
      return w / n;
    };
    expect(wr(replayWithPlan(days, { winRate: 80 }))).toBeGreaterThan(0.72);
    expect(wr(replayWithPlan(days, { winRate: 30 }))).toBeLessThan(0.38);
    expect(wr(replayWithPlan(days, { winRate: 80, exec: 50 }))).toBeLessThan(wr(replayWithPlan(days, { winRate: 80 })));
    const capped = replayWithPlan(days, { winRate: 30, dailyCap: 100 });
    for (const b of capped) {
      // once the running day P&L is ≤ −cap, no further trades are recorded
      let run = 0;
      for (const t of b.trades) {
        expect(run).toBeGreaterThan(-100);
        run += t.pnl;
      }
    }
  });

  it('runGameplan with a plan keeps a baseline for comparison', () => {
    const ref = futuresAccount();
    const g = runGameplan(days, ref, { paths: 100, unit: 'micro', size: 5, daysToPassP50: 10, plan: { winRate: 70 } });
    expect(g.baseline).not.toBeNull();
    expect(g.plan).toEqual({ winRate: 70 });
    expect(g.phases.maxAccounts).toBe(5);
    const g0 = runGameplan(days, ref, { paths: 100, unit: 'micro', size: 5, daysToPassP50: 10 });
    expect(g0.baseline).toBeNull();
    expect(g0.funded).toEqual(g.baseline);
  });
});
