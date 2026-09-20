// tv-cli: the allowlist guard, the never-throw contract, and one real
// end-to-end run against the tvmcp CLI.
//
// The live test needs the account-cookie feed (TV_SESSIONID) because the
// owner's instrument, PEPPERSTONE:US500, is a broker CFD that exists on no free
// feed. It skips itself when the cookie is absent, so CI stays green.
import { afterAll, describe, expect, it } from 'vitest';
import { rmSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import type { Settings } from '@shared/api';
import { DEFAULT_SETTINGS, DEFAULT_TVMCP_PATH } from '@shared/defaults';
import { callTool } from './tv-cli';

const cfg: Settings = { ...DEFAULT_SETTINGS, tvmcpPath: process.env.TVMCP_PATH || DEFAULT_TVMCP_PATH };

describe('the allowlist guard', () => {
  it('refuses an unlisted tool without ever spawning anything', async () => {
    const started = Date.now();
    const r = await callTool(cfg, 'tv_desktop_draw', { shape: 'rectangle' });
    expect(r.ok).toBe(false);
    expect(r.ok === false && r.error).toContain('not in the allowlist');
    // No process was started: a refusal is instant, not a 2.5 s interpreter boot.
    expect(Date.now() - started).toBeLessThan(500);
  });

  it('refuses junk names the same way', async () => {
    for (const name of ['', 'rm -rf /', 'tv_scan_levels; echo hi', 'TV_SCAN_LEVELS']) {
      const r = await callTool(cfg, name, {});
      expect(r.ok).toBe(false);
      expect(r.ok === false && r.error).toContain('not in the allowlist');
    }
  });

  it('refuses a listed tool when the tvmcp path is not configured, rather than spawning in the wrong place', async () => {
    const r = await callTool({ ...cfg, tvmcpPath: '   ' }, 'tv_sentinel_status', {});
    expect(r.ok).toBe(false);
    expect(r.ok === false && r.error).toContain('tvmcp path');
  });

  it('never throws, whatever the tool does', async () => {
    // A bad directory makes `uv` fail; the caller still gets a value, not an exception.
    const r = await callTool({ ...cfg, tvmcpPath: join(homedir(), 'no-such-tvmcp-directory') }, 'tv_sentinel_status', {}, { timeoutMs: 20_000 });
    expect(r.ok).toBe(false);
    expect(typeof (r.ok === false ? r.error : '')).toBe('string');
  });
});

// ---------- the live end-to-end check ----------

const live = process.env.TV_SESSIONID ? describe : describe.skip;
const RUN_ID = `desk-test-${Date.now().toString(36)}`;
const runFile = join(process.env.TV_SENTINEL_DIR || join(homedir(), '.tvmcp', 'sentinel'), `${RUN_ID}.json`);

afterAll(() => {
  rmSync(runFile, { force: true });
});

live('a real replay through the CLI path (needs TV_SESSIONID)', () => {
  it(
    'reproduces the owner\'s 2026-09-08 initial balance: high 7712.6 / low 7683.4',
    async () => {
      const spec = {
        symbol: 'PEPPERSTONE:US500',
        timeframe: 'M5',
        provider: 'session',
        session: { start: '16:30', end: '23:00', tz: 'Europe/Kyiv' },
        range: { source: 'session_open', minutes: 30 },
        watch: { breaks: true, break_range: true, sweep: true, levels: true, tick_size: 0.1 },
        confirm: { mode: 'closes', closes: 2 },
        day: '2026-09-08',
      };
      const started = await callTool(cfg, 'tv_sentinel_start', {
        spec,
        run_id: RUN_ID,
        replay: { from: '2026-09-08T13:00:00Z', to: '2026-09-08T16:00:00Z' },
      });
      expect(started.ok, started.ok === false ? started.error : '').toBe(true);
      expect((started as { data: { run_id: string } }).data.run_id).toBe(RUN_ID);

      const polled = await callTool(cfg, 'tv_sentinel_poll', { run_id: RUN_ID, since_seq: 0, max_events: 100 });
      expect(polled.ok, polled.ok === false ? polled.error : '').toBe(true);
      const data = (polled as { data: { events: { type: string; payload: Record<string, number> }[]; state: { status: string } } }).data;

      const closed = data.events.find((e) => e.type === 'RANGE_CLOSED');
      expect(closed, `no RANGE_CLOSED in ${data.events.map((e) => e.type).join(', ')}`).toBeTruthy();
      expect(closed!.payload.high).toBe(7712.6);
      expect(closed!.payload.low).toBe(7683.4);

      // The owner's own confirmation rule fires on the same day.
      expect(data.events.some((e) => e.type === 'BREAK_CONFIRMED')).toBe(true);
      // A replay completes in one poll, so the scheduler must stop here.
      expect(data.state.status).toBe('finished');
    },
    300_000,
  );
});
