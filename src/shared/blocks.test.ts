import { describe, expect, it } from 'vitest';
import { BLOCK_KINDS, BLOCK_SCHEMAS, costPerTrade, isBlockKind, netAfterCosts, parseBlock, weightedWinRate, type StressReport } from './blocks';
import { SAMPLE_BLOCKS } from './blocks-sample';

describe('block schemas', () => {
  it('accept the canned sample for every kind', () => {
    for (const kind of BLOCK_KINDS) {
      const res = parseBlock(kind, JSON.stringify(SAMPLE_BLOCKS[kind]));
      expect(res.ok, `${kind}: ${res.ok ? '' : res.error}`).toBe(true);
      if (res.ok) expect(res.block.kind).toBe(kind);
    }
  });

  it('every kind has a schema and isBlockKind is case-insensitive', () => {
    for (const kind of BLOCK_KINDS) expect(BLOCK_SCHEMAS[kind]).toBeDefined();
    expect(isBlockKind('StressReport')).toBe(true);
    expect(isBlockKind('python')).toBe(false);
  });

  it('nextsteps: rejects more than 4 options and an option without prompt/action', () => {
    const five = { options: Array.from({ length: 5 }, (_, i) => ({ label: `o${i}`, prompt: 'x' })) };
    expect(parseBlock('nextsteps', JSON.stringify(five)).ok).toBe(false);
    const bad = parseBlock('nextsteps', JSON.stringify({ options: [{ label: 'no prompt' }] }));
    expect(bad.ok).toBe(false);
    if (!bad.ok) expect(bad.error).toContain('options');
  });

  it('chartcard: rejects an unknown chart type and a donut over 100%', () => {
    expect(parseBlock('chartcard', JSON.stringify({ title: 't', charts: [{ type: 'pie', items: [] }] })).ok).toBe(false);
    expect(parseBlock('chartcard', JSON.stringify({ title: 't', charts: [{ type: 'donut', pct: 120 }] })).ok).toBe(false);
  });

  it('chartcard: bars format defaults to number', () => {
    const res = parseBlock('chartcard', JSON.stringify({ title: 't', charts: [{ type: 'bars', items: [{ label: 'a', value: 1 }] }] }));
    expect(res.ok).toBe(true);
    if (res.ok && res.block.kind === 'chartcard') {
      const c = res.block.data.charts[0];
      expect(c.type === 'bars' && c.format).toBe('number');
    }
  });

  it('stressreport: rejects a grade outside A-F and a role outside baseline|variant|oos', () => {
    const base = SAMPLE_BLOCKS.stressreport as StressReport;
    expect(parseBlock('stressreport', JSON.stringify({ ...base, grade: 'E' })).ok).toBe(false);
    const badRole = { ...base, runs: [{ ...base.runs[0], role: 'live' }] };
    expect(parseBlock('stressreport', JSON.stringify(badRole)).ok).toBe(false);
  });

  it('prodata: market must be futures|forex|cfd and trades ≤ 2000', () => {
    const base = SAMPLE_BLOCKS.prodata as Record<string, unknown>;
    expect(parseBlock('prodata', JSON.stringify({ ...base, market: 'crypto' })).ok).toBe(false);
    const many = { ...base, trades: Array.from({ length: 2001 }, () => ({ d: '2025-01-01', pnl: 1 })) };
    expect(parseBlock('prodata', JSON.stringify(many)).ok).toBe(false);
  });

  it('briefreport: hit defaults to null and impact must be known', () => {
    const res = parseBlock('briefreport', JSON.stringify({ date: '2026-09-19', symbol: 'ES', levels: [{ name: 'PDH', price: 1 }] }));
    expect(res.ok).toBe(true);
    if (res.ok && res.block.kind === 'briefreport') expect(res.block.data.levels[0].hit).toBeNull();
    const bad = parseBlock('briefreport', JSON.stringify({ date: 'd', symbol: 's', news: [{ time: '10:00', title: 'x', impact: 'huge' }] }));
    expect(bad.ok).toBe(false);
  });

  it('healthdata: rejects a trade without pnl', () => {
    expect(parseBlock('healthdata', JSON.stringify({ strategy: 's', trades: [{ d: '2025-01-01' }] })).ok).toBe(false);
  });

  it('workflowstate: rejects an unknown verdict and confidence > 1', () => {
    const base = SAMPLE_BLOCKS.workflowstate as Record<string, unknown>;
    expect(parseBlock('workflowstate', JSON.stringify({ ...base, verdict: 'MAYBE' })).ok).toBe(false);
    expect(parseBlock('workflowstate', JSON.stringify({ ...base, bias: { direction: 'LONG', confidence: 1.5 } })).ok).toBe(false);
  });

  it('reports invalid JSON compactly and never throws', () => {
    const res = parseBlock('chartcard', '{not json');
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.kind).toBe('chartcard');
      expect(res.error.startsWith('invalid JSON')).toBe(true);
    }
    expect(parseBlock('nope', '{}').ok).toBe(false);
  });

  it('caps the issue list in the error text', () => {
    const res = parseBlock('stressreport', JSON.stringify({ runs: [{}] }));
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.error.split(';').length).toBeLessThanOrEqual(5);
  });
});

describe('derived numbers', () => {
  const costs = { commissionRT: 4, slippageTicksPerSide: 1, tickValue: 5 };
  it('costPerTrade = commission + 2 × slippage × tickValue', () => {
    expect(costPerTrade(costs)).toBe(14);
  });
  it('netAfterCosts subtracts trades × costPerTrade, null without costs', () => {
    const run = { label: 'x', trades: 10, winRate: 50, profitFactor: 1, netProfit: 500, maxDrawdownPct: 5, role: 'baseline' as const };
    expect(netAfterCosts(run, costs)).toBe(360);
    expect(netAfterCosts(run, null)).toBeNull();
  });
  it('weightedWinRate weights by trades', () => {
    const runs = [
      { label: 'a', trades: 10, winRate: 60, profitFactor: 1, netProfit: 0, maxDrawdownPct: 0, role: 'baseline' as const },
      { label: 'b', trades: 30, winRate: 40, profitFactor: 1, netProfit: 0, maxDrawdownPct: 0, role: 'variant' as const },
    ];
    expect(weightedWinRate(runs)).toBe(45);
    expect(weightedWinRate([])).toBe(0);
  });
});
