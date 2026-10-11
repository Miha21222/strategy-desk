import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { listDays, loadDay, resolveNote } from './macro-state';

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

  it('survives a half-written file', async () => {
    const d = await loadDay(vault(), '2026-10-09');
    expect(d.us.scale).toBeNull();
  });

  it('opens only markdown notes inside the notes folder', () => {
    const v = vault();
    expect(resolveNote(v, path.join('Макро-обзоры', '2026-10-08 Европа.md'))).toBe(path.join(v, 'Макро-обзоры', '2026-10-08 Европа.md'));
    expect(resolveNote(v, path.join('Макро-обзоры', '..', 'AGENTS.md'))).toBeNull();
    expect(resolveNote(v, path.join('Макро-обзоры', 'x.exe'))).toBeNull();
    expect(resolveNote('', 'Макро-обзоры/x.md')).toBeNull();
  });
});
