import { describe, expect, it } from 'vitest';
import { band, linearFit, maxDrawdown, mulberry32, pearson, percentile } from '../rng';
import { AnalyticsError, basicStats, groupDays, normalizeTrades } from '../trades';
import { bootstrapDays, monteCarloTrades } from '../bootstrap';
import { daysOf, prodataOf, synthTrades } from './helpers';

describe('rng', () => {
  it('mulberry32 is deterministic and uniform in [0,1)', () => {
    const a = mulberry32(7);
    const b = mulberry32(7);
    const xs = Array.from({ length: 1000 }, () => a());
    const ys = Array.from({ length: 1000 }, () => b());
    expect(xs).toEqual(ys);
    expect(Math.min(...xs)).toBeGreaterThanOrEqual(0);
    expect(Math.max(...xs)).toBeLessThan(1);
    const m = xs.reduce((s, x) => s + x, 0) / xs.length;
    expect(m).toBeGreaterThan(0.45);
    expect(m).toBeLessThan(0.55);
  });

  it('percentile interpolates linearly', () => {
    expect(percentile([1, 2, 3, 4], 50)).toBe(2.5);
    expect(percentile([1, 2, 3, 4], 0)).toBe(1);
    expect(percentile([1, 2, 3, 4], 100)).toBe(4);
    expect(percentile([10, 20, 30], 25)).toBe(15);
    expect(percentile([5], 95)).toBe(5);
  });

  it('band is piecewise linear and clamps', () => {
    const a = [[0, 0], [1, 100]] as const;
    expect(band(-1, a)).toBe(0);
    expect(band(0.25, a)).toBe(25);
    expect(band(2, a)).toBe(100);
    expect(band(1.5, [[1, 100], [2, 0]])).toBe(50);
  });

  it('linearFit / maxDrawdown / pearson basics', () => {
    expect(linearFit([1, 2, 3, 4]).r2).toBeCloseTo(1);
    expect(linearFit([4, 3, 2, 1]).slope).toBeLessThan(0);
    expect(maxDrawdown([100, -50, -100, 200])).toBe(150);
    expect(pearson([1, 2, 3], [2, 4, 6])).toBeCloseTo(1);
    expect(pearson([1, 2, 3], [3, 2, 1])).toBeCloseTo(-1);
  });
});

describe('trades', () => {
  it('normalizes futures P&L per contract and derives hour/day', () => {
    const raw = synthTrades({ days: 4, perDay: 3, win: 200, loss: 100 });
    const p = prodataOf(raw, { contractsInBacktest: 2 });
    const t = normalizeTrades(p);
    expect(t).toHaveLength(12);
    expect(new Set(t.map((x) => x.pnl))).toEqual(new Set([100, -50]));
    expect(t[0].hour).toBe(9);
    expect(t[0].dow).toBe(1); // 2025-01-06 is a Monday
    const days = groupDays(t);
    expect(days).toHaveLength(4);
    expect(days[0].trades).toHaveLength(3);
    expect(days[0].net).toBe(days[0].trades.reduce((a, x) => a + x.pnl, 0));
  });

  it('uses R for forex when present, raw pnl otherwise', () => {
    const raw = synthTrades({ days: 4, perDay: 3, win: 200, loss: 100 });
    const t = normalizeTrades(prodataOf(raw, { market: 'forex', backtestUnit: 'lot' }));
    expect(new Set(t.map((x) => x.pnl))).toEqual(new Set([2, -1]));
    const noR = raw.map((x) => ({ ...x, r: undefined }));
    const t2 = normalizeTrades(prodataOf(noR, { market: 'cfd', contractsInBacktest: 3 }));
    expect(new Set(t2.map((x) => x.pnl))).toEqual(new Set([200, -100]));
  });

  it('throws on fewer than 10 trades', () => {
    expect(() => normalizeTrades(prodataOf(synthTrades({ days: 3, perDay: 3 })))).toThrow(AnalyticsError);
  });

  it('basicStats computes PF, expectancy, avg win/loss', () => {
    const s = basicStats([100, -50, 100, -50]);
    expect(s.profitFactor).toBe(2);
    expect(s.expectancy).toBe(25);
    expect(s.avgLoss).toBe(50);
    expect(s.winRate).toBe(0.5);
    expect(basicStats([1, 2]).profitFactor).toBe(99);
  });
});

describe('bootstrap', () => {
  it('is deterministic under the default seed', () => {
    const days = daysOf(Array.from({ length: 30 }, (_, i) => [i % 3 === 0 ? -80 : 100, 50]));
    const a = bootstrapDays(days, { paths: 300 });
    const b = bootstrapDays(days, { paths: 300 });
    expect(a.final).toEqual(b.final);
    expect(a.maxDD).toEqual(b.maxDD);
    expect(a.seed).toBe(4242 + 30);
    const c = bootstrapDays(days, { paths: 300, seed: 1 });
    expect(c.final).not.toEqual(a.final);
  });

  it('100% winners never draw down', () => {
    const days = daysOf(Array.from({ length: 20 }, () => [100, 100]));
    const b = bootstrapDays(days, { paths: 200 });
    expect(b.maxDD.p95).toBe(0);
    expect(b.final.p5).toBe(4000);
    expect(b.ruinProb(1)).toBe(0);
    expect(b.lossShare).toBe(0);
    const mc = monteCarloTrades([100, 100, 100], 100);
    expect(mc.maxDD.p95).toBe(0);
  });
});
