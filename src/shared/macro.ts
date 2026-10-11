// The macro-review contract: what the desk reads from the owner's vault and the
// pure helpers that turn it into a picture.
//
// The data is produced by the `macro-review` skill in the vault
// (`indices-kb/tools/macro_review.py`, `scale.py`, note 22). The desk never
// computes a factor itself and never edits these files — it reads them, watches
// them, and asks the skill (through a chat turn) when something must change.
// The field names below are the skill's; `state/scale_<D>_<region>.json` is
// declared a data contract there («поля не переименовывать»).

export type Region = 'eu' | 'us';
export type Side = 'LONG' | 'SHORT';
/** The three daily review windows, Kyiv time. */
export type MacroWindowId = 'eu' | 'us' | 'review';
export const MACRO_WINDOWS: MacroWindowId[] = ['eu', 'us', 'review'];

/** Kyiv clock of each window and the cash open it prepares for. */
export const WINDOW_TIMES: Record<MacroWindowId, { at: string; open: string | null; until: string }> = {
  eu: { at: '09:30', open: '10:00', until: '12:00' },
  us: { at: '16:00', open: '16:30', until: '18:30' },
  review: { at: '20:00', open: null, until: '21:30' },
};

/** Paths inside the vault. Kept here so tests and the main process agree. */
export const VAULT_PATHS = {
  state: ['indices-kb', 'state'],
  journal: ['indices-kb', 'journal'],
  config: ['indices-kb', 'config', 'scale.json'],
  notes: ['Макро-обзоры'],
  /** Chart screenshots the skill takes of the review's levels (PNG only). */
  shots: ['indices-kb', 'shots'],
} as const;

export interface ScaleFactor {
  id: string;
  name: string;
  group: string;
  share_cfg?: number;
  on?: boolean;
  side: Side | '' | null;
  strength: number;
  why?: string;
  share: number;
  points: number;
}

export interface GroupResult {
  share: number;
  long: number;
  short: number;
  needle: number;
}

export interface ScaleResult {
  long: number;
  short: number;
  undecided: number;
  max?: number;
  needle: number;
  zone: string;
  side: Side | null;
  groups?: Record<string, GroupResult>;
}

export interface ScaleLight {
  color: 'green' | 'yellow' | 'red' | string;
  label: string;
  red?: string[];
  yellow?: string[];
  risk_pct?: number;
}

export interface PlanLevel {
  price?: number | null;
  name: string;
  adr?: number | null;
}

export interface PlanSetup {
  setup: string;
  why?: string;
  entry?: string;
  stop?: string;
  target?: string;
  until?: string;
}

export interface ScalePlan {
  side?: Side | null;
  price?: number;
  adr?: number;
  ib_typical_adr?: number;
  ib_typical_class?: string;
  risk_pct?: number;
  session?: Record<string, string>;
  manipulation_levels?: PlanLevel[];
  targets?: PlanLevel[];
  setups?: PlanSetup[];
  cancel?: string;
  summary?: string;
}

export interface DiscussionItem {
  at?: string;
  by: 'owner' | 'agent' | string;
  side: Side;
  share: number;
  text: string;
  reply?: string;
  agent_view?: 'agree' | 'partial' | 'disagree' | '' | string;
}

export interface ScaleState {
  region: Region;
  date: string;
  asof: string;
  sym: string;
  pair?: string;
  version?: string;
  factors: ScaleFactor[];
  result: ScaleResult;
  /** Needle after the discussion: base factors + every argument. Absent = no discussion. */
  after?: ScaleResult | null;
  light: ScaleLight;
  levels?: Record<string, number | null>;
  plan?: ScalePlan;
  discussion?: DiscussionItem[];
  history?: { asof: string; needle: number; after: number | null }[];
}

export interface DayShape {
  forecast?: string;
  prob_two_sided?: number;
  summary?: string;
  windows?: { window: string; expect: string }[];
}

export interface MacroVerdict {
  verdict: 'LONG' | 'SHORT' | 'NEUTRAL' | 'NO_TRADE' | string;
  confidence?: 'low' | 'medium' | 'high' | string;
  summary?: string;
  reasons?: string[];
  risks?: string[];
  watch?: string[];
  day_shape?: DayShape | null;
  sources?: { title: string; url?: string; source?: string }[];
  plan?: string[];
  scale_judgment?: { side: Side; strength?: number; text: string }[];
}

export interface CalendarEvent {
  title: string;
  impact?: string;
  time_eet?: string;
  currency?: string;
  forecast?: number | string | null;
  previous?: number | string | null;
}

/** The part of `macro_<D>_<region>.json` the desk shows (headlines are trimmed to their summary). */
export interface MacroCollect {
  asof_eet?: string;
  open_eet?: string;
  calendar?: { status?: string; reasons?: string[]; flags?: string[]; events?: CalendarEvent[] } | null;
  markets?: { label: string; chg_pct?: number | null; main?: boolean; kind?: string }[];
  headlines?: { count?: number; relevant?: number; tone_sum?: number; tags?: Record<string, number> } | null;
  tilt?: { score?: number; hint?: string } | null;
  regime?: unknown;
  prev_day_check?: unknown[];
  previous_verdict?: MacroVerdict | null;
}

/** What a line on a chart screenshot stands for. */
export type ShotKind = 'manip' | 'target' | 'level' | 'open';

/** One line drawn on a screenshot, as the legend under the picture names it. */
export interface ShotLegend {
  key: string;
  /** Plain words, written by the skill («манипуляция: открытие дня»). */
  label: string;
  kind: ShotKind | string;
  color: string;
}

export interface MacroShot {
  name: string;
  /** Timeframe as TradingView names it: M15, H1. */
  tf: string;
  title: string;
  /** Vault-relative path of the PNG (forward slashes); the file may be missing. */
  path: string;
  /** False when the skill's drawing check found a mismatch; absent = not checked. */
  verify_ok?: boolean | null;
  legend: ShotLegend[];
}

/**
 * `state/shots_<D>_<region>.json`: the review's levels drawn on TradingView
 * and screenshotted. `ok: false` means no pictures, and `error` says why
 * («dry-run», «TradingView не запущен», «нет шкалы с планом: …»).
 */
export interface MacroShots {
  date: string;
  region: Region;
  asof: string;
  ok: boolean;
  error: string | null;
  shots: MacroShot[];
}

export interface RegionSnapshot {
  region: Region;
  scale: ScaleState | null;
  verdict: MacroVerdict | null;
  collect: MacroCollect | null;
  /** Vault-relative path of the day's note, when it exists. */
  note: string | null;
  /** The chart screenshots of this window; null when the skill wrote none. */
  shots: MacroShots | null;
}

export interface ReviewFactorRow {
  factor: string;
  group?: string;
  share?: number | string;
  side?: string;
  strength?: number | string;
  dir?: string;
  hit?: '1' | '0' | '' | string;
}

export interface ReviewRegion {
  missing?: string;
  facts?: Record<string, unknown> & { dir?: string; chg_adr?: number; first_break?: string | null; ib_adr?: number; partial?: boolean };
  score?: Record<string, unknown> & { needle_hit?: string; after_hit?: string; plan_hit?: string | null; manip_swept?: boolean | null };
  factors?: ReviewFactorRow[];
  discussion?: DiscussionItem[];
  needle?: number;
  zone?: string;
  after?: number | null;
  light?: ScaleLight;
  macro_verdict?: Partial<MacroVerdict>;
}

export interface ReviewMech {
  date: string;
  asof?: string;
  regions: Partial<Record<Region, ReviewRegion>>;
}

export interface ReviewAgent {
  summary?: string;
  regions?: Partial<Record<Region, { what_worked?: string; what_failed?: string; discussion?: string }>>;
  lessons?: string[];
  weight_ideas?: string[];
}

export interface FactorStat {
  region: Region;
  factor: string;
  hits: number;
  total: number;
}

export interface MacroDay {
  date: string;
  eu: RegionSnapshot;
  us: RegionSnapshot;
  review: { mech: ReviewMech | null; agent: ReviewAgent | null; note: string | null };
  /** Newest lessons first, at most LESSONS_TAIL. */
  lessons: string[];
  /** Hit rate per factor over the last 30 days before (and including) this date. */
  stats: FactorStat[];
  /** Factor id → readable name, from config/scale.json. */
  names: Record<string, string>;
  zones: { neutral: number; strong: number };
}

export interface MacroIndex {
  /** Vault path the desk is reading; empty when not configured. */
  vault: string;
  ok: boolean;
  error?: string;
  /** Dates that have any macro file, newest first. */
  days: string[];
  /**
   * Reading the demo vault (`DESK_DEMO_VAULT`, see scripts/make-demo-vault.py):
   * invented days for checking the interface. The desk must not send anything
   * to the agent then — a run would land in the real vault.
   */
  demo?: boolean;
}

export const LESSONS_TAIL = 15;
export const DEFAULT_ZONES = { neutral: 15, strong: 40 };

// ---------------------------------------------------------------- pure helpers

export type ZoneKey = 'strong-short' | 'short' | 'neutral' | 'long' | 'strong-long';
export const ZONE_KEYS: ZoneKey[] = ['strong-short', 'short', 'neutral', 'long', 'strong-long'];

/** Same thresholds as scale.py: |needle| < neutral → neutral, ≥ strong → strong. */
export function zoneOf(needle: number, zones = DEFAULT_ZONES): ZoneKey {
  const a = Math.abs(needle);
  if (a < zones.neutral) return 'neutral';
  if (needle > 0) return a >= zones.strong ? 'strong-long' : 'long';
  return a >= zones.strong ? 'strong-short' : 'short';
}

export function clampNeedle(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return Math.max(-100, Math.min(100, n));
}

/**
 * Gauge geometry: −100 (strong short) sits at the left end of the arc (180°),
 * +100 at the right end (0°). Angles are mathematical (counter-clockwise from +x).
 */
export function needleAngle(needle: number): number {
  return 90 - clampNeedle(needle) * 0.9;
}

/** Point on a circle centred at (cx, cy); y grows downwards as in SVG. */
export function polar(cx: number, cy: number, r: number, angleDeg: number): [number, number] {
  const a = (angleDeg * Math.PI) / 180;
  return [cx + r * Math.cos(a), cy - r * Math.sin(a)];
}

/** SVG path of an annular arc between two needle values (from < to). */
export function arcPath(cx: number, cy: number, r0: number, r1: number, from: number, to: number): string {
  const a0 = needleAngle(from);
  const a1 = needleAngle(to);
  const [x0, y0] = polar(cx, cy, r1, a0);
  const [x1, y1] = polar(cx, cy, r1, a1);
  const [x2, y2] = polar(cx, cy, r0, a1);
  const [x3, y3] = polar(cx, cy, r0, a0);
  const f = (n: number) => n.toFixed(2);
  return `M${f(x0)} ${f(y0)} A${r1} ${r1} 0 0 1 ${f(x1)} ${f(y1)} L${f(x2)} ${f(y2)} A${r0} ${r0} 0 0 0 ${f(x3)} ${f(y3)} Z`;
}

/** The five segments of the gauge as needle ranges. */
export function zoneSegments(zones = DEFAULT_ZONES): { key: ZoneKey; from: number; to: number }[] {
  return [
    { key: 'strong-short', from: -100, to: -zones.strong },
    { key: 'short', from: -zones.strong, to: -zones.neutral },
    { key: 'neutral', from: -zones.neutral, to: zones.neutral },
    { key: 'long', from: zones.neutral, to: zones.strong },
    { key: 'strong-long', from: zones.strong, to: 100 },
  ];
}

/** Factors split the way the report reads: for long, for short, silent — heaviest first. */
export function splitFactors(factors: ScaleFactor[]): { long: ScaleFactor[]; short: ScaleFactor[]; silent: ScaleFactor[] } {
  const by = (a: ScaleFactor, b: ScaleFactor) => b.points - a.points || b.share - a.share;
  const on = factors.filter((f) => f.on !== false);
  return {
    long: on.filter((f) => f.side === 'LONG' && f.points > 0).sort(by),
    short: on.filter((f) => f.side === 'SHORT' && f.points > 0).sort(by),
    silent: on.filter((f) => !f.side || f.points <= 0).sort((a, b) => b.share - a.share),
  };
}

/** Kyiv wall clock as {date: 'YYYY-MM-DD', minutes since midnight}. */
export function kyivNow(now: Date = new Date()): { date: string; minutes: number } {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone: 'Europe/Kyiv',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  }).formatToParts(now);
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? '00';
  return { date: `${get('year')}-${get('month')}-${get('day')}`, minutes: Number(get('hour')) * 60 + Number(get('minute')) };
}

const toMinutes = (hhmm: string) => Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3, 5));

export type WindowState = 'ready' | 'partial' | 'waiting' | 'due' | 'missed';

/**
 * Where a window stands for a given day: data present (`ready`; `partial` when
 * the mechanics ran but the agent has not written its part yet), not yet time
 * (`waiting`), its time has come and nothing is there (`due`), or the day is
 * over without it (`missed`).
 */
export function windowState(id: MacroWindowId, day: MacroDay | null, date: string, now = kyivNow()): WindowState {
  if (day) {
    if (id === 'review') {
      if (day.review.mech && day.review.agent) return 'ready';
      if (day.review.mech || day.review.agent) return 'partial';
    } else {
      const r = day[id];
      if (r.scale && r.verdict) return 'ready';
      if (r.scale || r.verdict) return 'partial';
    }
  }
  if (date > now.date) return 'waiting';
  if (date < now.date) return 'missed';
  const w = WINDOW_TIMES[id];
  if (now.minutes < toMinutes(w.at) - 5) return 'waiting';
  if (now.minutes <= toMinutes(w.until)) return 'due';
  return 'missed';
}

/** The window worth opening first for a day: the latest one that has something, else the current one. */
export function defaultWindow(day: MacroDay | null, date: string, now = kyivNow()): MacroWindowId {
  if (day && date === now.date) {
    if (now.minutes >= toMinutes('19:30') && (day.review.mech || day.review.agent)) return 'review';
    if (now.minutes >= toMinutes('13:00') && (day.us.scale || day.us.verdict)) return 'us';
    if (now.minutes >= toMinutes('19:30')) return 'review';
    if (now.minutes >= toMinutes('13:00')) return 'us';
    return 'eu';
  }
  if (day?.review.mech || day?.review.agent) return 'review';
  if (day?.us.scale || day?.us.verdict) return 'us';
  return 'eu';
}

/** Minimal RFC-4180 CSV reader (quotes, commas and newlines inside quotes). */
export function parseCsv(text: string): Record<string, string>[] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  const src = text.replace(/^﻿/, '');
  for (let i = 0; i < src.length; i++) {
    const c = src[i];
    if (quoted) {
      if (c === '"' && src[i + 1] === '"') {
        cell += '"';
        i++;
      } else if (c === '"') quoted = false;
      else cell += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') {
      row.push(cell);
      cell = '';
    } else if (c === '\n' || c === '\r') {
      if (c === '\r' && src[i + 1] === '\n') i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
    } else cell += c;
  }
  if (cell || row.length) {
    row.push(cell);
    rows.push(row);
  }
  const [head, ...body] = rows.filter((r) => r.length > 1 || r[0] !== '');
  if (!head) return [];
  return body.map((r) => Object.fromEntries(head.map((h, i) => [h, r[i] ?? ''])));
}

/** Days between two ISO dates (b − a). */
function dayDiff(a: string, b: string): number {
  return Math.round((Date.parse(`${b}T00:00:00Z`) - Date.parse(`${a}T00:00:00Z`)) / 86_400_000);
}

/**
 * Hit rate per factor from `journal/scale_factor_scores.csv` over `days`
 * calendar days ending at `date`. Rows without a verdict (`hit` empty: the
 * factor was silent, or the window stayed flat) do not count — the same rule
 * as `scale.py stats`.
 */
export function factorStats(rows: Record<string, string>[], date: string, days = 30): FactorStat[] {
  const acc = new Map<string, FactorStat>();
  for (const r of rows) {
    if (r.hit !== '1' && r.hit !== '0') continue;
    if (r.region !== 'eu' && r.region !== 'us') continue;
    const d = dayDiff(r.date, date);
    if (d < 0 || d >= days) continue;
    const key = `${r.region}:${r.factor}`;
    const s = acc.get(key) ?? { region: r.region, factor: r.factor, hits: 0, total: 0 };
    s.total += 1;
    if (r.hit === '1') s.hits += 1;
    acc.set(key, s);
  }
  return [...acc.values()].sort((a, b) => b.total - a.total || a.factor.localeCompare(b.factor));
}

/** Bullet lines of `journal/lessons.md`, newest last in the file → newest first here. */
export function lessonsTail(md: string, n = LESSONS_TAIL): string[] {
  return md
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => /^[-*]\s+/.test(l))
    .map((l) => l.replace(/^[-*]\s+/, ''))
    .slice(-n)
    .reverse();
}

const num = (x: unknown): number => (typeof x === 'number' && Number.isFinite(x) ? x : 0);

/** A number for the screen: never throws on a missing field. */
export function fmt(x: unknown, digits = 1): string {
  return typeof x === 'number' && Number.isFinite(x) ? x.toFixed(digits) : '—';
}

/**
 * Bring a scale written by an older `scale.py` to the v3 shape the desk draws.
 *
 * v1 (before 10.10.2026) counted weights 1–3 and points out of `max`, with no
 * `share` per factor, no `undecided` and no group shares. The needle itself is
 * kept as written; only the missing percentages are derived, the way v3 would:
 * share = weight / Σweights, points = share × strength, long % = long / max.
 * A v3 file passes through untouched.
 */
export function normalizeScale(s: ScaleState): ScaleState {
  const factors = Array.isArray(s.factors) ? s.factors : [];
  const legacy = factors.some((f) => typeof f.share !== 'number');
  const fixResult = (r: ScaleResult | null | undefined, scale: number): ScaleResult | null | undefined => {
    if (!r) return r;
    if (typeof r.undecided === 'number' && typeof r.long === 'number' && typeof r.short === 'number' && !legacy) return r;
    const long = Math.round(num(r.long) * scale * 10) / 10;
    const short = Math.round(num(r.short) * scale * 10) / 10;
    const groups = r.groups
      ? Object.fromEntries(
          Object.entries(r.groups).map(([k, g]) => {
            const share = factors.filter((f) => f.group === k && f.on !== false).reduce((n, f) => n + num((f as { weight?: number }).weight), 0) * scale;
            return [k, { share: typeof g.share === 'number' ? g.share : Math.round(share * 10) / 10, long: Math.round(num(g.long) * scale * 10) / 10, short: Math.round(num(g.short) * scale * 10) / 10, needle: num(g.needle) }];
          }),
        )
      : undefined;
    return { ...r, long, short, undecided: Math.max(0, Math.round((100 - long - short) * 10) / 10), needle: num(r.needle), groups };
  };
  if (!legacy) {
    return { ...s, factors, result: fixResult(s.result, 1)!, after: fixResult(s.after, 1) ?? s.after };
  }
  const total = factors.filter((f) => f.on !== false).reduce((n, f) => n + num((f as { weight?: number }).weight), 0) || num(s.result?.max) || 1;
  const scale = 100 / total;
  const fixed = factors.map((f) => {
    const share = Math.round(num((f as { weight?: number }).weight) * scale * 10) / 10;
    return { ...f, share, strength: num(f.strength), points: f.side ? Math.round(share * num(f.strength) * 10) / 10 : 0 };
  });
  const rScale = 100 / (num(s.result?.max) || total);
  return { ...s, factors: fixed, result: fixResult(s.result, rScale)!, after: fixResult(s.after, rScale) ?? s.after };
}

/** Which state files belong to a date: `scale_2026-10-09_us.json` → 2026-10-09. */
export function dateOfStateFile(name: string): string | null {
  const m = name.match(/^(?:scale|macro|review|regime|shots)_(\d{4}-\d{2}-\d{2})(?:_|\.)/);
  return m ? m[1] : null;
}

// ---------------------------------------------------------------- chart screenshots

const str = (x: unknown): string => (typeof x === 'string' ? x : x == null ? '' : String(x));

/** Line colour per kind when the file's own colour is missing or not a plain hex. */
export const SHOT_KIND_COLORS: Record<ShotKind, string> = { manip: '#d32f2f', target: '#1e8e3e', level: '#37474f', open: '#9e9e9e' };

/**
 * A colour safe to put in a style: `#rgb`, `#rrggbb` or `#rrggbbaa` from the
 * file, else the kind's default. The file is the skill's, but a style
 * attribute is no place for whatever string it happens to hold.
 */
export function legendColor(item: Pick<ShotLegend, 'color' | 'kind'>): string {
  const c = str(item.color).trim();
  if (/^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(c)) return c;
  return SHOT_KIND_COLORS[item.kind as ShotKind] ?? SHOT_KIND_COLORS.level;
}

/**
 * Bring a parsed `shots_<D>_<region>.json` to the shape the desk draws, or
 * null when it is not one at all (garbage, an array, a half-written file).
 * Shots without a path are dropped; a missing legend is an empty one.
 */
export function normalizeShots(raw: unknown, region: Region): MacroShots | null {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.ok !== 'boolean' && !Array.isArray(r.shots)) return null;
  const shots: MacroShot[] = (Array.isArray(r.shots) ? r.shots : [])
    .filter((x): x is Record<string, unknown> => Boolean(x) && typeof x === 'object' && typeof (x as Record<string, unknown>).path === 'string')
    .map((x) => ({
      name: str(x.name),
      tf: str(x.tf),
      title: str(x.title),
      path: str(x.path),
      verify_ok: typeof x.verify_ok === 'boolean' ? x.verify_ok : null,
      legend: (Array.isArray(x.legend) ? x.legend : [])
        .filter((l): l is Record<string, unknown> => Boolean(l) && typeof l === 'object' && typeof (l as Record<string, unknown>).label === 'string')
        .map((l) => ({ key: str(l.key), label: str(l.label), kind: str(l.kind) || 'level', color: str(l.color) })),
    }));
  return {
    date: str(r.date),
    region: r.region === 'eu' || r.region === 'us' ? r.region : region,
    asof: str(r.asof),
    // A file that lists pictures but forgot `ok` is taken at its pictures.
    ok: typeof r.ok === 'boolean' ? r.ok : shots.length > 0,
    error: r.error == null || r.error === '' ? null : str(r.error),
    shots,
  };
}

/** The picture for the main view: the M15 one when there is one, else the first. */
export function mainShot(shots: MacroShot[]): MacroShot | null {
  return shots.find((s) => s.name.toLowerCase() === 'm15' || s.tf.toUpperCase() === 'M15') ?? shots[0] ?? null;
}

/** «M15 · ночь и уровни у открытия» from tf «M15» and title «M15 — ночь и уровни у открытия». */
export function shotTabLabel(shot: Pick<MacroShot, 'tf' | 'title' | 'name'>): string {
  const tf = shot.tf.trim() || shot.name.trim().toUpperCase();
  let rest = shot.title.trim();
  if (tf && rest.toUpperCase().startsWith(tf.toUpperCase())) rest = rest.slice(tf.length).replace(/^[\s—–\-:·]+/, '');
  return rest ? (tf ? `${tf} · ${rest}` : rest) : tf;
}

/**
 * Why there are no pictures, ready for «Снимков графика нет: …»: `dry-run` is
 * a known reason the screen words itself; anything else is shown as written,
 * on one line and cut short.
 */
export function shotsErrorText(error: string | null | undefined, max = 90): { known: 'dry-run' | 'none' | null; text: string } {
  const e = str(error).replace(/\s+/g, ' ').trim();
  if (!e) return { known: 'none', text: '' };
  if (/^dry[-_ ]?run\b/i.test(e)) return { known: 'dry-run', text: '' };
  return { known: null, text: e.length > max ? `${e.slice(0, max - 1).trimEnd()}…` : e };
}

/** A vault-relative screenshot path the renderer may ask for: `.png` under `indices-kb/shots/`, no `..`. */
export function isShotPath(rel: unknown): rel is string {
  if (typeof rel !== 'string' || !rel || rel.length > 400) return false;
  const p = rel.replace(/\\/g, '/');
  if (p.startsWith('/') || /^[a-z]:/i.test(p) || p.includes('\0')) return false;
  const parts = p.split('/');
  if (parts.some((x) => x === '..' || x === '.' || x === '')) return false;
  return parts.length > 3 && parts[0] === VAULT_PATHS.shots[0] && parts[1] === VAULT_PATHS.shots[1] && /\.png$/i.test(p);
}

// ---------------------------------------------------------------- what the desk sends to the agent

/** The slash command the skill answers to, per window. */
export function runCommand(id: MacroWindowId): string {
  return `/macro-review ${id}`;
}

/**
 * An argument from the owner, phrased so the skill's «Обсуждение после отчёта»
 * path picks it up: side and share are explicit, so the agent does not have to
 * ask, and it still answers with its own position before recording it.
 */
export function discussPrompt(region: Region, side: Side, share: number, text: string): string {
  const where = region === 'eu' ? 'Европа' : 'США';
  const s = Math.max(1, Math.min(50, Math.round(share)));
  return [
    `Довод по шкале (${where}, регион ${region}): ${text.trim()}`,
    `Сторона: ${side === 'LONG' ? 'лонг' : 'шорт'} (${side}). Доля: ${s} %.`,
    'Ответь своей позицией (согласен / частично / не согласен) с причиной из данных, запиши довод через scale.py discuss и скажи, как изменились стрелка и план.',
  ].join('\n');
}

/** Title of the chat that carries one day of reviews. */
export function macroChatTitle(date: string): string {
  return `Обзор дня ${date.slice(8, 10)}.${date.slice(5, 7)}`;
}
