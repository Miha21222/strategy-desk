import { describe, expect, it } from 'vitest';
import {
  arcPath,
  dateOfStateFile,
  defaultWindow,
  discussPrompt,
  factorStats,
  lessonsTail,
  macroChatTitle,
  needleAngle,
  parseCsv,
  splitFactors,
  windowState,
  zoneOf,
  zoneSegments,
  type MacroDay,
  type ScaleFactor,
} from './macro';

const emptyRegion = (region: 'eu' | 'us') => ({ region, scale: null, verdict: null, collect: null, note: null });
const day = (over: Partial<MacroDay> = {}): MacroDay => ({
  date: '2026-10-12',
  eu: emptyRegion('eu'),
  us: emptyRegion('us'),
  review: { mech: null, agent: null, note: null },
  lessons: [],
  stats: [],
  names: {},
  zones: { neutral: 15, strong: 40 },
  ...over,
});

describe('zones and the gauge', () => {
  it('uses the same thresholds as scale.py', () => {
    expect(zoneOf(0)).toBe('neutral');
    expect(zoneOf(14)).toBe('neutral');
    expect(zoneOf(-14.9)).toBe('neutral');
    expect(zoneOf(15)).toBe('long');
    expect(zoneOf(-15)).toBe('short');
    expect(zoneOf(39)).toBe('long');
    expect(zoneOf(40)).toBe('strong-long');
    expect(zoneOf(-58)).toBe('strong-short');
    expect(zoneOf(20, { neutral: 25, strong: 50 })).toBe('neutral');
  });

  it('puts strong short on the left, strong long on the right, neutral on top', () => {
    expect(needleAngle(-100)).toBe(180);
    expect(needleAngle(0)).toBe(90);
    expect(needleAngle(100)).toBe(0);
    expect(needleAngle(250)).toBe(0);
  });

  it('covers the whole dial with five contiguous segments', () => {
    const segs = zoneSegments();
    expect(segs.map((s) => s.key)).toEqual(['strong-short', 'short', 'neutral', 'long', 'strong-long']);
    expect(segs[0].from).toBe(-100);
    expect(segs[4].to).toBe(100);
    for (let i = 1; i < segs.length; i++) expect(segs[i].from).toBe(segs[i - 1].to);
    expect(arcPath(160, 172, 104, 142, -100, -40)).toMatch(/^M18\.00 172\.00 A142 142 0 0 1 /);
  });
});

describe('factors', () => {
  const f = (id: string, side: ScaleFactor['side'], share: number, strength: number, on = true): ScaleFactor => ({ id, name: id, group: 'g', side, share, strength, points: side ? share * strength : 0, on });
  it('splits by side, heaviest first, silent and off factors apart', () => {
    const r = splitFactors([f('a', 'LONG', 10, 0.5), f('b', 'LONG', 6, 1), f('c', 'SHORT', 12, 1), f('d', '', 5, 0), f('e', 'SHORT', 8, 1, false)]);
    expect(r.long.map((x) => x.id)).toEqual(['b', 'a']);
    expect(r.short.map((x) => x.id)).toEqual(['c']);
    expect(r.silent.map((x) => x.id)).toEqual(['d']);
  });
});

describe('windows', () => {
  const at = (date: string, hhmm: string) => ({ date, minutes: Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3)) });
  it('says when a window is due, waiting or missed', () => {
    expect(windowState('eu', null, '2026-10-12', at('2026-10-12', '09:00'))).toBe('waiting');
    expect(windowState('eu', null, '2026-10-12', at('2026-10-12', '09:31'))).toBe('due');
    expect(windowState('eu', null, '2026-10-12', at('2026-10-12', '12:30'))).toBe('missed');
    expect(windowState('us', null, '2026-10-12', at('2026-10-12', '15:58'))).toBe('due');
    expect(windowState('review', null, '2026-10-11', at('2026-10-12', '09:00'))).toBe('missed');
    expect(windowState('review', null, '2026-10-13', at('2026-10-12', '09:00'))).toBe('waiting');
  });
  it('reads ready / partial from the files present', () => {
    const scale = { region: 'eu', date: '2026-10-12', asof: '', sym: 'FDAX', factors: [], result: { long: 0, short: 0, undecided: 100, needle: 0, zone: '', side: null }, light: { color: 'green', label: '' } } as never;
    const d1 = day({ eu: { ...emptyRegion('eu'), scale } });
    expect(windowState('eu', d1, '2026-10-12', at('2026-10-12', '09:40'))).toBe('partial');
    const d2 = day({ eu: { ...emptyRegion('eu'), scale, verdict: { verdict: 'LONG' } } });
    expect(windowState('eu', d2, '2026-10-12', at('2026-10-12', '09:40'))).toBe('ready');
  });
  it('opens the window that matters now', () => {
    expect(defaultWindow(day(), '2026-10-12', at('2026-10-12', '09:40'))).toBe('eu');
    expect(defaultWindow(day(), '2026-10-12', at('2026-10-12', '16:05'))).toBe('us');
    expect(defaultWindow(day(), '2026-10-12', at('2026-10-12', '20:10'))).toBe('review');
    expect(defaultWindow(day({ review: { mech: { date: '', regions: {} }, agent: null, note: null } }), '2026-10-09', at('2026-10-12', '09:00'))).toBe('review');
  });
});

describe('journal files', () => {
  it('parses quoted CSV cells', () => {
    const rows = parseCsv('﻿a,b,c\r\n1,"x, y",3\n4,"he said ""hi""",\n');
    expect(rows).toEqual([
      { a: '1', b: 'x, y', c: '3' },
      { a: '4', b: 'he said "hi"', c: '' },
    ]);
  });
  it('counts factor hits over the last 30 days only, ignoring silent rows', () => {
    const rows = parseCsv(
      [
        'date,region,factor,group,share,side,strength,dir,hit',
        '2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1',
        '2026-10-08,eu,bias_a,g,12,LONG,1,SHORT,0',
        '2026-10-08,eu,smt,g,2,,0,SHORT,',
        '2026-08-01,eu,bias_a,g,12,SHORT,1,SHORT,1',
        '2026-10-13,eu,bias_a,g,12,SHORT,1,SHORT,1',
        '2026-10-08,us,room,g,12,LONG,1,SHORT,0',
      ].join('\n'),
    );
    const s = factorStats(rows, '2026-10-12');
    expect(s.find((x) => x.region === 'eu' && x.factor === 'bias_a')).toEqual({ region: 'eu', factor: 'bias_a', hits: 1, total: 2 });
    expect(s.find((x) => x.factor === 'smt')).toBeUndefined();
    expect(s.find((x) => x.region === 'us')?.total).toBe(1);
  });
  it('takes the newest lessons first', () => {
    expect(lessonsTail('# Уроки\n\n## 2026-10-08\n- 2026-10-08: один\n- 2026-10-08: два\n* 2026-10-09: три\n', 2)).toEqual(['2026-10-09: три', '2026-10-08: два']);
  });
  it('knows which date a state file belongs to', () => {
    expect(dateOfStateFile('scale_2026-10-09_us.json')).toBe('2026-10-09');
    expect(dateOfStateFile('review_2026-10-08.json')).toBe('2026-10-08');
    expect(dateOfStateFile('review_2026-10-08_agent.json')).toBe('2026-10-08');
    expect(dateOfStateFile('macro_2026-10-07_eu_verdict.json')).toBe('2026-10-07');
    expect(dateOfStateFile('scale_backtest_us.json')).toBeNull();
    expect(dateOfStateFile('bias_2026-10-05.json')).toBeNull();
  });
});

describe('what goes to the agent', () => {
  it('states side and share so the skill does not have to ask, and clamps the share', () => {
    const p = discussPrompt('eu', 'LONG', 80, '  DAX у недельного минимума  ');
    expect(p).toContain('регион eu');
    expect(p).toContain('DAX у недельного минимума');
    expect(p).toContain('(LONG)');
    expect(p).toContain('Доля: 50 %');
    expect(p).toContain('scale.py discuss');
  });
  it('names the day chat by date', () => {
    expect(macroChatTitle('2026-10-12')).toBe('Обзор дня 12.10');
  });
});
