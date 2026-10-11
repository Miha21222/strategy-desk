// Reads the macro-review files out of the owner's vault and watches them.
//
// Read-only by construction: nothing here writes into the vault. A change to a
// scale (an argument, a re-run) happens through the skill in a chat turn, and
// the watcher below is how the picture follows it — the same way it follows a
// run started by the Windows Task Scheduler while the desk was open.
import { promises as fs, watch, type FSWatcher } from 'node:fs';
import path from 'node:path';
import {
  dateOfStateFile,
  DEFAULT_ZONES,
  factorStats,
  isShotPath,
  lessonsTail,
  normalizeScale,
  normalizeShots,
  parseCsv,
  VAULT_PATHS,
  type MacroCollect,
  type MacroDay,
  type MacroIndex,
  type MacroVerdict,
  type Region,
  type RegionSnapshot,
  type ReviewAgent,
  type ReviewMech,
  type ScaleState,
} from '@shared/macro';

const REGION_NOTE: Record<Region, string> = { eu: 'Европа', us: 'США' };

export const statePath = (vault: string) => path.join(vault, ...VAULT_PATHS.state);
const journalPath = (vault: string) => path.join(vault, ...VAULT_PATHS.journal);

async function readJson<T>(file: string): Promise<T | null> {
  try {
    // The Python side writes UTF-8, sometimes with a BOM from PowerShell.
    return JSON.parse((await fs.readFile(file, 'utf8')).replace(/^﻿/, '')) as T;
  } catch {
    return null;
  }
}

async function readText(file: string): Promise<string | null> {
  try {
    return await fs.readFile(file, 'utf8');
  } catch {
    return null;
  }
}

async function exists(file: string): Promise<boolean> {
  try {
    await fs.access(file);
    return true;
  } catch {
    return false;
  }
}

/** Every date with a macro file, newest first. */
export async function listDays(vault: string): Promise<MacroIndex> {
  if (!vault.trim()) return { vault, ok: false, error: 'no-vault', days: [] };
  let names: string[];
  try {
    names = await fs.readdir(statePath(vault));
  } catch {
    return { vault, ok: false, error: 'no-state-dir', days: [] };
  }
  const days = new Set<string>();
  for (const n of names) {
    if (!/^(scale|macro|review)_/.test(n)) continue;
    const d = dateOfStateFile(n);
    if (d) days.add(d);
  }
  return { vault, ok: true, days: [...days].sort().reverse() };
}

/**
 * The collect file is large (hundreds of headlines). The renderer gets the
 * part it draws; the full text stays on disk for the agent.
 */
function trimCollect(raw: Record<string, unknown> | null): MacroCollect | null {
  if (!raw) return null;
  const h = raw.headlines as Record<string, unknown> | undefined;
  return {
    asof_eet: raw.asof_eet as string | undefined,
    open_eet: raw.open_eet as string | undefined,
    calendar: (raw.calendar as MacroCollect['calendar']) ?? null,
    markets: Array.isArray(raw.markets)
      ? (raw.markets as Record<string, unknown>[]).map((m) => ({ label: String(m.label ?? m.symbol ?? ''), chg_pct: m.chg_pct as number | null, main: Boolean(m.main), kind: m.kind as string | undefined }))
      : [],
    headlines: h ? { count: h.count as number, relevant: h.relevant as number, tone_sum: h.tone_sum as number, tags: h.tags as Record<string, number> } : null,
    tilt: (raw.tilt as MacroCollect['tilt']) ?? null,
    regime: raw.regime ?? null,
    prev_day_check: Array.isArray(raw.prev_day_check) ? raw.prev_day_check : [],
    previous_verdict: (raw.previous_verdict as MacroVerdict | null) ?? null,
  };
}

async function regionSnapshot(vault: string, date: string, region: Region): Promise<RegionSnapshot> {
  const dir = statePath(vault);
  const [scale, verdictFile, collectRaw, shotsRaw] = await Promise.all([
    readJson<ScaleState>(path.join(dir, `scale_${date}_${region}.json`)),
    readJson<MacroVerdict>(path.join(dir, `macro_${date}_${region}_verdict.json`)),
    readJson<Record<string, unknown>>(path.join(dir, `macro_${date}_${region}.json`)),
    readJson<unknown>(path.join(dir, `shots_${date}_${region}.json`)),
  ]);
  // `note` copies the verdict into the collect file too; the verdict file is
  // the one the agent writes, so it wins when both exist.
  const verdict = verdictFile ?? ((collectRaw?.verdict as MacroVerdict | null) || null);
  const noteRel = path.join(...VAULT_PATHS.notes, `${date} ${REGION_NOTE[region]}.md`);
  return {
    region,
    // Older scale files (v1) lack the percentages the screen draws; a file
    // without a result at all is half-written and is shown as «not yet».
    scale: scale && scale.result ? normalizeScale(scale) : null,
    verdict,
    collect: trimCollect(collectRaw),
    note: (await exists(path.join(vault, noteRel))) ? noteRel : null,
    shots: normalizeShots(shotsRaw, region),
  };
}

export async function loadDay(vault: string, date: string): Promise<MacroDay> {
  const dir = statePath(vault);
  const jdir = journalPath(vault);
  const reviewNoteRel = path.join(...VAULT_PATHS.notes, `${date} Итог дня.md`);
  const [eu, us, mech, agent, lessonsMd, scoresCsv, cfg, reviewNote] = await Promise.all([
    regionSnapshot(vault, date, 'eu'),
    regionSnapshot(vault, date, 'us'),
    readJson<ReviewMech>(path.join(dir, `review_${date}.json`)),
    readJson<ReviewAgent>(path.join(dir, `review_${date}_agent.json`)),
    readText(path.join(jdir, 'lessons.md')),
    readText(path.join(jdir, 'scale_factor_scores.csv')),
    readJson<{ zones?: { neutral?: number; strong?: number }; regions?: Record<string, { factors?: { id: string; name: string }[] }> }>(path.join(vault, ...VAULT_PATHS.config)),
    exists(path.join(vault, reviewNoteRel)),
  ]);
  const names: Record<string, string> = {};
  for (const r of Object.values(cfg?.regions ?? {})) for (const f of r.factors ?? []) names[f.id] = f.name;
  return {
    date,
    eu,
    us,
    review: { mech, agent, note: reviewNote ? reviewNoteRel : null },
    lessons: lessonsMd ? lessonsTail(lessonsMd) : [],
    stats: scoresCsv ? factorStats(parseCsv(scoresCsv), date) : [],
    names,
    zones: { neutral: cfg?.zones?.neutral ?? DEFAULT_ZONES.neutral, strong: cfg?.zones?.strong ?? DEFAULT_ZONES.strong },
  };
}

/**
 * A note path the renderer may ask to open: it must be a markdown file inside
 * the vault's notes folder. Anything else is refused, so the bridge cannot be
 * turned into "open an arbitrary path".
 */
export function resolveNote(vault: string, rel: string): string | null {
  if (!vault || typeof rel !== 'string' || !rel.endsWith('.md')) return null;
  const root = path.resolve(vault, ...VAULT_PATHS.notes);
  const full = path.resolve(vault, rel);
  return full.startsWith(root + path.sep) ? full : null;
}

/**
 * A chart screenshot the renderer may ask for: a `.png` inside the vault's
 * `indices-kb/shots/`. Same rule as `resolveNote` — the bridge reads pictures
 * the skill took, never an arbitrary file.
 */
export function resolveShot(vault: string, rel: string): string | null {
  if (!vault || !isShotPath(rel)) return null;
  const root = path.resolve(vault, ...VAULT_PATHS.shots);
  const full = path.resolve(vault, rel);
  return full.startsWith(root + path.sep) && full.toLowerCase().endsWith('.png') ? full : null;
}

/** A screenshot is a few hundred KB; anything far bigger is not one of ours. */
const SHOT_MAX_BYTES = 12 * 1024 * 1024;
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/**
 * The screenshot as a `data:` URL for an <img>, or null when it is refused,
 * missing, too big or not a PNG. A data URL keeps the renderer off `file:`.
 */
export async function readShot(vault: string, rel: string): Promise<string | null> {
  const full = resolveShot(vault, rel);
  if (!full) return null;
  try {
    // realpath: a symlink inside shots/ must not lead out of it.
    const real = await fs.realpath(full);
    const root = await fs.realpath(path.resolve(vault, ...VAULT_PATHS.shots));
    if (!real.startsWith(root + path.sep)) return null;
    const st = await fs.stat(real);
    if (!st.isFile() || st.size > SHOT_MAX_BYTES || st.size < PNG_MAGIC.length) return null;
    const buf = await fs.readFile(real);
    if (!buf.subarray(0, PNG_MAGIC.length).equals(PNG_MAGIC)) return null;
    return `data:image/png;base64,${buf.toString('base64')}`;
  } catch {
    return null;
  }
}

// ---------------------------------------------------------------- watching

let watcher: FSWatcher | null = null;
let watchedDir = '';
let timer: ReturnType<typeof setTimeout> | null = null;
const pending = new Set<string>();

/**
 * Watch `indices-kb/state` (and the journal) and call `onChange` with the dates
 * whose files moved, debounced: the skill writes several files in a burst and
 * Python writes them through a temp file, so one change becomes many events.
 */
export function watchVault(vault: string, onChange: (dates: string[]) => void): void {
  const dir = vault ? statePath(vault) : '';
  if (dir === watchedDir && watcher) return;
  stopWatching();
  watchedDir = dir;
  if (!dir) return;
  try {
    watcher = watch(dir, { persistent: false }, (_ev, name) => {
      const d = name ? dateOfStateFile(String(name)) : null;
      if (!d) return;
      pending.add(d);
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        const dates = [...pending];
        pending.clear();
        onChange(dates);
      }, 500);
    });
    watcher.on('error', () => stopWatching());
  } catch {
    watcher = null;
  }
}

export function stopWatching(): void {
  if (timer) clearTimeout(timer);
  timer = null;
  pending.clear();
  try {
    watcher?.close();
  } catch {
    /* already closed */
  }
  watcher = null;
  watchedDir = '';
}
