import { describe, expect, it } from 'vitest';
import { analyzeProdata, listAccounts, findAccount, computeTradingScore } from '../index';
import { loadRules, prodataOf, synthTrades } from './helpers';

const rules = loadRules();

describe('rules file', () => {
  it('loads both families from data/prop-firms.json', () => {
    expect(rules.schema).toBe('strategy-desk/prop-firms/v1');
    const fut = listAccounts(rules, { family: 'futures' });
    const fx = listAccounts(rules, { family: 'forex' });
    expect(fut.length).toBeGreaterThan(20);
    expect(fx.length).toBeGreaterThan(3);
    expect(findAccount(rules, 'apex-50k')?.firm.id).toBe('apex');
    expect(listAccounts(rules, { ids: ['apex'], family: 'futures' }).every((r) => r.firm.id === 'apex')).toBe(true);
    expect(listAccounts(rules, { ids: ['topstep-50k'] })).toHaveLength(1);
  });
});

describe('analyzeProdata', () => {
  const raw = synthTrades({ days: 45, perDay: 3, winRate: 0.58, win: 60, loss: 45, seed: 77 });

  it('survival focus: header + eval grid, deterministic', () => {
    const p = prodataOf(raw, { focus: 'survival', firms: ['apex-50k', 'topstep-50k', 'tradeify-growth-50k'] });
    const a = analyzeProdata(p, rules, { paths: 150 });
    const b = analyzeProdata(p, rules, { paths: 150 });
    expect(a).toEqual(b);
    expect(a.header.n).toBe(135);
    expect(a.header.days).toBe(45);
    expect(a.header.unitLabel).toBe('per 1 micro');
    expect(a.evalGrid).toHaveLength(3);
    expect(a.evalGrid![0].sizes.length).toBeGreaterThan(0);
    expect(a.gameplan).toBeNull();
    expect(a.quality).toBeNull();
    expect(a.bootstrap.paths).toBe(150);
    expect(a.simCaveat).toContain('trade-close');
    for (const g of a.evalGrid!) for (const s of g.sizes) {
      expect(s.passPct + s.breachTrailingPct + s.breachDailyPct + s.stallPct + s.noDecisionPct).toBeCloseTo(100, 0);
    }
  });

  it('gameplan focus: funded sim for the chosen account', () => {
    const p = prodataOf(raw, { focus: 'gameplan', gameplan: { firm: 'apex-50k' } });
    const r = analyzeProdata(p, rules, { paths: 100 });
    expect(r.gameplan).not.toBeNull();
    expect(r.gameplan!.accountId).toBe('apex-50k');
    expect(r.gameplan!.funded.paths).toBe(100);
    expect(r.gameplan!.phases.maxAccounts).toBe(20);
    expect(r.evalGrid).toHaveLength(1);
    const missing = analyzeProdata(prodataOf(raw, { focus: 'gameplan', gameplan: { firm: 'nope' } }), rules, { paths: 20 });
    expect(missing.gameplan).toBeNull();
    expect(missing.notes[0]).toContain('nope');
  });

  it('full focus: every section, forex family from a forex prodata', () => {
    const p = prodataOf(raw, { focus: 'full', firms: ['fundingpips-10k-2step-standard'], market: 'forex', backtestUnit: 'lot', gameplan: { firm: 'fundingpips-10k-2step-standard' }, variants: 3 });
    const r = analyzeProdata(p, rules, { paths: 100 });
    expect(r.family).toBe('forex');
    expect(r.header.unitLabel).toBe('R');
    expect(r.evalGrid).toHaveLength(1);
    expect(r.evalGrid![0].sizes.map((s) => s.size)).toEqual([0.25, 0.5, 1, 2]);
    expect(r.gameplan!.sizeLabel).toMatch(/%$/);
    expect(r.quality).not.toBeNull();
    expect(r.quality!.score.caps.map((c) => c.reason)).toContain('variants>=3');
    expect(r.quality!.rolling.segments).toHaveLength(6);
    expect(r.quality!.trueCost.unit).toBe('lot');
    expect(r.quality!.regime).toBeNull();
    expect(r.quality!.killzone).not.toBeNull();
  });

  it('rejects too few trades', () => {
    expect(() => analyzeProdata(prodataOf(raw.slice(0, 5)), rules)).toThrow(/at least 10/);
  });
});

describe('computeTradingScore export', () => {
  it('is re-exported from the index', () => {
    expect(computeTradingScore([]).status).toBe('collecting');
  });
});
