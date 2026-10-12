import { existsSync, mkdtempSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { listDays, loadDay, readShot, resolveNote, resolveShot, writeDayAnswers, writeHumanRead } from './macro-state';

const PNG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function vault(): string {
  const root = mkdtempSync(path.join(tmpdir(), 'macro-vault-'));
  const state = path.join(root, 'indices-kb', 'state');
  const journal = path.join(root, 'indices-kb', 'journal');
  mkdirSync(state, { recursive: true });
  mkdirSync(journal, { recursive: true });
  mkdirSync(path.join(root, 'indices-kb', 'config'), { recursive: true });
  mkdirSync(path.join(root, 'Макро-обзоры'), { recursive: true });
  const scale = {
    region: 'eu', date: '2026-10-08', asof: '2026-10-08 09:35', sym: 'FDAX', version: 'v3.1',
    factors: [{ id: 'bias_a', name: 'Точка A / BIAS', group: 'контекст дня', side: 'SHORT', strength: 1, share: 12.4, points: 12.4 }],
    result: { long: 7.4, short: 65.5, undecided: 27.1, needle: -58, zone: 'сильный шорт', side: 'SHORT' },
    light: { color: 'yellow', label: '🟡 осторожно', risk_pct: 0.5 },
  };
  writeFileSync(path.join(state, 'scale_2026-10-08_eu.json'), '﻿' + JSON.stringify(scale));
  writeFileSync(path.join(state, 'macro_2026-10-08_eu.json'), JSON.stringify({ asof_eet: '09:31', headlines: { count: 284, relevant: 138, tone_sum: -5, tags: {}, feeds: [1, 2, 3] }, markets: [{ symbol: 'X', label: 'FDAX', chg_pct: -0.5, main: true, last: 1 }], verdict: { verdict: 'NEUTRAL' } }));
  writeFileSync(path.join(state, 'macro_2026-10-08_eu_verdict.json'), JSON.stringify({ verdict: 'SHORT', confidence: 'medium' }));
  writeFileSync(path.join(state, 'review_2026-10-07.json'), JSON.stringify({ date: '2026-10-07', regions: {} }));
  writeFileSync(path.join(state, 'scale_backtest_eu.json'), '{}');
  writeFileSync(path.join(state, 'bias_2026-10-09.json'), '{}');
  writeFileSync(path.join(state, 'scale_2026-10-09_us.json'), '{not json');
  writeFileSync(path.join(root, 'Макро-обзоры', '2026-10-08 Европа.md'), '# note');
  const shots = {
    date: '2026-10-08', region: 'eu', asof: '2026-10-08 09:40', ok: true, error: null,
    shots: [{ name: 'm15', tf: 'M15', title: 'M15 — ночь', path: 'indices-kb/shots/2026-10-08/macro_eu/m15.png', verify_ok: true,
      legend: [{ key: 'NYM', label: 'полночь Нью-Йорка', kind: 'open', color: '#9e9e9e' }] }],
  };
  writeFileSync(path.join(state, 'shots_2026-10-08_eu.json'), '﻿' + JSON.stringify(shots));
  writeFileSync(path.join(state, 'shots_2026-10-08_us.json'), '{"ok": tru');
  writeFileSync(path.join(state, 'shots_2026-10-07_eu.json'), JSON.stringify({ date: '2026-10-07', region: 'eu', asof: '', ok: false, error: 'dry-run', shots: [] }));
  const shotDir = path.join(root, 'indices-kb', 'shots', '2026-10-08', 'macro_eu');
  mkdirSync(shotDir, { recursive: true });
  writeFileSync(path.join(shotDir, 'm15.png'), Buffer.concat([PNG, Buffer.from('rest')]));
  writeFileSync(path.join(shotDir, 'fake.png'), 'not a png at all');
  writeFileSync(path.join(root, 'indices-kb', 'state', 'secret.png'), PNG);
  // Note 23 §2–3: the memory cards. 2026-10-09 is a Friday.
  writeFileSync(path.join(state, 'remember_2026-10-08_eu.json'), '﻿' + JSON.stringify({ date: '2026-10-08', region: 'eu', asof: '2026-10-08 09:31', items: [{ kind: 'lesson', text: 'В пилу ждать вторую манипуляцию', why: 'те же теги', ref: 'lessons.md 01.10', tags: ['eu', 'пила'] }], conditions: { tags: ['eu'] }, similar: null, lesson_candidates: 2, enough_history: true }));
  writeFileSync(path.join(state, 'remember_2026-10-08_us.json'), '{"items": [');
  writeFileSync(path.join(state, 'drift_2026-10-08_eu.json'), JSON.stringify({ date: '2026-10-08', region: 'eu', days: 20, enough_data: true, signals: [{ kind: 'phase', text: 'фаза сменилась: тренд → пила', severity: 'warn', data: {} }] }));
  writeFileSync(path.join(state, 'drift_2026-10-08_us.json'), JSON.stringify({ date: '2026-10-08', region: 'us', days: 20, enough_data: true, signals: [] }));
  writeFileSync(path.join(state, 'week_2026-10-09.json'), JSON.stringify({ week_start: '2026-10-05', week_end: '2026-10-09', days: 5, enough_data: true, lines: ['1', '2', '3', '4', '5'], stats: {}, trader: ['из недели'], share_proposal: null }));
  writeFileSync(path.join(state, 'review_2026-10-09.json'), JSON.stringify({ date: '2026-10-09', regions: {}, trader: ['из дня', 7, ''] }));
  writeFileSync(path.join(state, 'ideas.json'), JSON.stringify({ asof: '2026-10-09 20:10', items: [{ id: 'a', date: '2026-10-01', text: 'идея', side: 'LONG', region: 'eu', source: 'chat', confirmed: 3, refuted: 0, status: 'proposed', backtest: null, last_check: '2026-10-08' }] }));
  writeFileSync(path.join(journal, 'lessons.md'), '- 2026-10-07: старый\n- 2026-10-08: новый\n');
  writeFileSync(path.join(journal, 'scale_factor_scores.csv'), 'date,region,factor,group,share,side,strength,dir,hit\n2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1\n');
  writeFileSync(path.join(root, 'indices-kb', 'config', 'scale.json'), JSON.stringify({ zones: { neutral: 15, strong: 40 }, regions: { eu: { factors: [{ id: 'bias_a', name: 'Точка A / BIAS' }] } } }));
  return root;
}

describe('the vault reader', () => {
  it('lists macro days newest first and ignores backtests and other files', async () => {
    const v = vault();
    expect(await listDays(v)).toEqual({ vault: v, ok: true, days: ['2026-10-09', '2026-10-08', '2026-10-07'] });
    expect((await listDays('')).error).toBe('no-vault');
    expect((await listDays(path.join(v, 'nope'))).error).toBe('no-state-dir');
  });

  it('loads a day: scale with a BOM, the verdict file over the copy, trimmed headlines, the note, journal', async () => {
    const v = vault();
    const d = await loadDay(v, '2026-10-08');
    expect(d.eu.scale?.result.needle).toBe(-58);
    expect(d.eu.verdict?.verdict).toBe('SHORT');
    expect(d.eu.collect?.headlines).toEqual({ count: 284, relevant: 138, tone_sum: -5, tags: {} });
    expect(d.eu.collect?.markets).toEqual([{ label: 'FDAX', chg_pct: -0.5, main: true, kind: undefined }]);
    expect(d.eu.note).toBe(path.join('Макро-обзоры', '2026-10-08 Европа.md'));
    expect(d.us.scale).toBeNull();
    expect(d.review.mech).toBeNull();
    expect(d.lessons).toEqual(['2026-10-08: новый', '2026-10-07: старый']);
    expect(d.stats).toEqual([{ region: 'eu', factor: 'bias_a', hits: 1, total: 1 }]);
    expect(d.names.bias_a).toBe('Точка A / BIAS');
  });

  it('reads the memory cards of note 23 §2–3, and shows nothing where there is nothing', async () => {
    const v = vault();
    const d = await loadDay(v, '2026-10-08');
    expect(d.eu.remember?.items).toEqual([{ kind: 'lesson', text: 'В пилу ждать вторую манипуляцию', why: 'те же теги', ref: 'lessons.md 01.10', tags: ['eu', 'пила'] }]);
    expect(d.us.remember).toBeNull();
    expect(d.eu.drift?.signals[0]).toMatchObject({ kind: 'phase', severity: 'warn' });
    expect(d.us.drift?.signals).toEqual([]);
    expect(d.week).toBeNull();
    expect(d.trader).toEqual([]);
    expect(d.ideas?.items[0]).toMatchObject({ id: 'a', status: 'proposed', confirmed: 3 });
    const fri = await loadDay(v, '2026-10-09');
    expect(fri.week?.lines).toEqual(['1', '2', '3', '4', '5']);
    expect(fri.week?.trader).toEqual(['из недели']);
    expect(fri.trader).toEqual(['из дня']);
    expect(fri.eu.remember).toBeNull();
    expect(fri.eu.drift).toBeNull();
    expect((await listDays(v)).days).toEqual(['2026-10-09', '2026-10-08', '2026-10-07']);
  });

  it('survives a half-written file', async () => {
    const d = await loadDay(vault(), '2026-10-09');
    expect(d.us.scale).toBeNull();
  });

  it('reads the screenshots file of a window, and tolerates a BOM or garbage', async () => {
    const v = vault();
    const d = await loadDay(v, '2026-10-08');
    expect(d.eu.shots?.ok).toBe(true);
    expect(d.eu.shots?.shots[0].legend[0].label).toBe('полночь Нью-Йорка');
    expect(d.us.shots).toBeNull();
    const d7 = await loadDay(v, '2026-10-07');
    expect(d7.eu.shots).toMatchObject({ ok: false, error: 'dry-run', shots: [] });
    expect(d7.us.shots).toBeNull();
  });

  it('gives out only PNGs inside indices-kb/shots', async () => {
    const v = vault();
    const rel = 'indices-kb/shots/2026-10-08/macro_eu/m15.png';
    expect(resolveShot(v, rel)).toBe(path.join(v, 'indices-kb', 'shots', '2026-10-08', 'macro_eu', 'm15.png'));
    expect(resolveShot(v, 'indices-kb/shots/../state/secret.png')).toBeNull();
    expect(resolveShot(v, 'indices-kb/shots/2026-10-08/../../state/secret.png')).toBeNull();
    expect(resolveShot(v, 'indices-kb/state/secret.png')).toBeNull();
    expect(resolveShot(v, 'indices-kb/shots/2026-10-08/macro_eu/m15.json')).toBeNull();
    expect(resolveShot(v, path.join(v, rel))).toBeNull();
    expect(resolveShot('', rel)).toBeNull();
    const url = await readShot(v, rel);
    expect(url?.startsWith('data:image/png;base64,')).toBe(true);
    expect(Buffer.from(url!.split(',')[1], 'base64').subarray(0, 8).equals(PNG)).toBe(true);
    expect(await readShot(v, 'indices-kb/shots/2026-10-08/macro_eu/fake.png')).toBeNull();
    expect(await readShot(v, 'indices-kb/shots/2026-10-08/macro_eu/missing.png')).toBeNull();
    expect(await readShot(v, 'indices-kb/state/secret.png')).toBeNull();
  });

  it('opens only markdown notes inside the notes folder', () => {
    const v = vault();
    expect(resolveNote(v, path.join('Макро-обзоры', '2026-10-08 Европа.md'))).toBe(path.join(v, 'Макро-обзоры', '2026-10-08 Европа.md'));
    expect(resolveNote(v, path.join('Макро-обзоры', '..', 'AGENTS.md'))).toBeNull();
    expect(resolveNote(v, path.join('Макро-обзоры', 'x.exe'))).toBeNull();
    expect(resolveNote('', 'Макро-обзоры/x.md')).toBeNull();
  });
});

describe("the owner's two files (note 23)", () => {
  it('writes the read and the answers, and reads them back with the day', async () => {
    const v = vault();
    const read = await writeHumanRead(v, '2026-10-08', 'eu', 'SHORT', true);
    expect(read?.side).toBe('SHORT');
    expect(read?.at).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
    const ans = await writeDayAnswers(v, '2026-10-08', { regions: { eu: { trade: 'short', by_rule: 'yes' }, us: { trade: 'none' } }, note: '  ждал   вторую\nманипуляцию ' });
    expect(ans?.note).toBe('ждал вторую манипуляцию');
    const d = await loadDay(v, '2026-10-08');
    expect(d.eu.human).toEqual(read);
    expect(d.us.human).toBeNull();
    expect(d.answers).toEqual(ans);
    expect(existsSync(path.join(v, 'indices-kb', 'state', 'human_read_2026-10-08_eu.json'))).toBe(true);
    expect(readdirSync(path.join(v, 'indices-kb', 'state')).filter((n) => n.endsWith('.tmp'))).toEqual([]);
  });

  it('refuses a bad date, region or side, and writes nothing then', async () => {
    const v = vault();
    expect(await writeHumanRead(v, '2026-10-08x', 'eu', 'LONG', true)).toBeNull();
    expect(await writeHumanRead(v, '2026-10-08', 'xx' as never, 'LONG', true)).toBeNull();
    expect(await writeHumanRead(v, '2026-10-08', 'eu', 'UP' as never, true)).toBeNull();
    expect(await writeDayAnswers(v, '../2026-10-08', { regions: { eu: { trade: 'long' } }, note: '' })).toBeNull();
    expect(await writeDayAnswers(v, '2026-10-08', { regions: {}, note: 5 as never })).toBeNull();
    expect(await writeHumanRead('', '2026-10-08', 'eu', 'LONG', true)).toBeNull();
    expect(readdirSync(path.join(v, 'indices-kb', 'state')).filter((n) => /^(human_read|day_answers)_/.test(n))).toEqual([]);
  });
});
