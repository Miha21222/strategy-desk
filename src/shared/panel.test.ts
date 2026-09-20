import { describe, expect, it } from 'vitest';
import {
  adrFromScan,
  buildSentinelSpec,
  emptyPanelDay,
  eventPromptVars,
  eventsNeedingModel,
  isAllowedTvTool,
  levelsArgs,
  levelsFromScan,
  mergeEvents,
  nextPollDelayMs,
  PanelSchema,
  POLL_CEILING_S,
  POLL_FLOOR_S,
  renderPrompt,
  shouldAskModel,
  shouldContinuePolling,
  todayISO,
  TV_TOOL_ALLOWLIST,
  type PanelDef,
  type SentinelEvent,
} from './panel';

const panel = (over: Record<string, unknown> = {}): PanelDef =>
  PanelSchema.parse({
    kind: 'session-watch',
    sentinel: { symbol: 'PEPPERSTONE:US500', timeframe: 'M5', session: { start: '16:30', end: '23:00', tz: 'Europe/Kyiv' } },
    askModelOn: ['RANGE_CLOSED', 'BREAK_CONFIRMED', 'SWEEP', 'CLOCK'],
    ...over,
  });

// ---------- the panel schema ----------

describe('PanelSchema', () => {
  it('fills the defaults a workflow may leave out', () => {
    const p = panel();
    expect(p.kind).toBe('session-watch');
    expect(p.clocks).toEqual([]);
    expect(p.levelNames).toEqual([]);
    expect(p.pollFloorSeconds).toBe(5);
    expect(p.prompts).toEqual({});
  });

  it('refuses a session watch with no symbol', () => {
    const r = PanelSchema.safeParse({ kind: 'session-watch', sentinel: { timeframe: 'M5' } });
    expect(r.success).toBe(false);
    expect(r.success ? '' : r.error.issues[0].path.join('.')).toBe('sentinel.symbol');
  });

  it('refuses an unknown kind', () => {
    expect(PanelSchema.safeParse({ kind: 'crystal-ball' }).success).toBe(false);
  });

  it('accepts a risk desk with no sentinel at all', () => {
    const p = PanelSchema.parse({ kind: 'risk-desk', risk: { account: { starting_balance: 10000 } }, journal: { limit: 50 } });
    expect(p.kind).toBe('risk-desk');
    expect(p.sentinel).toBeUndefined();
    expect((p.risk as { account: { starting_balance: number } }).account.starting_balance).toBe(10000);
  });

  it('passes an unknown instrument and session through untouched, so a second workflow needs no code change', () => {
    const p = PanelSchema.parse({
      kind: 'session-watch',
      sentinel: { symbol: 'OANDA:XAUUSD', timeframe: 'M15', session: { name: 'London' }, range: { source: 'window', start: '10:00', minutes: 60 } },
    });
    expect(p.sentinel).toEqual({ symbol: 'OANDA:XAUUSD', timeframe: 'M15', session: { name: 'London' }, range: { source: 'window', start: '10:00', minutes: 60 } });
  });

  it('validates clock shape', () => {
    expect(PanelSchema.safeParse({ kind: 'risk-desk', clocks: [{ name: 'x', at: 'quarter to four' }] }).success).toBe(false);
    const p = PanelSchema.parse({ kind: 'risk-desk', clocks: [{ name: 'verdict', at: '15:45', tz: 'Europe/Kyiv' }] });
    expect(p.clocks[0]).toEqual({ name: 'verdict', at: '15:45', tz: 'Europe/Kyiv' });
  });

  it('defaults a clock timezone to UTC rather than guessing', () => {
    expect(PanelSchema.parse({ kind: 'risk-desk', clocks: [{ name: 'c', at: '09:00' }] }).clocks[0].tz).toBe('UTC');
  });
});

// ---------- the allowlist ----------

describe('the tool allowlist', () => {
  it('holds only read-and-poll tools', () => {
    expect([...TV_TOOL_ALLOWLIST].sort()).toEqual(
      [
        'tv_calendar_check',
        'tv_desktop_status',
        'tv_journal_load',
        'tv_risk_guard',
        'tv_scan_levels',
        'tv_sentinel_poll',
        'tv_sentinel_start',
        'tv_sentinel_status',
        'tv_sentinel_stop',
      ].sort(),
    );
  });

  it('accepts every listed tool', () => {
    for (const name of TV_TOOL_ALLOWLIST) expect(isAllowedTvTool(name)).toBe(true);
  });

  it('rejects a tool that is not listed, however real it is', () => {
    for (const name of ['tv_desktop_draw', 'tv_desktop_pine_save', 'tv_journal_parse', 'tv_backtest_run', 'tv_setup_doctor']) {
      expect(isAllowedTvTool(name)).toBe(false);
    }
  });

  it('rejects junk without throwing', () => {
    for (const bad of ['', ' tv_scan_levels', 'TV_SCAN_LEVELS', 'tv_scan_levels;rm -rf', null, undefined, 42, {}, ['tv_scan_levels']]) {
      expect(isAllowedTvTool(bad)).toBe(false);
    }
  });
});

// ---------- levels -> sentinel spec ----------

const scan = {
  levels: [
    { name: 'Previous day high', price: 7722.8 },
    { name: 'Previous day low', price: 7697.9 },
    { name: 'ETH low', price: 7682.5 },
    { name: 'RTH opening range', high: 7712.6, low: 7683.4 },
    { name: 'broken', price: null },
    { name: 'also broken' },
    { name: 'Previous day high', price: 1 },
  ],
  adr: { value: 47.7, n_used: 5 },
};

describe('levelsFromScan', () => {
  it('keeps prices and zones and drops anything it cannot price', () => {
    expect(levelsFromScan(scan)).toEqual([
      { name: 'Previous day high', price: 7722.8 },
      { name: 'Previous day low', price: 7697.9 },
      { name: 'ETH low', price: 7682.5 },
      { name: 'RTH opening range', high: 7712.6, low: 7683.4 },
    ]);
  });

  it('honours a name filter', () => {
    expect(levelsFromScan(scan, ['ETH low', 'nothing here'])).toEqual([{ name: 'ETH low', price: 7682.5 }]);
  });

  it('survives a failed scan', () => {
    expect(levelsFromScan(null)).toEqual([]);
    expect(levelsFromScan({ error: 'boom' })).toEqual([]);
    expect(adrFromScan(null)).toBeNull();
    expect(adrFromScan({ adr: { value: 0 } })).toBeNull();
    expect(adrFromScan(scan)).toBe(47.7);
  });
});

describe('levelsArgs', () => {
  it('defaults the symbol and provider to the ones the watch itself uses', () => {
    const p = panel({
      sentinel: { symbol: 'PEPPERSTONE:US500', provider: 'session', session: { name: 'New York' } },
      levels: { include: ['prev_day'], timeframe: 'M15' },
    });
    expect(levelsArgs(p, '2026-09-08')).toEqual({ include: ['prev_day'], timeframe: 'M15', date: '2026-09-08', symbol: 'PEPPERSTONE:US500', provider: 'session' });
  });

  it('lets a workflow measure levels on a different symbol or feed', () => {
    const p = panel({
      sentinel: { symbol: 'PEPPERSTONE:US500', provider: 'session', session: { name: 'New York' } },
      levels: { symbol: 'CME_MINI:ES1!', provider: 'auto' },
    });
    expect(levelsArgs(p, '2026-09-08')).toEqual({ symbol: 'CME_MINI:ES1!', provider: 'auto', date: '2026-09-08' });
  });

  it('is null when the workflow asks for no levels', () => {
    expect(levelsArgs(PanelSchema.parse({ kind: 'risk-desk' }), '2026-09-08')).toBeNull();
  });
});

describe('buildSentinelSpec', () => {
  it('adds the day, the levels and the ADR the yaml left out', () => {
    const spec = buildSentinelSpec(panel({ clocks: [{ name: 'verdict', at: '15:45', tz: 'Europe/Kyiv' }] }), {
      day: '2026-09-08',
      levels: levelsFromScan(scan),
      adr: adrFromScan(scan),
    });
    expect(spec.day).toBe('2026-09-08');
    expect((spec.levels as unknown[]).length).toBe(4);
    expect(spec.range).toEqual({ adr: 47.7 });
    expect(spec.clocks).toEqual([{ name: 'verdict', at: '15:45', tz: 'Europe/Kyiv' }]);
  });

  it('never overwrites an ADR the yaml stated', () => {
    const p = panel({ sentinel: { symbol: 'X', session: { name: 'London' }, range: { source: 'session_open', minutes: 30, adr: 12 } } });
    expect((buildSentinelSpec(p, { day: '2026-01-02', adr: 99 }).range as { adr: number }).adr).toBe(12);
  });

  it('drops watch level names the scan did not produce, instead of letting the tool refuse the whole spec', () => {
    const p = panel({
      sentinel: {
        symbol: 'X',
        session: { name: 'London' },
        watch: { sweep: true, sweep_levels: ['ETH low', 'Level that did not load'], break_levels: ['Nothing at all'] },
      },
    });
    const spec = buildSentinelSpec(p, { day: '2026-01-02', levels: levelsFromScan(scan) });
    expect((spec.watch as { sweep_levels: string[] }).sweep_levels).toEqual(['ETH low']);
    expect((spec.watch as Record<string, unknown>).break_levels).toBeUndefined();
  });

  it('omits levels entirely when the scan failed', () => {
    const spec = buildSentinelSpec(panel(), { day: '2026-01-02', levels: [] });
    expect(spec.levels).toBeUndefined();
  });
});

// ---------- the poll scheduler ----------

const running = { status: 'running', phase: 'range_forming', waiting_for: 'range_close' };

describe('the poll scheduler', () => {
  it('takes the interval the run itself asked for', () => {
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 75 })).toBe(75_000);
  });

  it('never polls faster than the floor, whatever the run says', () => {
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 1 })).toBe(POLL_FLOOR_S * 1000);
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 0 })).toBe(POLL_FLOOR_S * 1000);
    expect(nextPollDelayMs({ state: running, next_poll_after_s: -30 })).toBe(POLL_FLOOR_S * 1000);
    expect(nextPollDelayMs({ state: running })).toBe(POLL_FLOOR_S * 1000);
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 'soon' })).toBe(POLL_FLOOR_S * 1000);
  });

  it('honours a workflow of its own, slower floor', () => {
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 5 }, 60)).toBe(60_000);
  });

  it('caps a run that asks for a very long wait', () => {
    expect(nextPollDelayMs({ state: running, next_poll_after_s: 99_999 })).toBe(POLL_CEILING_S * 1000);
  });

  it('never polls a run that is done', () => {
    for (const state of [
      { status: 'stopped', phase: 'range_closed' },
      { status: 'finished', phase: 'range_closed' },
      { status: 'running', phase: 'session_closed' },
      { status: 'running', phase: 'stopped' },
      { status: 'running', phase: 'range_closed', replay: { from: 'a', to: 'b' } },
      { status: 'running', phase: 'range_closed', flags: { replay_done: true } },
    ]) {
      expect(shouldContinuePolling(state)).toBe(false);
      expect(nextPollDelayMs({ state, next_poll_after_s: 60 })).toBeNull();
    }
  });

  it('never polls without a state at all', () => {
    expect(shouldContinuePolling(null)).toBe(false);
    expect(nextPollDelayMs(null)).toBeNull();
    expect(nextPollDelayMs({ state: null, next_poll_after_s: 60 })).toBeNull();
  });

  it('keeps polling a live run in every phase before the close', () => {
    for (const phase of ['pending', 'session_open', 'range_forming', 'range_closed']) {
      expect(shouldContinuePolling({ status: 'running', phase })).toBe(true);
    }
  });
});

// ---------- the event -> model-turn rule ----------

const ev = (seq: number, type: string): SentinelEvent => ({ seq, ts_utc: `2026-09-08T14:0${seq}:00Z`, type, payload: { x: seq } });

describe('the event -> model-turn rule', () => {
  it('fires only for the types the workflow listed', () => {
    const p = panel();
    expect(shouldAskModel(p, 'RANGE_CLOSED')).toBe(true);
    expect(shouldAskModel(p, 'BREAK_CONFIRMED')).toBe(true);
    expect(shouldAskModel(p, 'SWEEP')).toBe(true);
    expect(shouldAskModel(p, 'CLOCK')).toBe(true);
    for (const quiet of ['SESSION_OPEN', 'RANGE_OPEN', 'BREAK', 'RETEST', 'LEVEL_TAGGED', 'STALE_DATA', 'NO_DATA', 'SESSION_CLOSE']) {
      expect(shouldAskModel(p, quiet)).toBe(false);
    }
  });

  it('never fires when the workflow listed nothing', () => {
    const p = PanelSchema.parse({ kind: 'risk-desk' });
    expect(shouldAskModel(p, 'RANGE_CLOSED')).toBe(false);
    expect(eventsNeedingModel(p, [ev(1, 'RANGE_CLOSED')])).toEqual([]);
  });

  it('never fires without a panel', () => {
    expect(shouldAskModel(null, 'RANGE_CLOSED')).toBe(false);
    expect(eventsNeedingModel(undefined, [ev(1, 'RANGE_CLOSED')])).toEqual([]);
  });

  it('filters a whole poll down to the events worth a turn, oldest first', () => {
    const p = panel();
    const got = eventsNeedingModel(p, [ev(4, 'BREAK_CONFIRMED'), ev(1, 'SESSION_OPEN'), ev(3, 'BREAK'), ev(2, 'RANGE_CLOSED'), ev(5, 'LEVEL_TAGGED')]);
    expect(got.map((e) => [e.seq, e.type])).toEqual([
      [2, 'RANGE_CLOSED'],
      [4, 'BREAK_CONFIRMED'],
    ]);
  });

  it('a busy session of noise costs nothing', () => {
    const p = panel();
    const noise = Array.from({ length: 50 }, (_, i) => ev(i + 1, i % 2 ? 'LEVEL_TAGGED' : 'BREAK'));
    expect(eventsNeedingModel(p, noise)).toEqual([]);
  });
});

describe('prompt rendering', () => {
  it('fills the variables the yaml template names', () => {
    const vars = eventPromptVars(ev(7, 'RANGE_CLOSED'), { phase: 'range_closed' }, { date: '2026-09-08' });
    const out = renderPrompt('A {event_type} at {event_time} on {date}: {event_json} / {state_json}', vars);
    expect(out).toContain('A RANGE_CLOSED at 2026-09-08T14:07:00Z on 2026-09-08');
    expect(out).toContain('"type":"RANGE_CLOSED"');
    expect(out).toContain('"phase":"range_closed"');
  });

  it('leaves an unknown placeholder visible rather than silently blank', () => {
    expect(renderPrompt('hello {nobody}', { date: 'x' })).toBe('hello {nobody}');
  });
});

// ---------- persisted day state ----------

describe('panel day state', () => {
  it('merges events by seq without duplicating or reordering', () => {
    const merged = mergeEvents([ev(1, 'A'), ev(2, 'B')], [ev(2, 'B'), ev(3, 'C')]);
    expect(merged.map((e) => e.seq)).toEqual([1, 2, 3]);
  });

  it('caps the feed at the newest events', () => {
    const many = Array.from({ length: 300 }, (_, i) => ev(i + 1, 'BREAK'));
    const merged = mergeEvents([], many, 200);
    expect(merged.length).toBe(200);
    expect(merged[0].seq).toBe(101);
  });

  it('starts a day empty', () => {
    const d = emptyPanelDay('ib-es', '2026-09-08');
    expect(d).toMatchObject({ workflowId: 'ib-es', date: '2026-09-08', runId: null, lastSeq: 0, events: [], asked: [], replay: null });
  });

  it('files a day under the local date', () => {
    expect(todayISO(new Date(2026, 8, 8, 23, 30))).toBe('2026-09-08');
    expect(todayISO(new Date(2026, 0, 1, 0, 1))).toBe('2026-01-01');
  });
});
