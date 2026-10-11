import { describe, expect, it } from 'vitest';
import {
  dayAnswersFile,
  humanHit,
  humanReadFile,
  normalizeDayAnswers,
  normalizeHumanRead,
  arcPath,
  dateOfStateFile,
  defaultWindow,
  discussPrompt,
  factorStats,
  isShotPath,
  legendColor,
  lessonsTail,
  mainShot,
  normalizeShots,
  shotsErrorText,
  shotTabLabel,
  macroChatTitle,
  normalizeScale,
  fmt,
  needleAngle,
  parseCsv,
  splitFactors,
  windowState,
  zoneOf,
  zoneSegments,
  type MacroDay,
  type ScaleFactor,
} from './macro';

const emptyRegion = (region: 'eu' | 'us') => ({ region, scale: null, verdict: null, collect: null, note: null, shots: null, human: null });
const day = (over: Partial<MacroDay> = {}): MacroDay => ({
  date: '2026-10-12',
  eu: emptyRegion('eu'),
  us: emptyRegion('us'),
  review: { mech: null, agent: null, note: null },
  answers: null,
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
    expect(dateOfStateFile('shots_2026-10-09_us.json')).toBe('2026-10-09');
  });
});

describe('chart screenshots', () => {
  const file = {
    date: '2026-10-09', region: 'us', asof: '2026-10-09 15:55', ok: true, error: null,
    shots: [
      { name: 'h1', tf: 'H1', title: 'H1 — зоны и цели дня', path: 'indices-kb/shots/2026-10-09/macro_us/h1.png', verify_ok: true, legend: [] },
      { name: 'm15', tf: 'M15', title: 'M15 — ночь и уровни у открытия', path: 'indices-kb/shots/2026-10-09/macro_us/m15.png', verify_ok: false,
        legend: [{ key: 'manip1+DO', label: 'манипуляция: открытие дня', kind: 'manip', color: '#d32f2f' }, { key: 'x', kind: 'level' }, 'junk'] },
      { name: 'nopath', tf: 'D1' },
    ],
  };

  it('reads the contract and drops what is not a picture', () => {
    const s = normalizeShots(file, 'eu')!;
    expect(s.region).toBe('us');
    expect(s.ok).toBe(true);
    expect(s.error).toBeNull();
    expect(s.shots.map((x) => x.name)).toEqual(['h1', 'm15']);
    expect(s.shots[1].verify_ok).toBe(false);
    expect(s.shots[1].legend).toEqual([{ key: 'manip1+DO', label: 'манипуляция: открытие дня', kind: 'manip', color: '#d32f2f' }]);
  });

  it('survives garbage and a failed run', () => {
    expect(normalizeShots(null, 'eu')).toBeNull();
    expect(normalizeShots([1, 2], 'eu')).toBeNull();
    expect(normalizeShots('x', 'eu')).toBeNull();
    expect(normalizeShots({ hello: 1 }, 'eu')).toBeNull();
    const f = normalizeShots({ ok: false, error: 'dry-run', shots: null }, 'eu')!;
    expect(f).toMatchObject({ ok: false, error: 'dry-run', shots: [], region: 'eu' });
  });

  it('puts M15 in the main view, whatever the order', () => {
    const s = normalizeShots(file, 'us')!;
    expect(mainShot(s.shots)?.name).toBe('m15');
    expect(mainShot([s.shots[0]])?.name).toBe('h1');
    expect(mainShot([])).toBeNull();
  });

  it('names tabs from the timeframe and the title', () => {
    expect(shotTabLabel({ tf: 'M15', title: 'M15 — ночь и уровни у открытия', name: 'm15' })).toBe('M15 · ночь и уровни у открытия');
    expect(shotTabLabel({ tf: 'H1', title: 'зоны дня', name: 'h1' })).toBe('H1 · зоны дня');
    expect(shotTabLabel({ tf: '', title: '', name: 'h4' })).toBe('H4');
  });

  it('words the reason in plain language', () => {
    expect(shotsErrorText('dry-run')).toEqual({ known: 'dry-run', text: '' });
    expect(shotsErrorText(null).known).toBe('none');
    expect(shotsErrorText('TradingView не запущен')).toEqual({ known: null, text: 'TradingView не запущен' });
    const long = shotsErrorText('нет шкалы с планом: ' + 'x'.repeat(200));
    expect(long.text.length).toBeLessThanOrEqual(90);
    expect(long.text.endsWith('…')).toBe(true);
  });

  it('keeps only plain hex colours, else the kind default', () => {
    expect(legendColor({ color: '#1e8e3e', kind: 'target' })).toBe('#1e8e3e');
    expect(legendColor({ color: 'red; background:url(x)', kind: 'manip' })).toBe('#d32f2f');
    expect(legendColor({ color: '', kind: 'weird' })).toBe('#37474f');
  });

  it('accepts only PNGs under indices-kb/shots', () => {
    expect(isShotPath('indices-kb/shots/2026-10-09/macro_us/m15.png')).toBe(true);
    expect(isShotPath('indices-kb\\shots\\2026-10-09\\m15.png')).toBe(true);
    expect(isShotPath('indices-kb/shots/../state/x.png')).toBe(false);
    expect(isShotPath('indices-kb/shots/2026-10-09/m15.jpg')).toBe(false);
    expect(isShotPath('indices-kb/state/m15.png')).toBe(false);
    expect(isShotPath('/etc/indices-kb/shots/a/b.png')).toBe(false);
    expect(isShotPath('C:/indices-kb/shots/a/b.png')).toBe(false);
    expect(isShotPath(42)).toBe(false);
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

describe('older scale files', () => {
  it('derives the percentages a v1 file (weights, points out of max) never had', () => {
    const v1 = {
      region: 'us', date: '2026-10-08', asof: '', sym: 'ES', version: 'v1',
      factors: [
        { id: 'a', name: 'A', group: 'техника', weight: 3, side: 'LONG', strength: 1, points: 3 },
        { id: 'b', name: 'B', group: 'сессия', weight: 1, side: 'SHORT', strength: 0.5, points: 0.5 },
        { id: 'c', name: 'C', group: 'сессия', weight: 1, side: '', strength: 0, points: 0 },
      ],
      result: { long: 3, short: 0.5, max: 5, needle: 71, zone: 'сильный лонг', side: 'LONG', groups: { техника: { long: 3, short: 0, needle: 100 }, сессия: { long: 0, short: 0.5, needle: -100 } } },
      light: { color: 'red', label: '' },
    } as never;
    const s = normalizeScale(v1);
    expect(s.factors.map((f) => [f.share, f.points])).toEqual([[60, 60], [20, 10], [20, 0]]);
    expect(s.result).toMatchObject({ long: 60, short: 10, undecided: 30, needle: 71 });
    expect(s.result.groups?.['техника']).toEqual({ share: 60, long: 60, short: 0, needle: 100 });
    expect(s.result.groups?.['сессия'].share).toBe(40);
  });
  it('leaves a v3 file alone', () => {
    const v3 = { factors: [{ id: 'a', name: 'A', group: 'g', side: 'LONG', strength: 1, share: 12, points: 12 }], result: { long: 12, short: 0, undecided: 88, needle: 12, zone: '', side: null } } as never;
    expect(normalizeScale(v3)).toEqual({ ...(v3 as object), after: undefined });
  });
  it('formats a missing number as a dash instead of throwing', () => {
    expect(fmt(undefined)).toBe('—');
    expect(fmt(1.234, 2)).toBe('1.23');
  });
});

describe("the owner's two files (note 23)", () => {
  it('reads a human read and refuses anything else', () => {
    expect(normalizeHumanRead({ side: 'LONG', at: '2026-10-12 09:31', before_scale: true }, '2026-10-12', 'eu')).toEqual({ date: '2026-10-12', region: 'eu', side: 'LONG', at: '2026-10-12 09:31', before_scale: true });
    expect(normalizeHumanRead({ side: 'UP' }, '2026-10-12', 'eu')).toBeNull();
    expect(normalizeHumanRead('LONG', '2026-10-12', 'eu')).toBeNull();
    expect(normalizeHumanRead({ side: 'NO_TRADE', before_scale: false, region: 'xx' }, '2026-10-12', 'us')?.region).toBe('us');
  });
  it('reads the evening answers and drops bad regions', () => {
    const a = normalizeDayAnswers({ regions: { eu: { trade: 'long', by_rule: 'yes' }, us: { trade: 'maybe' }, xx: { trade: 'short' } }, note: 'ждал вторую манипуляцию' }, '2026-10-12');
    expect(a).toEqual({ date: '2026-10-12', at: '', regions: { eu: { trade: 'long', by_rule: 'yes' } }, note: 'ждал вторую манипуляцию' });
    expect(normalizeDayAnswers({ regions: {} }, '2026-10-12')).toBeNull();
    expect(normalizeDayAnswers({ regions: { us: { trade: 'none', by_rule: 'what' } }, note: 7 }, '2026-10-12')?.regions.us).toEqual({ trade: 'none' });
  });
  it('scores the read like scale.py scores the needle', () => {
    expect(humanHit('LONG', 'LONG')).toBe(true);
    expect(humanHit('SHORT', 'LONG')).toBe(false);
    expect(humanHit('NEUTRAL', 'LONG')).toBeNull();
    expect(humanHit('LONG', 'FLAT')).toBeNull();
    expect(humanHit(null, 'LONG')).toBeNull();
  });
  it('names the files and lets the watcher see them', () => {
    expect(humanReadFile('2026-10-12', 'us')).toBe('human_read_2026-10-12_us.json');
    expect(dayAnswersFile('2026-10-12')).toBe('day_answers_2026-10-12.json');
    expect(dateOfStateFile('human_read_2026-10-12_us.json')).toBe('2026-10-12');
    expect(dateOfStateFile('day_answers_2026-10-12.json')).toBe('2026-10-12');
  });
});
