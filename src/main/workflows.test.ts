// The workflow files that actually ship must parse. A yaml typo that only
// shows up as a console warning at runtime, with the workflow silently absent
// from the list, is exactly the failure this catches at build time.
import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { parse as parseYaml } from 'yaml';
import { describeIssues, WorkflowFileSchema, type WorkflowFile } from '@shared/workflow-schema';

const ROOT = 'workflows';
const dirs: string[] = readdirSync(ROOT).filter((d: string) => statSync(join(ROOT, d)).isDirectory());

function load(dir: string): WorkflowFile {
  const r = WorkflowFileSchema.safeParse(parseYaml(readFileSync(join(ROOT, dir, 'workflow.yaml'), 'utf8')));
  if (!r.success) throw new Error(`${dir}: ${describeIssues(r.error)}`);
  return r.data;
}

describe('the shipped workflow files', () => {
  it('finds all four', () => {
    expect(dirs.sort()).toEqual(['ib-es', 'ib-fdax', 'strategy-lab', 'v1b3-fundingpips']);
  });

  it.each(dirs)('%s parses and its prompt block exists', (dir: string) => {
    const w = load(dir);
    expect(w.id).toBe(dir);
    expect(w.title.length).toBeGreaterThan(0);
    expect(readFileSync(join(ROOT, dir, w.promptBlock), 'utf8').trim().length).toBeGreaterThan(200);
  });

  it("ib-es declares a session watch on the owner's instrument", () => {
    const p = load('ib-es').panel;
    expect(p?.kind).toBe('session-watch');
    const s = p?.sentinel as Record<string, unknown>;
    expect(s.symbol).toBe('PEPPERSTONE:US500');
    expect(s.timeframe).toBe('M5');
    expect(s.provider).toBe('session');
    expect(s.session).toEqual({ start: '16:30', end: '23:00', tz: 'Europe/Kyiv' });
    // The initial balance is the first 30 minutes of the RTH session.
    expect(s.range).toEqual({ source: 'session_open', minutes: 30 });
    // The owner's own rule: two consecutive closes outside the range hold it.
    expect(s.confirm).toEqual({ mode: 'closes', closes: 2 });
  });

  it('ib-es spends a turn only on the four event types worth one', () => {
    const p = load('ib-es').panel;
    expect(p?.askModelOn).toEqual(['RANGE_CLOSED', 'BREAK_CONFIRMED', 'SWEEP', 'CLOCK']);
    expect(p?.prompts.event).toBeTruthy();
    expect(p?.prompts.verdict).toBeTruthy();
  });

  it('ib-es asks the level scan for its anchors, or the Daily open never exists', () => {
    const lv = load('ib-es').panel?.levels as Record<string, unknown>;
    expect(lv.include).toContain('anchors');
    expect(load('ib-es').panel?.levelNames).toContain('Daily open');
  });

  it("ib-es names its clocks in the owner's timezone", () => {
    const clocks = load('ib-es').panel?.clocks ?? [];
    expect(clocks.map((c) => c.name)).toEqual(['verdict', 'eth_scan', 'ib_close', 'entries_closed', 'flat']);
    for (const c of clocks) expect(c.tz).toBe('Europe/Kyiv');
  });

  it('ib-fdax watches the Xetra session on the Eurex future itself', () => {
    const p = load('ib-fdax').panel;
    expect(p?.kind).toBe('session-watch');
    const s = p?.sentinel as Record<string, unknown>;
    expect(s.symbol).toBe('EUREX:FDAX1!');
    expect(s.provider).toBe('session');
    expect(s.session).toEqual({ start: '10:00', end: '18:30', tz: 'Europe/Kyiv' });
    // IB = 10:00-10:30, thirty minutes, never an hour.
    expect(s.range).toEqual({ source: 'session_open', minutes: 30 });
    // DAX canon: one full-bodied close beyond a narrow IB is the trigger.
    expect(s.confirm).toEqual({ mode: 'closes', closes: 1 });
    expect((s.watch as Record<string, unknown>).tick_size).toBe(1);
  });

  it('ib-fdax computes every level it hands the sentinel', () => {
    const p = load('ib-fdax').panel;
    const lv = p?.levels as Record<string, unknown>;
    // Anchors only come back when asked for in `include` (ib-es missed this once).
    expect(lv.include).toEqual(expect.arrayContaining(['prev_day', 'prev_session', 'sessions', 'anchors', 'adr']));
    expect(lv.prev_session_name).toBe('Xetra');
    expect(lv.adr_session).toBe('Xetra');
    expect(p?.levelNames).toEqual(
      expect.arrayContaining(['Xetra prev high', 'Xetra prev low', 'Premarket high', 'Premarket low']),
    );
  });

  it('ib-fdax has the pre-open plan turn and clocks a few minutes before each mark', () => {
    const p = load('ib-fdax').panel;
    expect(p?.prompts.plan).toContain('{levels_json}');
    expect(p?.askModelOn).toEqual(['RANGE_CLOSED', 'BREAK_CONFIRMED', 'SWEEP', 'CLOCK']);
    const clocks = p?.clocks ?? [];
    expect(clocks.map((c) => c.name)).toEqual([
      'premarket_scan',
      'ib_close',
      'late_entry',
      'entries_closed',
      'us_news_check',
      'us_open_check',
      'flat',
    ]);
    for (const c of clocks) expect(c.tz).toBe('Europe/Kyiv');
  });

  it('v1b3-fundingpips is a risk desk with no session watch at all', () => {
    const p = load('v1b3-fundingpips').panel;
    expect(p?.kind).toBe('risk-desk');
    expect(p?.sentinel).toBeUndefined();
    expect(p?.clocks).toEqual([]);
    // No watch means no events, so no automatic turns: the user asks.
    expect(p?.askModelOn).toEqual([]);
    expect(p?.prompts.verdict).toBeTruthy();
  });

  it('v1b3-fundingpips keeps the prop numbers in the yaml, where the owner can see them', () => {
    const risk = load('v1b3-fundingpips').panel?.risk as { account: Record<string, unknown> };
    expect(risk.account).toEqual({ starting_balance: 10000, currency: 'USD', max_loss_pct: 10, floor_type: 'static', risk_per_r: 50 });
  });

  it('never bakes a journal or a rules path into a workflow: those are settings', () => {
    for (const dir of dirs) {
      const p = load(dir).panel;
      if (!p) continue;
      const risk = (p.risk ?? {}) as Record<string, unknown>;
      const journal = (p.journal ?? {}) as Record<string, unknown>;
      expect(risk.journal).toBeUndefined();
      expect(risk.rules_path).toBeUndefined();
      expect(journal.path_or_name).toBeUndefined();
    }
  });

  it('strategy-lab still loads without a panel', () => {
    expect(load('strategy-lab').panel).toBeUndefined();
  });
});

describe('a broken workflow file', () => {
  const base = { id: 'x', title: 'X', promptBlock: 'prompt.md' };

  it('is rejected, with the path of the problem named', () => {
    const r = WorkflowFileSchema.safeParse({ ...base, panel: { kind: 'session-watch', sentinel: {} } });
    expect(r.success).toBe(false);
    expect(r.success ? '' : describeIssues(r.error)).toContain('sentinel.symbol');
  });

  it('is rejected for an id that is not a directory-safe slug', () => {
    expect(WorkflowFileSchema.safeParse({ ...base, id: '../escape' }).success).toBe(false);
  });

  it('loads fine with no panel section', () => {
    const r = WorkflowFileSchema.safeParse(base);
    expect(r.success).toBe(true);
    expect(r.success && r.data.panel).toBeUndefined();
  });
});
