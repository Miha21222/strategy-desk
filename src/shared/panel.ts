// The `panel` contract: what a workflow declares so the app can watch a
// trading day on its own, and the pure rules that drive that watch.
//
// Everything here is deterministic and free of DOM / Electron / node, so the
// main process, the renderer and vitest all import the same code.
//
// Design rule: the app spends model tokens only where a judgment is wanted.
// Levels, calendar, risk, journal and every sentinel poll go through the tvmcp
// CLI (`uv --directory <tvmcp> run tv <tool> '<json>'`) and cost nothing but a
// couple of seconds of local CPU. A chat turn is sent ONLY when an event whose
// type is listed in `panel.askModelOn` arrives, or when the user asks.
import { z } from 'zod';

// ---------- the tool allowlist ----------

/**
 * The only tools the renderer may run through `tv:call`. Read-and-poll only:
 * nothing here draws on the user's chart, writes a journal row or touches a
 * Pine script. `tv_sentinel_start/poll/stop` write a run file under
 * TV_SENTINEL_DIR and nothing else.
 */
export const TV_TOOL_ALLOWLIST = [
  'tv_sentinel_start',
  'tv_sentinel_poll',
  'tv_sentinel_status',
  'tv_sentinel_stop',
  'tv_scan_levels',
  'tv_calendar_check',
  'tv_risk_guard',
  'tv_journal_load',
  'tv_desktop_status',
] as const;

export type TvToolName = (typeof TV_TOOL_ALLOWLIST)[number];

const ALLOWED = new Set<string>(TV_TOOL_ALLOWLIST);

export function isAllowedTvTool(name: unknown): name is TvToolName {
  return typeof name === 'string' && ALLOWED.has(name);
}

/** Result of one CLI call. `callTool` never throws into the renderer. */
export type TvCallResult =
  | { ok: true; data: unknown; warnings: string[]; ms: number }
  | { ok: false; error: string; warnings: string[]; ms: number };

// ---------- the panel schema ----------

/**
 * The sentinel / levels / calendar / risk / journal sections are passed to the
 * tool almost verbatim, so they stay permissive records: a second workflow can
 * declare another instrument, session or rules file with no code change. The
 * tool itself validates them (pydantic, `extra="forbid"`), and a bad section
 * surfaces as a call error in the panel rather than a crash at load time.
 */
const args = () => z.record(z.string(), z.unknown());

export const PANEL_KINDS = ['session-watch', 'risk-desk'] as const;
export type PanelKind = (typeof PANEL_KINDS)[number];

export const ClockSchema = z.object({
  name: z.string().min(1).max(40),
  at: z.string().regex(/^\d{1,2}:\d{2}$/),
  tz: z.string().max(60).default('UTC'),
});

export const PanelPromptsSchema = z
  .object({
    /** Sent when an event listed in askModelOn arrives. */
    event: z.string().max(4000).optional(),
    /** Sent by the «спросить вердикт» button. */
    verdict: z.string().max(4000).optional(),
    /** Sent by the «разобрать день» button after a replay. */
    review: z.string().max(4000).optional(),
  })
  .default({});

export const PanelSchema = z
  .object({
    kind: z.enum(PANEL_KINDS),
    /** tv_sentinel_start spec, minus `day` and `levels` which the app fills. */
    sentinel: args().optional(),
    /** tv_scan_levels arguments; its result fills sentinel.levels. */
    levels: args().optional(),
    /** Subset of the computed level names to hand the sentinel; empty = all. */
    levelNames: z.array(z.string().min(1).max(60)).max(50).default([]),
    /** Clock marks merged into the sentinel spec. */
    clocks: z.array(ClockSchema).max(20).default([]),
    /** Event types worth one chat turn. Everything else the app renders alone. */
    askModelOn: z.array(z.string().min(1).max(40)).max(20).default([]),
    /** tv_calendar_check arguments (the app adds today's date). */
    calendar: args().optional(),
    /** tv_risk_guard arguments (the app adds `today` and the settings paths). */
    risk: args().optional(),
    /** tv_journal_load arguments (the app adds the settings path). */
    journal: args().optional(),
    prompts: PanelPromptsSchema,
    /** Never poll faster than this, whatever the run asks for. */
    pollFloorSeconds: z.number().int().min(1).max(3600).default(5),
  })
  .superRefine((p, ctx) => {
    if (p.kind === 'session-watch') {
      const sym = p.sentinel && (p.sentinel as Record<string, unknown>).symbol;
      if (typeof sym !== 'string' || !sym.trim()) {
        ctx.addIssue({ code: 'custom', path: ['sentinel', 'symbol'], message: 'session-watch needs panel.sentinel.symbol' });
      }
    }
  });

export type PanelDef = z.infer<typeof PanelSchema>;

export const POLL_FLOOR_S = 5;
/** A poll that asks for longer than this is still checked once a quarter hour. */
export const POLL_CEILING_S = 900;

// ---------- sentinel spec assembly ----------

export interface SentinelLevel {
  name: string;
  price?: number;
  high?: number;
  low?: number;
}

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/**
 * Flatten a `tv_scan_levels` result into sentinel `levels[]`.
 * Prices become points, zones become high/low, anything else is dropped —
 * an invented level is worse than a missing one.
 */
export function levelsFromScan(scan: unknown, keep: string[] = [], max = 50): SentinelLevel[] {
  const rows = (scan as { levels?: unknown })?.levels;
  if (!Array.isArray(rows)) return [];
  const wanted = keep.length ? new Set(keep) : null;
  const out: SentinelLevel[] = [];
  const seen = new Set<string>();
  for (const raw of rows) {
    const r = raw as Record<string, unknown>;
    const name = typeof r.name === 'string' ? r.name.trim().slice(0, 60) : '';
    if (!name || seen.has(name)) continue;
    if (wanted && !wanted.has(name)) continue;
    if (finite(r.price)) out.push({ name, price: r.price });
    else if (finite(r.high) && finite(r.low)) out.push({ name, high: r.high, low: r.low });
    else continue;
    seen.add(name);
    if (out.length >= max) break;
  }
  return out;
}

/** ADR in points from a `tv_scan_levels` result, or null. */
export function adrFromScan(scan: unknown): number | null {
  const v = (scan as { adr?: { value?: unknown } })?.adr?.value;
  return finite(v) && v > 0 ? v : null;
}

export interface BuildSpecOptions {
  day: string;
  levels?: SentinelLevel[];
  /** Fills range.adr when the yaml left it out, so RANGE_CLOSED reports size_vs_adr. */
  adr?: number | null;
}

/**
 * The spec handed to `tv_sentinel_start`: the yaml's `sentinel` section plus
 * today's date, the levels just computed and the panel's clocks.
 *
 * `watch.break_levels` / `watch.sweep_levels` are filtered down to the names
 * that actually made it into `levels[]` — the tool refuses a spec that names a
 * level it cannot see, and a feed that returned one level fewer must not take
 * the whole day's watch down with it.
 */
export function buildSentinelSpec(panel: PanelDef, opts: BuildSpecOptions): Record<string, unknown> {
  const base = { ...((panel.sentinel ?? {}) as Record<string, unknown>) };
  const levels = opts.levels ?? [];
  const names = new Set(levels.map((l) => l.name));

  const watch = { ...((base.watch ?? {}) as Record<string, unknown>) };
  for (const key of ['break_levels', 'sweep_levels'] as const) {
    const listed = watch[key];
    if (Array.isArray(listed)) {
      const kept = listed.filter((n): n is string => typeof n === 'string' && names.has(n));
      if (kept.length) watch[key] = kept;
      else delete watch[key];
    }
  }

  const range = { ...((base.range ?? {}) as Record<string, unknown>) };
  if (range.adr === undefined && finite(opts.adr) && opts.adr > 0) range.adr = opts.adr;

  const spec: Record<string, unknown> = { ...base, day: opts.day };
  if (levels.length) spec.levels = levels;
  else delete spec.levels;
  if (Object.keys(watch).length) spec.watch = watch;
  if (Object.keys(range).length) spec.range = range;
  const clocks = panel.clocks.length ? panel.clocks : (base.clocks as unknown[] | undefined);
  if (Array.isArray(clocks) && clocks.length) spec.clocks = clocks;
  return spec;
}

// ---------- the poll scheduler ----------

/** The `state` object every sentinel tool returns (only the fields we read). */
export interface SentinelState {
  run_id?: string | null;
  status?: string;
  phase?: string;
  waiting_for?: string;
  schedule?: Record<string, unknown> | null;
  range?: Record<string, unknown> | null;
  last_seq?: number;
  bars_seen?: number;
  replay?: { from?: string; to?: string } | null;
  flags?: Record<string, unknown> | null;
}

export interface SentinelEvent {
  seq: number;
  ts_utc: string;
  type: string;
  payload?: Record<string, unknown>;
}

/**
 * Whether the app should keep polling this run.
 *
 * A stopped or finished run loads no bars, a closed session produces nothing
 * more, and a replay is complete after its single poll — polling any of them
 * again only burns 2.5 s of CPU per call for a byte-identical answer.
 */
export function shouldContinuePolling(state: SentinelState | null | undefined): boolean {
  if (!state) return false;
  const status = String(state.status ?? 'running');
  if (status === 'stopped' || status === 'finished') return false;
  const phase = String(state.phase ?? 'pending');
  if (phase === 'stopped' || phase === 'session_closed') return false;
  if (state.replay) return false;
  if (state.flags && state.flags.replay_done) return false;
  return true;
}

/**
 * Delay before the next poll, in milliseconds, or null when the run is done.
 * The run's own `next_poll_after_s` wins, clamped into [floor, ceiling].
 */
export function nextPollDelayMs(
  poll: { state?: SentinelState | null; next_poll_after_s?: unknown } | null | undefined,
  floorSeconds = POLL_FLOOR_S,
): number | null {
  if (!poll || !shouldContinuePolling(poll.state)) return null;
  const floor = Math.max(1, Math.floor(floorSeconds));
  const asked = poll.next_poll_after_s;
  const seconds = finite(asked) && asked > 0 ? asked : floor;
  return Math.round(Math.min(POLL_CEILING_S, Math.max(floor, seconds)) * 1000);
}

// ---------- the event -> model-turn rule ----------

/** True only for an event type the workflow listed in `askModelOn`. */
export function shouldAskModel(panel: PanelDef | null | undefined, eventType: string): boolean {
  if (!panel || !panel.askModelOn.length) return false;
  return panel.askModelOn.includes(eventType);
}

/** The events of one poll that are worth a chat turn, oldest first. */
export function eventsNeedingModel(panel: PanelDef | null | undefined, events: SentinelEvent[]): SentinelEvent[] {
  if (!panel || !panel.askModelOn.length) return [];
  return events.filter((e) => e && typeof e.type === 'string' && shouldAskModel(panel, e.type)).sort((a, b) => a.seq - b.seq);
}

/**
 * Fill a prompt template from the yaml. Unknown placeholders are left alone so
 * a typo in the workflow file is visible in the chat instead of silent.
 */
export function renderPrompt(template: string, vars: Record<string, string>): string {
  let out = template;
  for (const [k, v] of Object.entries(vars)) out = out.split(`{${k}}`).join(v);
  return out;
}

/** The variables an event prompt may use. */
export function eventPromptVars(ev: SentinelEvent, state: SentinelState | null, extra: Record<string, string> = {}): Record<string, string> {
  return {
    event_type: ev.type,
    event_time: ev.ts_utc ?? '',
    event_json: JSON.stringify({ seq: ev.seq, ts_utc: ev.ts_utc, type: ev.type, payload: ev.payload ?? {} }),
    state_json: JSON.stringify(state ?? {}),
    ...extra,
  };
}

// ---------- persisted per-day panel state ----------

/** One workflow + one date: enough to reattach after an app restart. */
export interface PanelDayState {
  workflowId: string;
  date: string;
  runId: string | null;
  lastSeq: number;
  /** Events already rendered, newest last (capped). */
  events: SentinelEvent[];
  /** Event seqs a chat turn was already spent on, so a restart never re-asks. */
  asked: number[];
  replay: { from: string; to: string } | null;
  updatedAt: number;
}

export const MAX_PANEL_EVENTS = 200;
export const MAX_PANEL_DAYS = 30;

export function emptyPanelDay(workflowId: string, date: string): PanelDayState {
  return { workflowId, date, runId: null, lastSeq: 0, events: [], asked: [], replay: null, updatedAt: Date.now() };
}

/** Merge a poll's events into the stored day, keeping seq order and no duplicates. */
export function mergeEvents(stored: SentinelEvent[], incoming: SentinelEvent[], max = MAX_PANEL_EVENTS): SentinelEvent[] {
  const bySeq = new Map<number, SentinelEvent>();
  for (const e of stored) if (e && typeof e.seq === 'number') bySeq.set(e.seq, e);
  for (const e of incoming) if (e && typeof e.seq === 'number') bySeq.set(e.seq, e);
  return [...bySeq.values()].sort((a, b) => a.seq - b.seq).slice(-max);
}

/** The local date (YYYY-MM-DD) a session-watch day is filed under. */
export function todayISO(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
