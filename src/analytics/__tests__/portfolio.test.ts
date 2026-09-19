import { describe, expect, it } from 'vitest';
import { accountSurvival, healthCheck, mergePortfolio, overnightGap, portfolioSurvival, scoreboard } from '../portfolio';
import { groupDays, normalizeTrades } from '../trades';
import { dateAt, futuresAccount, prodataOf, synthTrades } from './helpers';
import type { DailyBar } from '../types';

describe('health check', () => {
  const baseline = Array.from({ length: 200 }, (_, i) => (i % 5 === 0 ? -80 : 30)); // mean +8

  it('flags a cold streak, a hot streak and a normal stretch', () => {
    expect(healthCheck(baseline, Array.from({ length: 20 }, () => -80)).verdict).toBe('investigate');
    expect(healthCheck(baseline, Array.from({ length: 20 }, () => 30)).verdict).toBe('hot');
    const typical = [30, 30, 30, 30, -80, 30, 30, 30, 30, -80, 30, 30, 30, 30, -80, 30, 30, 30, 30, -80];
    expect(healthCheck(baseline, typical).verdict).toBe('normal');
    const watch = healthCheck(baseline, [30, 30, -80, 30, -80, 30, 30, -80, 30, 30, 30, -80, 30, 30, 30, 30, -80, 30, 30, -80]);
    expect(['watch', 'investigate']).toContain(watch.verdict);
  });

  it('guards small samples and is deterministic', () => {
    expect(healthCheck(baseline, [1, 2, 3]).verdict).toBe('insufficient');
    expect(healthCheck(baseline.slice(0, 10), [1, 2, 3, 4, 5, 6]).verdict).toBe('insufficient');
    const a = healthCheck(baseline, [30, 30, 30, 30, -80, 30]);
    expect(a).toEqual(healthCheck(baseline, [30, 30, 30, 30, -80, 30]));
    expect(a.band!.p5).toBeLessThan(a.band!.p95);
  });
});

describe('scoreboard', () => {
  it('ranks strategies and flags identical vs inverted daily P&L', () => {
    const rawA = synthTrades({ days: 30, perDay: 2, winRate: 0.6, win: 120, loss: 100, seed: 21 });
    const rawInv = rawA.map((t) => ({ ...t, pnl: -t.pnl, r: t.r == null ? t.r : -t.r }));
    const rawOther = synthTrades({ days: 30, perDay: 2, winRate: 0.6, win: 120, loss: 100, seed: 22 });
    const sb = scoreboard([
      { name: 'A', prodata: prodataOf(rawA) },
      { name: 'A-copy', prodata: prodataOf(rawA) },
      { name: 'A-inverted', prodata: prodataOf(rawInv) },
      { name: 'Other', prodata: prodataOf(rawOther) },
      { name: 'tiny', prodata: prodataOf(rawA.slice(0, 4)) },
    ]);
    expect(sb.rows).toHaveLength(4);
    expect(sb.skipped.map((s) => s.name)).toEqual(['tiny']);
    expect(sb.rows[0].score).toBeGreaterThanOrEqual(sb.rows[sb.rows.length - 1].score);
    const same = sb.correlations.find((c) => c.a === 'A' && c.b === 'A-copy');
    const inv = sb.correlations.find((c) => c.a === 'A' && c.b === 'A-inverted');
    expect(same?.rho).toBe(1);
    expect(same?.flag).toBe(true);
    expect(inv?.rho).toBe(-1);
    expect(inv?.flag).toBe(true);
    expect(same?.sharedDays).toBe(30);
    const other = sb.correlations.find((c) => c.a === 'A' && c.b === 'Other');
    expect(Math.abs(other!.rho)).toBeLessThan(0.7);
  });
});

describe('portfolio survival', () => {
  it('intersects date windows, merges day blocks per micro and measures diversification', () => {
    const a = synthTrades({ days: 40, perDay: 2, winRate: 0.6, win: 120, loss: 100, seed: 31 });
    const b = synthTrades({ days: 40, perDay: 2, winRate: 0.6, win: 120, loss: 100, seed: 32 }).slice(20); // starts 10 days later
    const m = mergePortfolio([
      { name: 'A', prodata: prodataOf(a, { backtestUnit: 'mini' }) },
      { name: 'B', prodata: prodataOf(b) },
    ]);
    expect(m.window).toEqual({ from: dateAt(10), to: dateAt(39) });
    expect(m.merged).toHaveLength(30);
    expect(m.merged[0].trades).toHaveLength(4);
    // mini trades scaled ×0.1 → 12 / −10; micro trades unchanged.
    expect(new Set(m.merged.flatMap((d) => d.trades.map((t) => Math.abs(t.pnl))))).toEqual(new Set([12, 10, 120, 100]));
    const ps = portfolioSurvival([{ name: 'A', prodata: prodataOf(a) }, { name: 'B', prodata: prodataOf(b) }], futuresAccount(), { paths: 200 });
    expect(ps.survival).not.toBeNull();
    expect(ps.survival!.sizes.map((s) => s.size)).toEqual([1, 2, 5, 10]);
    expect(ps.diversification).toBeGreaterThanOrEqual(0);
    expect(ps.diversification).toBeLessThanOrEqual(100);
    const self = portfolioSurvival([{ name: 'A', prodata: prodataOf(a) }, { name: 'A2', prodata: prodataOf(a) }], null, { paths: 200 });
    expect(self.diversifying).toBe(false);
  });
});

describe('account survival and overnight gap', () => {
  it('sizes contracts by risk and measures ruin probabilities', () => {
    const days = groupDays(normalizeTrades(prodataOf(synthTrades({ days: 40, perDay: 2, winRate: 0.55, win: 120, loss: 100, seed: 41 }))));
    const r = accountSurvival(days, 10000, 'micro', { paths: 100 });
    expect(r.rows.map((x) => x.contracts)).toEqual([0, 1, 2]);
    expect(r.rows[0].ruin10Pct).toBe(0);
    expect(r.rows[2].ruin10Pct).toBeGreaterThanOrEqual(r.rows[1].ruin10Pct);
  });

  it('computes gap quantiles and the overnight/intraday split', () => {
    const daily: DailyBar[] = Array.from({ length: 40 }, (_, i) => ({ d: dateAt(i), o: 100 + i + (i % 4 === 0 ? 2 : 0), c: 100 + i }));
    const raw = synthTrades({ days: 12, perDay: 1, winRate: 1, win: 10 }).map((t, i) => ({ ...t, ed: i % 3 === 0 ? dateAt(Math.max(0, i - 1)) : t.d }));
    const trades = normalizeTrades(prodataOf(raw));
    const g = overnightGap(daily, 5, trades)!;
    expect(g.bars).toBe(40);
    expect(g.gapPoints.max).toBe(3);
    expect(g.gapDollars.max).toBe(15);
    expect(g.overnight.n).toBe(3); // i = 3, 6, 9 (i = 0 has ed === d)
    expect(g.intraday.n).toBe(9);
    expect(overnightGap(daily.slice(0, 10), 5, trades)).toBeNull();
  });
});
