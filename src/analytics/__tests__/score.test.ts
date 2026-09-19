import { describe, expect, it } from 'vitest';
import { computeTradingScore, edgePillar, overtradingAnalysis, pastStopAnalysis, revengeAnalysis, streakAnalysis, type HistoryTrade } from '../score';
import { groupDays, normalizeTrades } from '../trades';
import { dateAt, synthTrades } from './helpers';

function hist(o: Parameters<typeof synthTrades>[0]): HistoryTrade[] {
  return synthTrades(o).map((t) => ({ d: t.d, t: t.t, pnl: t.pnl }));
}

describe('trading score floor', () => {
  it('reports collecting below 30 trades or 10 days', () => {
    const few = computeTradingScore(hist({ days: 5, perDay: 4 }));
    expect(few.status).toBe('collecting');
    if (few.status === 'collecting') {
      expect(few.n).toBe(20);
      expect(few.needTrades).toBe(30);
    }
    const fewDays = computeTradingScore(hist({ days: 8, perDay: 5 }));
    expect(fewDays.status).toBe('collecting');
    const ok = computeTradingScore(hist({ days: 10, perDay: 3 }));
    expect(ok.status).toBe('ready');
  });
});

describe('edge pillar (hand-checked)', () => {
  it('matches the manual computation on a +100/−50/+100 series', () => {
    // 30 trades: PF 4, avgLoss 50, R = [2,−1,2]…, mean R 1, sd R 1.4384, SQN 3.808 → 94.04
    // edge = .35·94.04 + .25·100 + .15·100 + .25·100 = 97.91; shrink 0.3 → 50 + 47.91·0.3 = 64.4
    const pnls = Array.from({ length: 30 }, (_, i) => (i % 3 === 1 ? -50 : 100));
    const e = edgePillar(pnls);
    expect(e.parts.sqn.value).toBeCloseTo(3.808, 2);
    expect(e.parts.sqn.score).toBeCloseTo(94.0, 0);
    expect(e.parts.pf.score).toBe(100);
    expect(e.parts.pfWithoutTop3.score).toBe(100);
    expect(e.parts.expectancyRatio.value).toBe(1);
    expect(e.parts.expectancyRatio.score).toBe(100);
    expect(e.parts.bootLossShare.value).toBe(0);
    expect(e.score).toBeCloseTo(64.4, 0);
  });

  it('caps a fragile edge when the bootstrap loses often', () => {
    // Mostly small losses with a few big wins: many resampled histories end negative.
    const pnls = Array.from({ length: 100 }, (_, i) => (i % 10 === 0 ? 900 : -95));
    const e = edgePillar(pnls);
    expect(e.parts.bootLossShare.value).toBeGreaterThan(10);
    expect(e.score).toBeLessThanOrEqual(60);
  });
});

describe('overall score', () => {
  it('produces letters and tiers consistently and deterministically', () => {
    const h = hist({ days: 40, perDay: 3, winRate: 0.6, win: 120, loss: 100, seed: 4 });
    const a = computeTradingScore(h);
    const b = computeTradingScore(h);
    expect(a).toEqual(b);
    expect(a.status).toBe('ready');
    if (a.status !== 'ready') return;
    expect(a.tier).toBe('Developing');
    expect(a.overall).toBeGreaterThanOrEqual(0);
    expect(a.overall).toBeLessThanOrEqual(100);
    expect(['A', 'B', 'C', 'D', 'F']).toContain(a.letter);
    expect(a.briefing.hourBins.length).toBe(3);
    expect(a.briefing.bestHour).not.toBeNull();
    expect(a.discipline.parts.overtrading.score).not.toBeNull();
  });

  it('net-negative history uses the reduced consistency formula', () => {
    const r = computeTradingScore(hist({ days: 20, perDay: 3, winRate: 0.3, win: 50, loss: 100, seed: 9 }));
    expect(r.status).toBe('ready');
    if (r.status !== 'ready') return;
    expect(Object.keys(r.consistency.parts)).toEqual(['curve', 'greenRate']);
    expect(r.consistency.parts.curve.score).toBe(5);
  });
});

describe('discipline analyses', () => {
  it('detects revenge re-entries after worst-decile losses', () => {
    // Each day: a huge loss at 09:00 and a re-entry 5 minutes later; plus later trades 60 min apart.
    const raw: HistoryTrade[] = [];
    for (let d = 0; d < 20; d++) {
      raw.push({ d: dateAt(d), t: '09:00', pnl: -500 });
      raw.push({ d: dateAt(d), t: '09:05', pnl: -50 });
      raw.push({ d: dateAt(d), t: '10:05', pnl: 60 });
      raw.push({ d: dateAt(d), t: '11:05', pnl: 60 });
    }
    const days = groupDays(normalizeTrades({ trades: raw, market: 'futures', contractsInBacktest: 1 }));
    const r = revengeAnalysis(days);
    expect(r.opportunities).toBe(20);
    expect(r.rate).toBe(1);
    const calm = groupDays(normalizeTrades({ trades: raw.map((t) => (t.t === '09:05' ? { ...t, t: '09:50' } : t)), market: 'futures', contractsInBacktest: 1 }));
    expect(revengeAnalysis(calm).rate).toBe(0);
  });

  it('finds the first losing intraday slot', () => {
    const raw: HistoryTrade[] = [];
    for (let d = 0; d < 25; d++) {
      raw.push({ d: dateAt(d), t: '09:00', pnl: 50 });
      raw.push({ d: dateAt(d), t: '10:00', pnl: 50 });
      raw.push({ d: dateAt(d), t: '11:00', pnl: -40 });
      raw.push({ d: dateAt(d), t: '12:00', pnl: -40 });
    }
    const days = groupDays(normalizeTrades({ trades: raw, market: 'futures', contractsInBacktest: 1 }));
    const o = overtradingAnalysis(days);
    expect(o.slot).toBe(3);
    expect(o.shareBeyond).toBe(0.5);
    expect(o.netBeyond).toBe(-2000);
  });

  it('measures streak escalation and the expected streak', () => {
    const s = streakAnalysis([-10, -20, -40, 10, -10, -20, 10, 10, -10, -20, 10, -10, -20, 10, -10, -30]);
    expect(s.worstStreak).toBe(3);
    expect(s.escalation).toBe(2);
    expect(s.expStreak).not.toBeNull();
  });

  it('finds the collapse level where recovery odds drop below 20%', () => {
    // A: dip −40 → green; B: dip −100 → green; C: dip −150 → keeps trading to −200; D: −75 → −100.
    // Red depths {100×10, 200×10} → levels 100/150/200; at −100 a third recover, at −150 nobody.
    const raw: HistoryTrade[] = [];
    const shapes = [
      [-40, 90],
      [-100, 150],
      [-150, -50],
      [-75, -25],
    ];
    for (let d = 0; d < 40; d++) {
      const s = shapes[d % 4];
      raw.push({ d: dateAt(d), t: '09:00', pnl: s[0] });
      raw.push({ d: dateAt(d), t: '10:00', pnl: s[1] });
    }
    const days = groupDays(normalizeTrades({ trades: raw, market: 'futures', contractsInBacktest: 1 }));
    const p = pastStopAnalysis(days);
    expect(p.levels.map((l) => l.x)).toEqual([100, 150, 200]);
    expect(p.levels[0].pGreen).toBeCloseTo(1 / 3, 3);
    expect(p.collapseX).toBe(150);
    expect(p.shareWorse).toBe(1);
    expect(p.damage).toBe(-500);
  });
});
