import { describe, expect, it } from 'vitest';
import { monteCarloTrades } from '../bootstrap';
import { edgeFromSqn, killzone, labelRegimes, outliers, qualityScore, regime, rollingWindows, sqn, rMultiples, trueCost, concentration } from '../quality';
import { groupDays, normalizeTrades } from '../trades';
import { dateAt, prodataOf, synthTrades } from './helpers';
import type { DailyBar } from '../types';

function tradesOf(o: Parameters<typeof synthTrades>[0]) {
  return normalizeTrades(prodataOf(synthTrades(o)));
}

describe('quality score', () => {
  it('SQN and edge piecewise', () => {
    const { r, avgLoss } = rMultiples([100, -50, 100, -50, 100]);
    expect(avgLoss).toBe(50);
    expect(r).toEqual([2, -1, 2, -1, 2]);
    expect(sqn([1, 1, 1])).toBe(10);
    expect(edgeFromSqn(-2)).toBe(0);
    expect(edgeFromSqn(0)).toBe(20);
    expect(edgeFromSqn(0.8)).toBe(20);
    expect(edgeFromSqn(1.6)).toBe(40);
    expect(edgeFromSqn(2)).toBe(55);
    expect(edgeFromSqn(2.5)).toBe(70);
    expect(edgeFromSqn(3)).toBe(82);
    expect(edgeFromSqn(5)).toBe(95);
    expect(edgeFromSqn(10)).toBe(100);
  });

  it('caps the score by sample size (n<20 → ≤45) and by variants (8 → ≤55)', () => {
    const small = tradesOf({ days: 5, perDay: 3, winRate: 1, win: 100 }).map((t) => t.pnl);
    const q = qualityScore(small, monteCarloTrades(small, 200), { tradingDays: 5 });
    expect(q.score).toBeLessThanOrEqual(45);
    expect(q.caps.map((c) => c.reason)).toContain('n<20');
    const big = tradesOf({ days: 60, perDay: 3, winRate: 0.6, win: 120, loss: 100, seed: 5 }).map((t) => t.pnl);
    const mc = monteCarloTrades(big, 300);
    const free = qualityScore(big, mc, { tradingDays: 60 });
    expect(free.caps).toHaveLength(0);
    expect(free.score).toBeGreaterThan(55);
    const lucky = qualityScore(big, mc, { tradingDays: 60, variants: 8 });
    expect(lucky.score).toBeLessThanOrEqual(55);
    expect(lucky.caps.map((c) => c.reason)).toContain('variants>=8');
    expect(qualityScore(big, mc, { tradingDays: 60, variants: 2 }).caps[0].cap).toBe(85);
    expect(qualityScore(big, mc, { tradingDays: 10 }).caps.map((c) => c.cap)).toContain(70);
    expect(qualityScore(big, mc, { tradingDays: 10, variants: 8, noCaps: true }).caps).toHaveLength(0);
  });

  it('blends OOS retention into robustness', () => {
    const big = tradesOf({ days: 60, perDay: 3, winRate: 0.6, win: 120, loss: 100, seed: 5 }).map((t) => t.pnl);
    const mc = monteCarloTrades(big, 300);
    const base = qualityScore(big, mc, { tradingDays: 60 });
    const bad = qualityScore(big, mc, { tradingDays: 60, oos: { isPF: 2, oosPF: 0.5 } });
    const good = qualityScore(big, mc, { tradingDays: 60, oos: { isPF: 2, oosPF: 2 } });
    expect(bad.robust).toBeLessThan(base.robust);
    expect(good.robust).toBeCloseTo(0.65 * base.robust + 35, 0);
    expect(bad.retention).toBe(0.25);
  });

  it('outliers: net without top winners and top share', () => {
    const o = outliers([500, 100, 100, -50, -50, -50]);
    expect(o.net).toBe(550);
    expect(o.netWithoutTop1).toBe(50);
    expect(o.netWithoutTop3).toBe(-150);
    expect(o.topShare).toBeCloseTo((500 / 700) * 100, 5);
  });
});

describe('rolling windows', () => {
  it('a steady strategy travels; a single lucky stretch does not', () => {
    const steady = tradesOf({ days: 30, perDay: 3, winRate: 0.7, win: 100, loss: 80, seed: 2 });
    const rw = rollingWindows(steady);
    expect(rw.segments).toHaveLength(5);
    expect(rw.travels).toBe(true);
    expect(rw.oneLuckyStretch).toBe(false);
    // 4 segments: one huge, three slightly negative.
    const raw = synthTrades({ days: 16, perDay: 2, winRate: 0, loss: 10 });
    raw.slice(0, 8).forEach((t) => (t.pnl = 500));
    const lucky = rollingWindows(normalizeTrades(prodataOf(raw)));
    expect(lucky.segments).toHaveLength(4);
    expect(lucky.travels).toBe(false);
    expect(lucky.oneLuckyStretch).toBe(true);
    expect(lucky.netWithoutBestSeg).toBeLessThan(0);
  });

  it('uses 6 segments for n ≥ 120', () => {
    const t = tradesOf({ days: 40, perDay: 3 });
    expect(rollingWindows(t).segments).toHaveLength(6);
  });
});

describe('concentration and killzone', () => {
  it('groups by hour and weekday and needs 20 timed trades for the killzone', () => {
    const t = tradesOf({ days: 15, perDay: 3, winRate: 1, win: 10 });
    const c = concentration(t);
    expect(c.byHour.map((r) => r.key)).toEqual(['09:00', '10:00', '11:00']);
    expect(c.byHour[0].n).toBe(15);
    expect(c.byWeekday.map((r) => r.key)).toEqual(['Mon', 'Tue', 'Wed', 'Thu', 'Fri']);
    expect(c.byWeekday[0].winRate).toBe(100);
    const kz = killzone(t);
    expect(kz).not.toBeNull();
    expect(kz!.cells.length).toBe(15);
    expect(kz!.best!.n).toBeGreaterThanOrEqual(3);
    expect(killzone(tradesOf({ days: 5, perDay: 3 }))).toBeNull();
    expect(killzone(tradesOf({ days: 20, perDay: 3, timed: false }))).toBeNull();
  });
});

describe('true cost', () => {
  it('mini/micro defaults give a crossover of 6 micros', () => {
    const pnls = Array.from({ length: 50 }, (_, i) => (i % 2 ? 100 : -40));
    const mini = trueCost(pnls, 'mini');
    expect(mini.costPerTrade).toBe(14);
    expect(mini.crossoverMicros).toBe(6);
    expect(mini.afterNet).toBe(1500 - 50 * 14);
    expect(mini.keepPct).toBeCloseTo(((1500 - 700) / 1500) * 100, 0);
    const micro = trueCost(pnls, 'micro');
    expect(micro.costPerTrade).toBe(2.5);
    const lot = trueCost(pnls, 'lot');
    expect(lot.costPerTrade).toBe(17);
    expect(lot.crossoverMicros).toBeNull();
  });
});

describe('regime', () => {
  function daily(): DailyBar[] {
    const bars: DailyBar[] = [];
    let c = 100;
    for (let i = 0; i < 60; i++) {
      c += 1;
      bars.push({ d: dateAt(i), o: c - 0.5, c });
    }
    for (let i = 60; i < 120; i++) {
      c += i % 2 ? 3 : -3;
      bars.push({ d: dateAt(i), o: c, c });
    }
    return bars;
  }

  it('labels a synthetic trend as trend and alternating bars as chop', () => {
    const { labels } = labelRegimes(daily());
    expect(labels.get(dateAt(40))).toBe('quiet trend');
    expect(labels.get(dateAt(110))).toBe('volatile chop');
    expect(labels.has(dateAt(5))).toBe(false);
  });

  it('joins trades by exit date and needs 10 trades per row', () => {
    const raw = synthTrades({ days: 120, perDay: 1, winRate: 1, win: 10 });
    const t = normalizeTrades(prodataOf(raw));
    const r = regime(t, daily());
    expect(r).not.toBeNull();
    const labels = r!.rows.map((x) => x.label);
    expect(labels).toContain('quiet trend');
    expect(labels).toContain('volatile chop');
    expect(regime(t, daily().slice(0, 30))).toBeNull();
  });
});
