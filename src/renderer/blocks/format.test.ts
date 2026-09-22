import { describe, expect, it } from 'vitest';
import { ddClass, donutDash, fmtDistance, fmtValue, isNegativeText, levelsLadder, pfClass, positiveBar, samePrice, signedBar } from './format';

describe('fmtValue', () => {
  it('formats money with sign in front of the currency', () => {
    expect(fmtValue(-320, 'money')).toBe('-$320');
    expect(fmtValue(4543, 'money')).toMatch(/^\$4.543$/);
    expect(fmtValue(12.5, 'money')).toBe('$12,5');
  });
  it('formats percent and points', () => {
    expect(fmtValue(61, 'percent')).toBe('61%');
    expect(fmtValue(-2.25, 'points')).toBe('-2,25 pt');
  });
  it('falls back to a dash for non-finite', () => {
    expect(fmtValue(Number.NaN)).toBe('—');
  });
});

describe('isNegativeText', () => {
  it('flags leading minus or parenthesis', () => {
    expect(isNegativeText('-$320')).toBe(true);
    expect(isNegativeText('(1.2%)')).toBe(true);
    expect(isNegativeText('12')).toBe(false);
    expect(isNegativeText(-5)).toBe(true);
  });
});

describe('colour classes', () => {
  it('profit factor thresholds', () => {
    expect(pfClass(1.5)).toBe('good');
    expect(pfClass(1.1)).toBe('warn');
    expect(pfClass(0.9)).toBe('bad');
  });
  it('drawdown thresholds', () => {
    expect(ddClass(9)).toBe('good');
    expect(ddClass(15)).toBe('warn');
    expect(ddClass(25)).toBe('bad');
  });
});

describe('levelsLadder', () => {
  const items = [
    { label: 'PDL', price: 7520 },
    { label: 'PDH', price: 7555 },
    { label: 'IBH', price: 7550.25 },
  ];
  it('sorts top-down and inserts the current price row', () => {
    const rows = levelsLadder(items, 7544.75);
    expect(rows.map((r) => (r.kind === 'now' ? 'NOW' : r.label))).toEqual(['PDH', 'IBH', 'NOW', 'PDL']);
    const pdh = rows[0];
    if (pdh.kind !== 'level') throw new Error('expected level');
    expect(pdh.distance).toBeCloseTo(10.25);
    expect(pdh.side).toBe('above');
    const pdl = rows[3];
    if (pdl.kind !== 'level') throw new Error('expected level');
    expect(pdl.side).toBe('below');
  });
  it('puts the now row last when price is below every level, first when above', () => {
    expect(levelsLadder(items, 7000).at(-1)?.kind).toBe('now');
    expect(levelsLadder(items, 9000)[0].kind).toBe('now');
  });
  it('omits the now row and distances without last', () => {
    const rows = levelsLadder(items, undefined);
    expect(rows.every((r) => r.kind === 'level' && r.distance === null)).toBe(true);
  });
  it('fmtDistance keeps the sign', () => {
    expect(fmtDistance(10.25)).toBe('+10,25');
    expect(fmtDistance(-3)).toBe('-3');
    expect(fmtDistance(0)).toBe('0');
  });
});

describe('svg geometry', () => {
  it('donut dash covers the right fraction of the circumference', () => {
    const { circ, dash } = donutDash(50, 10);
    const [on] = dash.split(' ').map(Number);
    expect(on).toBeCloseTo(circ / 2, 1);
    expect(donutDash(150, 10).dash.split(' ')[1]).toBe('0.00');
  });
  it('signed bars grow left for negatives', () => {
    const neg = signedBar(-50, 100, 200);
    expect(neg.w).toBe(50);
    expect(neg.x).toBe(50);
    const pos = signedBar(100, 100, 200);
    expect(pos.x).toBe(100);
    expect(pos.w).toBe(100);
    expect(signedBar(1, 0, 200).w).toBe(0);
  });
  it('positive bars clamp to the track', () => {
    expect(positiveBar(2, 1, 100)).toBe(100);
    expect(positiveBar(0.5, 1, 100)).toBe(50);
    expect(positiveBar(-1, 1, 100)).toBe(0);
  });
});

describe('levelsLadder drops a level that is the current price again', () => {
  const items = [
    { label: 'Текущая цена', price: 7763.75 },
    { label: 'rth.IB+1x', price: 7744.25 },
    { label: 'PDH', price: 7740 },
  ];

  it('keeps one row for that price: the highlighted "now" row', () => {
    const rows = levelsLadder(items, 7763.75);
    expect(rows.filter((r) => r.kind === 'now')).toHaveLength(1);
    expect(rows.some((r) => r.kind === 'level' && r.label === 'Текущая цена')).toBe(false);
    expect(rows[0].kind).toBe('now');
    expect(rows).toHaveLength(3);
  });

  it('keeps the levels that are not the current price', () => {
    const rows = levelsLadder(items, 7763.75).filter((r) => r.kind === 'level');
    expect(rows.map((r) => (r.kind === 'level' ? r.label : ''))).toEqual(['rth.IB+1x', 'PDH']);
  });

  it('without `last` nothing is dropped', () => {
    expect(levelsLadder(items, undefined)).toHaveLength(3);
  });

  it('samePrice tolerates float noise but not a real gap', () => {
    expect(samePrice(7763.750000001, 7763.75)).toBe(true);
    expect(samePrice(7763.5, 7763.75)).toBe(false);
  });
});
