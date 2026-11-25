// Golden cases for the pure helpers of src/shared/macro.ts (and the two file
// readers of src/main/macro-state.ts that have a pure answer), produced by
// RUNNING the TS functions. Rust's desk-core must give the same answers
// (TEAM.md rule 4) unless docs/rust/TZ.md changes the behaviour on purpose —
// the only such place here is windows.json, which carries both answers:
// `ts` (the old app, Kyiv clock fixed at 09:30 / 16:00 / 20:00) and `tz`
// (TZ №21: the windows follow the Xetra and New York opens).
//
//   node capture-helpers.cjs <golden_vault> <out_dir>
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import * as M from '../../../src/shared/macro';
import { listDays, loadDay, readShot } from '../../../src/main/macro-state';
import { canonLines, canonValue, shotRef } from './canon';

const vault = path.resolve(process.argv[2]);
const out = path.resolve(process.argv[3]);
mkdirSync(out, { recursive: true });
const put = (name: string, text: string) => writeFileSync(path.join(out, name), text);
const SOURCE = 'Produced by rust/fixtures/tools/capture-helpers.ts from the TS functions named in each table. Do not edit by hand: run rust/fixtures/tools/capture-golden.sh.';
/** JSON cannot hold NaN / ±Infinity; tables spell them as strings. */
const special = (x: number): number | string => (Number.isFinite(x) ? x : String(x));
const hm = (min: number) => `${String(Math.floor(min / 60)).padStart(2, '0')}:${String(min % 60).padStart(2, '0')}`;
const toMin = (hhmm: string) => Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3, 5));

(async () => {
  // ------------------------------------------------------------ zones.json
  const ZONES = [M.DEFAULT_ZONES, { neutral: 25, strong: 50 }, { neutral: 10, strong: 30 }, { neutral: 0, strong: 0 }];
  const NEEDLES = [-250, -100, -58, -40.0001, -40, -39.99, -25, -15.0001, -15, -14.9999, -0.5, 0, 0.5, 14.9999, 15, 20, 25, 39.99, 40, 40.0001, 71, 100, 250];
  put('zones.json', canonLines(
    { about: 'zoneOf(needle, zones) — |n| < neutral → neutral, |n| ≥ strong → strong-*; clampNeedle; zoneSegments. zones {0,0} is a degenerate config: TS answers strong-short for 0.', source: SOURCE },
    {
      zone_of: ZONES.flatMap((z) => NEEDLES.map((n) => ({ needle: n, zones: z, out: M.zoneOf(n, z) }))),
      clamp_needle: [-1e9, -100.5, -100, -3.3, 0, 42.42, 100, 100.5, 1e9, NaN, Infinity, -Infinity].map((n) => ({ in: special(n), out: M.clampNeedle(n) })),
      zone_segments: ZONES.map((z) => ({ zones: z, out: M.zoneSegments(z) })),
    },
  ));

  // ------------------------------------------------------------ gauge.json
  // Gauge.tsx: W 320, H 200, CX 160, CY 172, R0 104, R1 142.
  const G = { cx: 160, cy: 172, r0: 104, r1: 142 };
  const corners = (from: number, to: number) => [
    M.polar(G.cx, G.cy, G.r1, M.needleAngle(from)),
    M.polar(G.cx, G.cy, G.r1, M.needleAngle(to)),
    M.polar(G.cx, G.cy, G.r0, M.needleAngle(to)),
    M.polar(G.cx, G.cy, G.r0, M.needleAngle(from)),
  ];
  const angleGrid = [-250, -100.5, ...Array.from({ length: 41 }, (_, i) => -100 + i * 5), 100.5, 250, -33.3, 12.34];
  put('gauge.json', canonLines(
    { about: 'needleAngle: −100 → 180°, 0 → 90°, +100 → 0° (counter-clockwise from +x, clamped). polar(cx, cy, r, deg) with y down. arcPath is the SVG string of a zone segment; `corners` are its four points (outer from, outer to, inner to, inner from) for a non-SVG painter.', geometry: G, source: SOURCE },
    {
      needle_angle: angleGrid.map((n) => ({ needle: n, out: M.needleAngle(n) })),
      polar: [0, 30, 45, 54, 90, 126, 135, 180, -10, 200].map((a) => ({ cx: G.cx, cy: G.cy, r: G.r1, deg: a, out: M.polar(G.cx, G.cy, G.r1, a) })),
      arc: ZONES.slice(0, 2).flatMap((z) => M.zoneSegments(z).map((s) => ({ zones: z, key: s.key, from: s.from, to: s.to, path: M.arcPath(G.cx, G.cy, G.r0, G.r1, s.from, s.to), corners: corners(s.from, s.to) }))),
    },
  ));

  // ------------------------------------------------------------ windows.json
  // When does each exchange open, on the Kyiv clock, on a given Kyiv date?
  const wall = (tz: string, t: Date) => {
    const p = Object.fromEntries(new Intl.DateTimeFormat('en-CA', { timeZone: tz, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).formatToParts(t).map((x) => [x.type, x.value]));
    return { date: `${p.year}-${p.month}-${p.day}`, minutes: Number(p.hour) * 60 + Number(p.minute) };
  };
  const instantOf = (tz: string, date: string, hhmm: string): Date => {
    for (let off = -14; off <= 14; off++) {
      const t = new Date(Date.parse(`${date}T${hhmm}:00Z`) - off * 3_600_000);
      const w = wall(tz, t);
      if (w.date === date && w.minutes === toMin(hhmm)) return t; // the earlier one on an ambiguous hour
    }
    throw new Error(`no instant for ${date} ${hhmm} in ${tz}`);
  };
  const openKyiv = (date: string, tz: string, hhmm: string) => M.kyivNow(instantOf(tz, date, hhmm)).minutes;
  /** TZ №21: a window starts 30 min before its cash open and ends 2 h after it; the review stays at 20:00–21:30 Kyiv. */
  const tzTimes = (date: string) => {
    const xo = openKyiv(date, 'Europe/Berlin', '09:00');
    const ny = openKyiv(date, 'America/New_York', '09:30');
    return { eu: { at: xo - 30, open: xo, until: xo + 120 }, us: { at: ny - 30, open: ny, until: ny + 120 }, review: { at: toMin('20:00'), open: null, until: toMin('21:30') } };
  };
  /** windowState with the TZ's exchange-driven clock; otherwise the same rule as the TS one. */
  const windowStateTz = (id: M.MacroWindowId, day: M.MacroDay, date: string, now: { date: string; minutes: number }): M.WindowState => {
    if (id === 'review') {
      if (day.review.mech && day.review.agent) return 'ready';
      if (day.review.mech || day.review.agent) return 'partial';
    } else {
      const r = day[id];
      if (r.scale && r.verdict) return 'ready';
      if (r.scale || r.verdict) return 'partial';
    }
    if (date > now.date) return 'waiting';
    if (date < now.date) return 'missed';
    const w = tzTimes(now.date)[id];
    if (now.minutes < w.at - 5) return 'waiting';
    if (now.minutes <= w.until) return 'due';
    return 'missed';
  };
  const BASE_DATE = '2026-09-14';
  const base = canonValue(await loadDay(vault, BASE_DATE), vault) as M.MacroDay;
  type Pres = { eu: string; us: string; review: string };
  const NONE: Pres = { eu: 'none', us: 'none', review: 'none' };
  const ALL: Pres = { eu: 'both', us: 'both', review: 'both' };
  const shape = (p: Pres, date: string): M.MacroDay => {
    const d = JSON.parse(JSON.stringify(base)) as M.MacroDay;
    d.date = date;
    for (const r of ['eu', 'us'] as const) {
      if (!['scale', 'both'].includes(p[r])) d[r].scale = null;
      if (!['verdict', 'both'].includes(p[r])) d[r].verdict = null;
    }
    if (!['mech', 'both'].includes(p.review)) d.review.mech = null;
    if (!['agent', 'both'].includes(p.review)) d.review.agent = null;
    return d;
  };
  const kyivUtc = (date: string, hhmm: string) => instantOf('Europe/Kyiv', date, hhmm);
  const addDays = (date: string, n: number) => new Date(Date.parse(`${date}T12:00:00Z`) + n * 86_400_000).toISOString().slice(0, 10);
  const DATES: [string, string][] = [
    ['2026-09-15', 'Tue, summer time on both sides (Kyiv−NY 7 h)'],
    ['2026-11-25', 'Wed, winter time on both sides (7 h)'],
    ['2026-03-09', 'Mon, 2026 spring mismatch: US moved 2026-03-08, EU moves 2026-03-29 (6 h)'],
    ['2026-03-27', 'Fri, last trading day of the 2026 spring mismatch'],
    ['2026-03-30', 'Mon, first day after the EU change (7 h again)'],
    ['2026-10-26', 'Mon, 2026 autumn mismatch: EU moved 2026-10-25, US moves 2026-11-01 (6 h)'],
    ['2026-10-30', 'Fri, last trading day of the 2026 autumn mismatch'],
    ['2026-11-02', 'Mon, first day after the US change (7 h again)'],
    ['2027-03-15', 'Mon, 2027 spring mismatch: US 2027-03-14, EU 2027-03-28'],
    ['2027-03-26', 'Fri, last trading day of the 2027 spring mismatch'],
    ['2027-11-01', 'Mon, 2027 autumn mismatch: EU 2027-10-31, US 2027-11-07'],
    ['2027-11-08', 'Mon, first day after the 2027 US change'],
  ];
  const TIMES = ['00:00', '06:59', '09:24', '09:25', '09:30', '10:00', '12:00', '12:01', '13:00', '14:54', '14:55', '15:00', '15:30', '15:54', '15:55', '16:00', '17:30', '17:31', '18:30', '18:31', '19:29', '19:30', '19:54', '19:55', '20:00', '21:30', '21:31', '23:59'];
  const cases: unknown[] = [];
  const add = (now: Date, shown: string, p: Pres, note?: string) => {
    const k = M.kyivNow(now);
    const day = shape(p, shown);
    const ts = Object.fromEntries(M.MACRO_WINDOWS.map((id) => [id, M.windowState(id, day, shown, k)]));
    const tz = Object.fromEntries(M.MACRO_WINDOWS.map((id) => [id, windowStateTz(id, day, shown, k)]));
    const t = tzTimes(k.date);
    cases.push({ utc: now.toISOString(), kyiv: k, shown, presence: p, ts, tz, tz_differs: JSON.stringify(ts) !== JSON.stringify(tz), us_window_tz: `${hm(t.us.at)}–${hm(t.us.until)}`, default_window_ts: M.defaultWindow(day, shown, k), ...(note ? { note } : {}) });
  };
  for (const [d] of DATES) for (const t of TIMES) add(kyivUtc(d, t), d, NONE);
  const PRES: Pres[] = [
    { eu: 'scale', us: 'none', review: 'none' }, { eu: 'verdict', us: 'none', review: 'none' }, { eu: 'both', us: 'none', review: 'none' },
    { eu: 'both', us: 'scale', review: 'none' }, { eu: 'both', us: 'both', review: 'none' }, { eu: 'none', us: 'both', review: 'none' },
    { eu: 'both', us: 'both', review: 'mech' }, { eu: 'both', us: 'both', review: 'agent' }, ALL, { eu: 'none', us: 'none', review: 'mech' },
  ];
  for (const d of ['2026-09-15', '2026-10-26']) for (const t of ['09:00', '09:40', '13:30', '15:10', '16:10', '19:40', '20:10']) for (const p of PRES) add(kyivUtc(d, t), d, p);
  for (const d of ['2026-09-15', '2026-10-26', '2027-03-15']) {
    for (const t of ['00:00', '10:00', '16:10', '23:59']) {
      add(kyivUtc(d, t), addDays(d, -1), NONE, 'shown day is yesterday');
      add(kyivUtc(d, t), addDays(d, 1), NONE, 'shown day is tomorrow');
      add(kyivUtc(d, t), addDays(d, -3), ALL, 'a past day with everything');
      add(kyivUtc(d, t), addDays(d, -1), { eu: 'both', us: 'none', review: 'none' }, 'yesterday, Europe only');
    }
  }
  // Kyiv midnight is not UTC midnight: the same UTC day holds two Kyiv dates.
  for (const iso of ['2026-10-25T20:59:00Z', '2026-10-25T21:59:00Z', '2026-10-25T22:00:00Z', '2026-06-30T20:59:00Z', '2026-06-30T21:00:00Z']) add(new Date(iso), '2026-10-26', NONE, 'around Kyiv midnight');
  put('windows.json', canonLines(
    {
      about: 'windowState(id, day, shown, kyivNow(utc)) for a grid of instants. `ts` = the old app (Kyiv clock fixed: eu 09:30–12:00, us 16:00–18:30, review 20:00–21:30; «due» from 5 min before). `tz` = TZ №21: a window starts 30 min before its cash open (Xetra 09:00 Europe/Berlin, NYSE 09:30 America/New_York) and lasts until 2 h after it, so in the DST-mismatch weeks the US window is 15:00–17:30 Kyiv. Rust desk_core::windows must match `tz`; it equals `ts` everywhere else. The day is expected/day_2026-09-14.json with `date` = shown and the files absent per `presence` set to null. default_window_ts is defaultWindow() — the TZ does not change it.',
      window_times_ts: M.WINDOW_TIMES,
      base_day: `expected/day_${BASE_DATE}.json`,
      presence_values: { eu_us: ['none', 'scale', 'verdict', 'both'], review: ['none', 'mech', 'agent', 'both'] },
      dates: Object.fromEntries(DATES),
      source: SOURCE,
    },
    { cases },
  ));

  // ------------------------------------------------------------ stats.json
  const csvRel = 'vault/indices-kb/journal/scale_factor_scores.csv';
  const demoCsv = readFileSync(path.join(vault, 'indices-kb', 'journal', 'scale_factor_scores.csv'), 'utf8');
  const H = 'date,region,factor,group,share,side,strength,dir,hit';
  const SYN: Record<string, string> = {
    empty: '',
    header_only: H + '\n',
    bom_crlf: '\ufeff' + [H, '2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1', '2026-10-08,eu,bias_a,g,12,LONG,1,SHORT,0'].join('\r\n') + '\r\n',
    window_edges: [H, '2026-09-12,eu,a,g,1,LONG,1,LONG,1', '2026-09-13,eu,a,g,1,LONG,1,LONG,1', '2026-10-12,eu,a,g,1,LONG,1,LONG,0', '2026-10-13,eu,a,g,1,LONG,1,LONG,1'].join('\n'),
    silent_and_bad: [H, '2026-10-07,eu,a,g,1,,0,LONG,', '2026-10-07,EU,a,g,1,LONG,1,LONG,1', '2026-10-07,eu,a,g,1,LONG,1,LONG,1.0', '2026-10-07,asia,a,g,1,LONG,1,LONG,1', 'junk,eu,a,g,1,LONG,1,LONG,1', '2026-10-07,eu,a,g,1,LONG,1,LONG,0'].join('\n'),
    sort_ties: [H, ...['m_vol', 'm15_flow', 'Room', 'room', 'bias_a', 'Bias', 'night_sweep', 'h1_flow', 'точка', 'Точка', 'а_кир'].map((f) => `2026-10-07,eu,${f},g,1,LONG,1,LONG,1`), '2026-10-07,us,m_vol,g,1,LONG,1,LONG,1'].join('\n'),
    quoted: [H, '2026-10-07,eu,"bias, a",g,1,LONG,1,LONG,1', '2026-10-07,eu,"say ""hi""",g,1,LONG,1,LONG,0', '"2026-10-07",eu,"multi\nline",g,1,LONG,1,LONG,1'].join('\n'),
    short_row: [H, '2026-10-07,eu,a,g,1,LONG,1,LONG,1', '2026-10-07,eu,b'].join('\n'),
  };
  const statDates = ['2026-08-31', '2026-09-01', '2026-09-10', '2026-09-14', '2026-09-24', '2026-09-30', '2026-10-01', '2026-10-26'];
  const CSV_TEXTS = [
    'a,b,c\n1,2,3', 'a,b,c\r\n1,"x, y",3\n4,"he said ""hi""",\n', '\ufeffa,b\n1,2\n', 'a,b\n1\n', 'a,b\n1,2,3\n', 'a\n\n\nb\n', 'a,b\n"open quote,2\n3,4\n', '', '\n\n', 'a,b\r\r\n1,2', ',\n,', 'a,b\n"",""\n', 'h\nlast-no-newline',
  ];
  const LESSONS = [
    ['# Уроки\n\n## 2026-10-08\n- 2026-10-08: один\n- 2026-10-08: два\n* 2026-10-09: три\n', 2],
    ['- a\n- b\n- c', 15],
    ['\ufeff- первая после BOM\r\n- вторая\r\n', 15],
    ['-нет пробела\n-  два пробела\n  - с отступом\n+ плюс\n- \n-\t табуляция\n', 15],
    ['', 15],
    [Array.from({ length: 20 }, (_, i) => `- урок ${i + 1}`).join('\n'), 15],
    ['- x\n- y\n', 0],
  ] as const;
  put('stats.json', canonLines(
    { about: 'parseCsv (RFC-4180-ish), factorStats(rows, date, days=30): rows with hit 1/0 and region eu/us whose date is 0..days−1 days before `date`, sorted by total desc then factor.localeCompare (ICU collation, NOT byte order — see sort_ties). lessonsTail(md, n). `csv_file` paths are relative to rust/fixtures/golden/.', csv_inline: SYN, source: SOURCE },
    {
      parse_csv: CSV_TEXTS.map((t) => ({ text: t, out: M.parseCsv(t) })),
      factor_stats: [
        ...statDates.map((d) => ({ csv_file: csvRel, date: d, days: 30, out: M.factorStats(M.parseCsv(demoCsv), d) })),
        { csv_file: csvRel, date: '2026-09-24', days: 7, out: M.factorStats(M.parseCsv(demoCsv), '2026-09-24', 7) },
        { csv_file: csvRel, date: '2026-09-24', days: 1, out: M.factorStats(M.parseCsv(demoCsv), '2026-09-24', 1) },
        ...Object.keys(SYN).map((k) => ({ csv_inline: k, date: '2026-10-12', days: 30, out: M.factorStats(M.parseCsv(SYN[k]), '2026-10-12') })),
      ],
      lessons_tail: LESSONS.map(([md, n]) => ({ md, n, out: M.lessonsTail(md, n) })),
    },
  ));

  // ------------------------------------------------------------ names.json
  const dates = ['2026-10-12', '2026-01-05'];
  const fileNames: unknown[] = [];
  for (const d of dates) {
    for (const r of ['eu', 'us'] as const) {
      fileNames.push({ kind: 'human_read', date: d, region: r, out: M.humanReadFile(d, r), from: 'humanReadFile' });
      fileNames.push({ kind: 'remember', date: d, region: r, out: M.rememberFile(d, r), from: 'rememberFile' });
      fileNames.push({ kind: 'drift', date: d, region: r, out: M.driftFile(d, r), from: 'driftFile' });
      // The rest are template strings inside loadDay()/regionSnapshot() in macro-state.ts, transcribed.
      fileNames.push({ kind: 'scale', date: d, region: r, out: `scale_${d}_${r}.json`, from: 'macro-state.ts regionSnapshot' });
      fileNames.push({ kind: 'macro', date: d, region: r, out: `macro_${d}_${r}.json`, from: 'macro-state.ts regionSnapshot' });
      fileNames.push({ kind: 'macro_verdict', date: d, region: r, out: `macro_${d}_${r}_verdict.json`, from: 'macro-state.ts regionSnapshot' });
      fileNames.push({ kind: 'shots', date: d, region: r, out: `shots_${d}_${r}.json`, from: 'macro-state.ts regionSnapshot' });
      fileNames.push({ kind: 'shots_review', date: d, region: r, out: `shots_${d}_${r}_review.json`, from: 'macro-state.ts loadDay' });
      fileNames.push({ kind: 'note', date: d, region: r, out: `Макро-обзоры/${d} ${r === 'eu' ? 'Европа' : 'США'}.md`, from: 'macro-state.ts regionSnapshot (vault-relative, «/»)' });
    }
    fileNames.push({ kind: 'day_answers', date: d, region: null, out: M.dayAnswersFile(d), from: 'dayAnswersFile' });
    fileNames.push({ kind: 'week', date: d, region: null, out: M.weekFile(d), from: 'weekFile' });
    fileNames.push({ kind: 'review', date: d, region: null, out: `review_${d}.json`, from: 'macro-state.ts loadDay' });
    fileNames.push({ kind: 'review_agent', date: d, region: null, out: `review_${d}_agent.json`, from: 'macro-state.ts loadDay' });
    fileNames.push({ kind: 'review_note', date: d, region: null, out: `Макро-обзоры/${d} Итог дня.md`, from: 'macro-state.ts loadDay' });
  }
  fileNames.push({ kind: 'ideas', date: null, region: null, out: M.ideasFile(), from: 'ideasFile' });
  const stateNames = [
    'scale_2026-10-09_us.json', 'review_2026-10-08.json', 'review_2026-10-08_agent.json', 'macro_2026-10-07_eu_verdict.json', 'macro_2026-10-07_eu.json',
    'scale_backtest_us.json', 'bias_2026-10-05.json', 'shots_2026-10-09_us.json', 'shots_2026-10-09_eu_review.json', 'human_read_2026-10-12_us.json',
    'day_answers_2026-10-12.json', 'remember_2026-10-12_eu.json', 'drift_2026-10-12_us.json', 'week_2026-10-16.json', 'regime_2026-10-12.json', 'ideas.json',
    'scale_2026-10-09_us.json.4242.tmp', 'scale_2026-13-45_eu.json', 'scale_2026-10-9_eu.json', 'scale_2026-10-09', 'scale_2026-10-09x.json', 'Scale_2026-10-09_eu.json',
    'xscale_2026-10-09_eu.json', 'foo_2026-10-09.json', 'week_2026-10-16_extra.json', 'scale_2026-10-09.json', 'review_2026-10-08.md', 'macro_20261008_eu.json', '',
  ];
  const tmp = mkdtempSync(path.join(tmpdir(), 'names-'));
  mkdirSync(path.join(tmp, 'indices-kb', 'state'), { recursive: true });
  for (const n of stateNames.filter(Boolean)) writeFileSync(path.join(tmp, 'indices-kb', 'state', n), '{}');
  const listed = await listDays(tmp);
  const SHOT_PATHS: unknown[] = [
    'indices-kb/shots/2026-10-09/macro_us/m15.png', 'indices-kb\\shots\\2026-10-09\\m15.png', 'indices-kb/shots/../state/x.png', 'indices-kb/shots/2026-10-09/m15.jpg',
    'indices-kb/state/m15.png', '/etc/indices-kb/shots/a/b.png', 'C:/indices-kb/shots/a/b.png', 'c:\\indices-kb\\shots\\a\\b.png', 'indices-kb/shots/a.png', 'indices-kb/shots/a/B.PNG',
    'indices-kb/shots/a//b.png', 'indices-kb/shots/./a/b.png', 'indices-kb/shots/a/b.png\u0000', 'indices-kb/shots/' + 'x/'.repeat(200) + 'b.png', '', 42, null,
  ];
  put('names.json', canonLines(
    { about: 'File names the desk builds (state_file) and recognizes (dateOfStateFile; listDays keeps only scale_/macro_/review_ names). TS quirks: dateOfStateFile does not check the date is real (2026-13-45 passes) and accepts a temp file `…json.4242.tmp`; listDays therefore lists a date that has only a temp file. Rust state_file builds names only from a checked NaiveDate, so an invalid date never reaches it.', source: SOURCE },
    {
      state_file: fileNames,
      date_of_state_file: stateNames.map((n) => ({ name: n, out: M.dateOfStateFile(n) })),
      list_days: [{ names: stateNames.filter(Boolean), out: listed.days }],
      is_shot_path: SHOT_PATHS.map((p) => ({ rel: p, out: M.isShotPath(p) })),
    },
  ));

  // ------------------------------------------------------------ normalize.json
  const N: unknown[] = [];
  const nz = (fn: string, args: unknown[], outv: unknown) => N.push({ fn, args, out: outv === undefined ? null : outv });
  const HR: [unknown, string, M.Region][] = [
    [{ side: 'LONG', at: '2026-10-12 09:31', before_scale: true }, '2026-10-12', 'eu'],
    [{ side: 'UP' }, '2026-10-12', 'eu'], ['LONG', '2026-10-12', 'eu'], [null, '2026-10-12', 'eu'], [[], '2026-10-12', 'eu'],
    [{ side: 'NO_TRADE', before_scale: false, region: 'xx' }, '2026-10-12', 'us'], [{ side: 'NEUTRAL', before_scale: 'no' }, '2026-10-12', 'us'],
    [{ side: 'SHORT', before_scale: 0, date: 20261012, at: 931, region: 'eu', extra: 1 }, '2026-10-12', 'us'], [{ side: 'long' }, '2026-10-12', 'eu'],
  ];
  for (const [raw, d, r] of HR) nz('normalizeHumanRead', [raw, d, r], M.normalizeHumanRead(raw, d, r));
  const DA: unknown[] = [
    { regions: { eu: { trade: 'long', by_rule: 'yes' }, us: { trade: 'maybe' }, xx: { trade: 'short' } }, note: 'ждал вторую манипуляцию' },
    { regions: {} }, { regions: { us: { trade: 'none', by_rule: 'what' } }, note: 7 }, { note: '' }, { regions: [{ trade: 'long' }], note: 'x' },
    { regions: { eu: { trade: 'short', by_rule: 'unsure' }, us: null }, at: '2026-10-12 20:05', date: '2026-10-11' }, null, 'x', [],
    { regions: { eu: 'long' } }, { regions: 'eu' },
  ];
  for (const raw of DA) nz('normalizeDayAnswers', [raw, '2026-10-12'], M.normalizeDayAnswers(raw, '2026-10-12'));
  const REM: unknown[] = [
    { date: '2026-10-12', region: 'eu', asof: '2026-10-12 09:31', items: [
      { kind: 'lesson', text: 'В пилу ждать вторую манипуляцию', why: 'те же теги: пила, пятница', ref: 'lessons.md 08.10', tags: ['eu', 'пила'] },
      { kind: 'rule', text: 'Пятница в Европе — риск ×0.5' }, { kind: 'similar', text: 'Похожий день', why: '', ref: '2026-10-08 eu', tags: 'nope' },
      { kind: 'lesson', text: 'четвёртый — лишний' }, { kind: 'joke', text: 'не тот вид' }, { kind: 'lesson', text: '' }, 'garbage'],
      conditions: { region: 'eu', weekday: 'fri', phase: 'пила', night: 'wide', night_adr: 0.9, light: 'yellow', earnings_giant_prev: false, tags: ['eu', 'пила', 3] },
      similar: { date: '2026-10-08', region: 'eu', distance: 0.12, needle: -38, dir: 'SHORT', chg_adr: -0.4, human_side: 'SHORT' }, lesson_candidates: 4, enough_history: true },
    { items: [] }, { date: '2026-10-12' }, [], 'x', null,
    { items: [{ kind: 'lesson', text: '  пробелы  ', why: 5, ref: null, tags: ['a', 1, ' ', ' b '] }], conditions: [], similar: { date: 20261001 }, lesson_candidates: 2.5, enough_history: 'yes', region: 'EU' },
    { items: [{ kind: 'rule', text: '\ufeffс BOM' }], conditions: { night_adr: 'x', tags: 'eu' }, similar: { date: '2026-10-01' }, lesson_candidates: -1 },
  ];
  for (const raw of REM) nz('normalizeRemember', [raw, '2026-10-12', 'us'], M.normalizeRemember(raw, '2026-10-12', 'us'));
  const DR: unknown[] = [
    { date: '2026-10-12', region: 'eu', days: 20, enough_data: true, signals: [{ kind: 'phase', text: 'фаза: тренд → пила', severity: 'warn', data: { from: 'тренд' } }, { kind: 'adr', text: 'ADR сжался на 30 %' }, { kind: 'factor', text: '', severity: 'warn' }, { severity: 'warn' }, 5] },
    { signals: [] }, { date: '2026-10-12' }, null, { signals: { kind: 'phase' } }, { days: -5.5, signals: [{ text: 'без вида', severity: 'WARN', data: [1] }], enough_data: 1 },
  ];
  for (const raw of DR) nz('normalizeDrift', [raw, '2026-10-12', 'eu'], M.normalizeDrift(raw, '2026-10-12', 'eu'));
  const WK: unknown[] = [
    { week_start: '2026-09-14', week_end: '2026-09-18', days: 5, enough_data: true, lines: ['рынок: пила', 'сработало: сетап 3', '', 3, 'урок повторился'], stats: { hits: 3 }, trader: ['4 стопа из 5 — вход до закрытия IB', ''], share_proposal: { factor: 'Точка A', from: 12, to: 10, why: '3 промаха из 5.' } },
    { lines: [] }, { lines: ['x'], share_proposal: { why: 'no factor' } }, { lines: ['x'], share_proposal: { factor: 'f' } }, { week_start: '2026-09-14' }, null,
    { lines: 'пять строк' }, { lines: ['x'], stats: [1], share_proposal: { factor: 'f', from: true, to: '10 %', why: '  w  ' }, days: 4.6 },
  ];
  for (const raw of WK) nz('normalizeWeek', [raw], M.normalizeWeek(raw));
  const mk = (i: number, status: string, date = `2026-10-${String(i).padStart(2, '0')}`) => ({ id: `i${i}`, date, text: `идея ${i}`, side: 'LONG', region: 'eu', source: 'chat', confirmed: i, refuted: 1, status, backtest: i % 2 ? { days: 30, hit: 18, rate: 0.6 } : null, last_check: date });
  const IDEAS: unknown[] = [
    { asof: '2026-10-12', items: [mk(1, 'open'), mk(2, 'rule'), mk(3, 'proposed'), mk(4, 'dropped'), mk(5, 'open'), mk(6, 'open'), mk(7, 'open'), mk(8, 'open'), { status: 'open', text: '' }, { status: 'maybe', text: 'x' }, null] },
    { items: [{ text: 'без id', status: 'open' }] }, { asof: 'x' }, [],
    { items: [{ text: 'a', status: 'open', date: '2026-10-01' }, { text: 'b', status: 'open', date: '2026-10-01' }, { text: 'c', status: 'proposed', date: '' }, { text: 'd', status: 'open', date: '2026-10-1' }] },
    { items: [{ text: 'ок', status: 'open', confirmed: -3, refuted: 2.5, backtest: [], date: 20261001 }] },
  ];
  for (const raw of IDEAS) {
    const ideas = M.normalizeIdeas(raw);
    nz('normalizeIdeas', [raw], ideas);
    N.push({ fn: 'visibleIdeas', args: [raw], out: M.visibleIdeas(ideas).map((x) => x.id), note: 'args[0] is the RAW file; the ids of visibleIdeas(normalizeIdeas(raw))' });
  }
  const SHOTS: [unknown, M.Region][] = [
    [{ date: '2026-10-09', region: 'us', asof: '2026-10-09 15:55', ok: true, error: null, shots: [
      { name: 'h1', tf: 'H1', title: 'H1 — зоны и цели дня', path: 'indices-kb/shots/2026-10-09/macro_us/h1.png', verify_ok: true, legend: [] },
      { name: 'm15', tf: 'M15', title: 'M15 — ночь и уровни у открытия', path: 'indices-kb/shots/2026-10-09/macro_us/m15.png', verify_ok: false,
        legend: [{ key: 'manip1+DO', label: 'манипуляция: открытие дня', kind: 'manip', color: '#d32f2f' }, { key: 'x', kind: 'level' }, 'junk', { label: 'dashed', style: 'dashed' }, { label: 'dotted', style: 'dotted', kind: '' }] },
      { name: 'nopath', tf: 'D1' }] }, 'eu'],
    [null, 'eu'], [[1, 2], 'eu'], ['x', 'eu'], [{ hello: 1 }, 'eu'], [{ ok: false, error: 'dry-run', shots: null }, 'eu'],
    [{ shots: [{ path: 'indices-kb/shots/a/c.png' }], error: '' }, 'us'], [{ shots: [], error: 0 }, 'us'], [{ ok: 'yes', shots: [{ name: 5, path: 7 }] }, 'eu'],
    [{ ok: true, region: 'us', error: 42, date: 20261009 }, 'eu'],
  ];
  for (const [raw, r] of SHOTS) nz('normalizeShots', [raw, r], M.normalizeShots(raw, r));
  const v1 = { region: 'us', date: '2026-10-08', asof: '', sym: 'ES', version: 'v1', factors: [
    { id: 'a', name: 'A', group: 'техника', weight: 3, side: 'LONG', strength: 1, points: 3 }, { id: 'b', name: 'B', group: 'сессия', weight: 1, side: 'SHORT', strength: 0.5, points: 0.5 },
    { id: 'c', name: 'C', group: 'сессия', weight: 1, side: '', strength: 0, points: 0 }],
    result: { long: 3, short: 0.5, max: 5, needle: 71, zone: 'сильный лонг', side: 'LONG', groups: { техника: { long: 3, short: 0, needle: 100 }, сессия: { long: 0, short: 0.5, needle: -100 } } },
    light: { color: 'red', label: '' } };
  const SCALES: unknown[] = [
    v1,
    { factors: [{ id: 'a', name: 'A', group: 'g', side: 'LONG', strength: 1, share: 12, points: 12 }], result: { long: 12, short: 0, undecided: 88, needle: 12, zone: '', side: null } },
    { ...v1, result: { ...v1.result, max: undefined } },
    { ...v1, factors: [...v1.factors, { id: 'd', name: 'D', group: 'сессия', weight: 2, side: 'SHORT', strength: 1, points: 2, on: false }], after: { long: 3, short: 1.5, max: 5, needle: 33 } },
    { factors: [{ id: 'a', group: 'g', side: 'LONG', strength: 1, share: 33.333, points: 33.333 }], result: { long: 33.333, short: 10.05, needle: 23, groups: { g: { long: 33.333, short: 10.05, needle: 23 } } } },
    { factors: 'x', result: { long: 1, short: 2, undecided: 97, needle: -1 } },
    { factors: [{ id: 'a', weight: 1, side: 'LONG', strength: 'strong' }, { id: 'b', weight: '2', side: 'SHORT', strength: 1 }], result: { long: 1, short: 1, needle: 0 } },
    { factors: [{ id: 'a', weight: 0, side: 'LONG', strength: 1 }], result: { long: 0, short: 0, needle: 0 } },
    { factors: [{ id: 'a', weight: 1, side: 'LONG', strength: 1 }], result: { long: 0.15, short: 0.05, max: 1, needle: 10.4 }, after: null },
  ];
  for (const raw of SCALES) nz('normalizeScale', [raw], M.normalizeScale(raw as M.ScaleState));
  put('normalize.json', canonLines(
    { about: 'normalize*(raw, …) of src/shared/macro.ts: what survives a file of the wrong shape. `out: null` = «not this kind of file». normalizeScale is applied by loadDay only when the file has a truthy `result`.', source: SOURCE },
    { cases: N },
  ));

  // ------------------------------------------------------------ prompts.json
  const SHARES = [0, 0.4, 0.5, 1, 1.5, 2.5, 12, 49.4, 49.5, 50, 50.5, 80, -3, NaN];
  put('prompts.json', canonLines(
    { about: 'What the desk sends to the agent: runCommand(id, date), discussPrompt(region, side, share, text, date) — share rounded with Math.round (half up) and clamped to 1..50 (NaN → NaN in TS: «Доля: NaN %»), text trimmed —, macroChatTitle(date).', source: SOURCE },
    {
      run_command: M.MACRO_WINDOWS.flatMap((id) => ['2026-10-12', '2027-01-04'].map((d) => ({ id, date: d, out: M.runCommand(id, d) }))),
      discuss_prompt: [
        ...SHARES.map((s) => ({ region: 'eu', side: 'LONG', share: special(s), text: 'DAX у недельного минимума', date: '2026-10-12', out: M.discussPrompt('eu', 'LONG', s, 'DAX у недельного минимума', '2026-10-12') })),
        { region: 'us', side: 'SHORT', share: 10, text: '  две строки\nвторая  ', date: '2026-10-26', out: M.discussPrompt('us', 'SHORT', 10, '  две строки\nвторая  ', '2026-10-26') },
        { region: 'us', side: 'SHORT', share: 10, text: '', date: '2026-10-26', out: M.discussPrompt('us', 'SHORT', 10, '', '2026-10-26') },
      ],
      macro_chat_title: ['2026-10-12', '2026-01-05', '2027-12-31'].map((d) => ({ date: d, out: M.macroChatTitle(d) })),
    },
  ));

  // ------------------------------------------------------------ shots.json
  const index = await listDays(vault);
  const rels = new Set<string>();
  for (const d of index.days) {
    const day = await loadDay(vault, d);
    for (const s of [day.eu.shots, day.us.shots, day.review.shots?.eu, day.review.shots?.us]) for (const x of s?.shots ?? []) rels.add(x.path);
  }
  const readCases: unknown[] = [];
  for (const rel of [...rels].sort().concat(['indices-kb/shots/2026-09-14/macro_eu/missing.png', 'indices-kb/shots/../README.md', 'indices-kb/state/ideas.json', 'README.md'])) {
    const url = await readShot(vault, rel);
    readCases.push({ rel, out: url ? shotRef(url, vault) : null });
  }
  const shotObj = (name: string, tf: string, title = '') => ({ name, tf, title, path: `indices-kb/shots/x/${name}.png`, legend: [] });
  put('shots.json', canonLines(
    { about: 'Screenshot helpers. read_shot is readShot(vault, rel) over the golden vault with the data: URL replaced by {"$shot": rel, "bytes": N} — the shape desk-app hands to the UI instead of base64 (missing / refused / not-a-PNG → null). main_shot gives the index into `shots`.', source: SOURCE },
    {
      read_shot: readCases,
      main_shot: [
        [shotObj('h1', 'H1'), shotObj('m15', 'M15')], [shotObj('h1', 'H1')], [], [shotObj('x', 'm15'), shotObj('M15', 'H1')], [shotObj('a', 'H1'), shotObj('M15', '')],
      ].map((shots) => { const m = M.mainShot(shots); return { shots, out: m ? shots.indexOf(m) : null }; }),
      shot_tab_label: [
        { tf: 'M15', title: 'M15 — ночь и уровни у открытия', name: 'm15' }, { tf: 'H1', title: 'зоны дня', name: 'h1' }, { tf: '', title: '', name: 'h4' },
        { tf: 'm15', title: 'M15: ночь', name: 'x' }, { tf: 'H1', title: 'H1', name: 'h1' }, { tf: '', title: 'только заголовок', name: '' }, { tf: ' H4 ', title: ' H4 · – - :x ', name: 'h4' },
        { tf: 'H1', title: 'H10 большой', name: 'h1' },
      ].map((s) => ({ shot: s, out: M.shotTabLabel(s) })),
      shots_error_text: [
        'dry-run', 'dry_run: no TV', 'DRY RUN', 'dryrun', 'dry-running', null, '', '   ', 'TradingView не запущен', 'нет шкалы с планом: ' + 'x'.repeat(200), 'строка\nс\tпереносом', 'x'.repeat(90), 'x'.repeat(91), 'я'.repeat(95),
      ].map((e) => ({ error: e, out: M.shotsErrorText(e) })),
      legend_color: [
        { color: '#1e8e3e', kind: 'target' }, { color: 'red; background:url(x)', kind: 'manip' }, { color: '', kind: 'weird' }, { color: '#ABC', kind: 'level' }, { color: '#abcd', kind: 'zone' },
        { color: '#11223344', kind: 'fact' }, { color: ' #112233 ', kind: 'open' }, { color: 'rgb(1,2,3)', kind: 'open' }, { color: 5, kind: 'target' }, { color: null, kind: null },
      ].map((l) => ({ item: l, out: M.legendColor(l as never) })),
    },
  ));

  // ------------------------------------------------------------ misc.json
  const ff = (id: string, side: string, share: number, strength: number, on = true, points?: number) => ({ id, name: id, group: 'g', side, share, strength, points: points ?? (side ? share * strength : 0), on });
  const FACT_SETS = [
    [ff('a', 'LONG', 10, 0.5), ff('b', 'LONG', 6, 1), ff('c', 'SHORT', 12, 1), ff('d', '', 5, 0), ff('e', 'SHORT', 8, 1, false)],
    [ff('a', 'LONG', 10, 0.5), ff('b', 'LONG', 5, 1), ff('c', 'LONG', 6, 0, true, 0), ff('d', 'SHORT', 3, 1, true, -1)],
    [],
  ];
  const FMT: [unknown, number | undefined][] = [[undefined, undefined], [1.234, 2], [0.25, 1], [0.125, 2], [2.5, 0], [-0.05, 1], [1.005, 2], [-58, 1], [0, 1], [null, 1], ['5', 1], [NaN, 1], [1e21, 1], [-0.04, 1], [12.345, 1]];
  const UTCS = ['2026-10-25T00:30:00Z', '2026-10-25T00:59:00Z', '2026-10-25T01:00:00Z', '2026-10-25T01:30:00Z', '2026-03-29T00:59:00Z', '2026-03-29T01:00:00Z', '2026-11-25T07:30:00Z', '2026-07-01T06:30:00Z', '2026-12-31T21:59:00Z', '2026-12-31T22:00:00Z'];
  put('misc.json', canonLines(
    { about: 'Small helpers: fmt(x, digits) (JS toFixed — NOT Rust format! on ties, see 0.25/0.125/1.005), isFriday, humanHit, splitFactors (ids per bucket), traderLines, kyivNow(utc) incl. the hours around both 2026 EU switches.', source: SOURCE },
    {
      fmt: FMT.map(([x, d]) => ({ x: typeof x === 'number' ? special(x) : x ?? null, digits: d ?? null, out: d === undefined ? M.fmt(x) : M.fmt(x, d) })),
      is_friday: ['2026-09-18', '2026-10-09', '2026-10-30', '2027-01-01', '2026-09-14', '2026-10-12', 'nope', '', '2026-02-30', '2026-9-18'].map((d) => ({ date: d, out: M.isFriday(d) })),
      human_hit: [['LONG', 'LONG'], ['SHORT', 'LONG'], ['SHORT', 'SHORT'], ['NEUTRAL', 'LONG'], ['NO_TRADE', 'SHORT'], ['LONG', 'FLAT'], [null, 'LONG'], ['LONG', null], ['long', 'long']].map(([s, d]) => ({ side: s, dir: d, out: M.humanHit(s as never, d) })),
      split_factors: FACT_SETS.map((fs) => { const r = M.splitFactors(fs as never); return { factors: fs, out: { long: r.long.map((x) => x.id), short: r.short.map((x) => x.id), silent: r.silent.map((x) => x.id) } }; }),
      trader_lines: [
        { date: '2026-09-18', week: { lines: ['a'], trader: ['из недели'] }, trader: ['из дня'] }, { date: '2026-09-17', week: { lines: ['a'], trader: ['из недели'] }, trader: ['из дня'] },
        { date: '2026-09-18', week: { lines: ['a'] }, trader: ['из дня'] }, { date: '2026-09-18', week: null, trader: [] },
      ].map((x) => ({ ...x, out: M.traderLines({ date: x.date, week: x.week ? M.normalizeWeek(x.week) : null, trader: x.trader }), note: 'week is the RAW week file; it goes through normalizeWeek first' })),
      kyiv_now: UTCS.map((u) => ({ utc: u, out: M.kyivNow(new Date(u)) })),
    },
  ));
  console.log('helpers written to', out);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
