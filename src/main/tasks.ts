// The things the app may do FOR the user, each behind one press: provision the
// Python runtime, fetch the headless browser, seed the course knowledge base,
// download its embedding model, register and authenticate the official MCP.
//
// Why a runner at all instead of a handler each: they are all slow (a first
// `uv sync` pulls ~600 MB, a seed copies ~300 MB), they can all fail halfway,
// and a progress bar with no output is indistinguishable from a hang. So they
// share one shape — start, a stream of lines, an end with a verdict — and the
// renderer renders that shape once. One of them copies instead of spawning;
// that changes how it runs, not what the renderer is allowed to ask for.
//
// Two rules, both structural rather than advisory:
//   * THE RENDERER NAMES A TASK ID, NEVER A COMMAND. Every command is built
//     here, from `@shared/tasks`'s closed list. Same discipline as `tv:call`;
//   * nothing runs at launch. A task starts from a button in the preflight, and
//     the UI shows the command before running it.
import { app } from 'electron';
import { spawn, type ChildProcess } from 'node:child_process';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Settings } from '@shared/api';
import { OFFICIAL_MCP_NAME, OFFICIAL_MCP_URL } from '@shared/official-mcp';
import { applyTaskEvent, emptyTask, splitLines, TASK_IDS, type TaskEvent, type TaskId, type TaskState } from '@shared/tasks';
import { chromiumSpec, claudeBin, initRuntimePaths, kbSeedPlan, provisionSpec, runtimeRoot, writeMarker, type RuntimePaths } from './runtime';
import { kbModelSpec, modeOf, type SpawnSpec } from './runtime-spec';
import { invalidateTvmcpCache } from './status';

/** Generous: a cold `uv sync` downloads a Python and ~290 packages. */
const TIMEOUTS: Record<TaskId, number> = {
  'provision-runtime': 45 * 60_000,
  'install-chromium': 30 * 60_000,
  // A local copy of ~55 MB, plus ~240 MB of embedding model when one shipped.
  'kb-import': 15 * 60_000,
  'kb-model': 30 * 60_000,
  'official-mcp-add': 60_000,
  // Opens a browser and waits for the redirect; the user may take a while.
  'official-mcp-login': 10 * 60_000,
};

interface Plan {
  spec: SpawnSpec;
  env: Record<string, string>;
  cwd: string;
  /** Runs after a zero exit; a throw here fails the task. */
  after?: () => Promise<void>;
  /**
   * A task that copies rather than spawns (seeding the knowledge base). The
   * rule the runner keeps is that the RENDERER NAMES AN ID — not that every
   * task is a child process. A copy is planned here like everything else,
   * displayed before it runs and reported through the same events.
   */
  copies?: { from: string; to: string }[];
}

type Emit = (ev: TaskEvent) => void;

const states = new Map<TaskId, TaskState>();
const running = new Map<TaskId, { child: ChildProcess; done: Promise<TaskState> }>();

export function taskState(id: TaskId): TaskState {
  return states.get(id) ?? emptyTask(id);
}

export function allTaskStates(): TaskState[] {
  return TASK_IDS.map((id) => taskState(id));
}

/** True while any task is in flight — the preflight uses it to avoid probing mid-install. */
export function anyTaskRunning(): boolean {
  return running.size > 0;
}

/** Awaits a task already in flight, so a caller never races a half-built venv. */
export function taskInFlight(id: TaskId): Promise<TaskState> | null {
  return running.get(id)?.done ?? null;
}

export function stopTask(id: TaskId): void {
  running.get(id)?.child.kill();
}

/** What a task will run, as one line — shown in the UI before the press. */
export async function taskCommand(cfg: Settings, id: TaskId): Promise<string> {
  const plan = await planFor(cfg, id);
  if (!plan) return '';
  if (plan.copies) return plan.copies.map((c) => `copy "${c.from}" -> "${c.to}"`).join('; ');
  return [plan.spec.command, ...plan.spec.args].join(' ');
}

async function planFor(cfg: Settings, id: TaskId): Promise<Plan | null> {
  const paths = await initRuntimePaths();
  const cwd = app.getPath('userData');

  if (id === 'provision-runtime' || id === 'install-chromium') {
    if (modeOf(paths) !== 'bundled') return null;
    await ensureDirs(paths);
    if (id === 'provision-runtime') {
      const { spec, env } = provisionSpec(paths);
      return {
        spec,
        env,
        cwd,
        after: async () => {
          // The marker is what `runtimeFact` reads; writing it only after a
          // clean exit is what keeps a half-finished sync from looking done.
          await writeMarker(paths, spec.command);
          invalidateTvmcpCache();
        },
      };
    }
    const { spec, env } = chromiumSpec(cfg, paths);
    return { spec, env, cwd };
  }

  // The knowledge base: a copy out of the installation directory, and — only
  // when the installer carried no model — a download of the embedding model.
  if (id === 'kb-import') {
    const plan = kbSeedPlan(paths);
    if (!plan) return null;
    await fs.mkdir(paths.kbDataDir, { recursive: true });
    return { spec: { command: '', args: [] }, env: {}, cwd, copies: plan.copies, after: plan.after };
  }
  if (id === 'kb-model') {
    const model = kbModelSpec(cfg, paths);
    if (!model) return null;
    await fs.mkdir(paths.kbModelDir, { recursive: true });
    return { spec: model.spec, env: model.env, cwd };
  }

  // The official MCP lives in Claude Code's store, not ours. `-s user` on
  // purpose: the default scope is `local`, which is private to one working
  // directory, and the app's own directory is not where the user works.
  const claude = claudeBin();
  if (id === 'official-mcp-add') {
    return {
      spec: { command: claude, args: ['mcp', 'add', '-s', 'user', '-t', 'http', OFFICIAL_MCP_NAME, OFFICIAL_MCP_URL] },
      env: {},
      cwd,
    };
  }
  return { spec: { command: claude, args: ['mcp', 'login', OFFICIAL_MCP_NAME] }, env: {}, cwd };
}

async function ensureDirs(paths: RuntimePaths): Promise<void> {
  const dirs = [runtimeRoot(), paths.cacheDir, paths.pythonInstallDir, paths.numbaCacheDir, paths.pycacheDir, paths.browsersDir].filter(Boolean);
  for (const d of dirs) await fs.mkdir(d, { recursive: true });
}

/**
 * Run one task. Never throws: a refusal and a crash both come back as a
 * finished state with `ok: false`, because the renderer has a place to show
 * that and nowhere to show an exception.
 */
export async function runTask(cfg: Settings, id: TaskId, emit: Emit): Promise<TaskState> {
  const already = running.get(id);
  if (already) return already.done;

  const plan = await planFor(cfg, id);
  if (!plan) return finish(id, emit, false, 'this task needs the bundled runtime, and this is a development run');
  if (plan.copies) return runCopyTask(id, plan, emit);
  if (!plan.spec.command) return finish(id, emit, false, 'nothing to run: the runtime paths are not resolved');

  const command = [plan.spec.command, ...plan.spec.args].join(' ');
  let state = applyTaskEvent(taskState(id), { id, kind: 'start', at: Date.now(), command });
  states.set(id, state);
  emit({ id, kind: 'start', at: state.startedAt, command });

  const child = spawn(plan.spec.command, plan.spec.args, {
    cwd: plan.cwd,
    env: { ...process.env, ...plan.env },
    windowsHide: true,
    // stdin closed: none of these four may wait on a prompt we cannot answer.
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  const done = new Promise<TaskState>((resolve) => {
    const timer = setTimeout(() => child.kill(), TIMEOUTS[id]);

    const push = (text: string) => {
      state = applyTaskEvent(state, { id, kind: 'line', text });
      states.set(id, state);
      emit({ id, kind: 'line', text });
    };

    // A chunk is not a line: `uv sync` prints one package per line and the pipe
    // splits wherever it likes, so feeding chunks straight to splitLines shows
    // "websockets==17.0." and "1" as two entries. Each stream keeps its own
    // remainder until a newline arrives, and whatever is left is flushed at the
    // end — the last line of a failure is usually the whole diagnosis.
    const reader = (): ((chunk: Buffer | string) => void) & { flush: () => void } => {
      let rest = '';
      const on = (chunk: Buffer | string) => {
        const text = rest + String(chunk);
        const cut = text.lastIndexOf('\n');
        if (cut < 0) {
          rest = text;
          return;
        }
        rest = text.slice(cut + 1);
        for (const line of splitLines(text.slice(0, cut))) push(line);
      };
      on.flush = () => {
        for (const line of splitLines(rest)) push(line);
        rest = '';
      };
      return on;
    };
    const onOut = reader();
    const onErr = reader();
    child.stdout?.on('data', onOut);
    child.stderr?.on('data', onErr);

    const settle = async (ok: boolean, error: string) => {
      clearTimeout(timer);
      onOut.flush();
      onErr.flush();
      running.delete(id);
      let finalOk = ok;
      let finalError = error;
      if (ok && plan.after) {
        try {
          await plan.after();
        } catch (e) {
          finalOk = false;
          finalError = String((e as Error)?.message ?? e).slice(0, 400);
        }
      }
      resolve(finish(id, emit, finalOk, finalError, state));
    };

    child.on('error', (e) => void settle(false, String(e?.message ?? e).slice(0, 400)));
    child.on('close', (code, signal) => {
      if (signal) void settle(false, `stopped (${signal})`);
      else void settle(code === 0, code === 0 ? '' : `exit code ${code}`);
    });
  });

  running.set(id, { child, done });
  return done;
}

/**
 * The copy-shaped task (seeding the knowledge base).
 *
 * `fs.cp` with `force` overwrites what is there: a re-seed after an app update
 * must replace a store built from the previous payload, and a half-copied store
 * from an interrupted run is exactly the thing that has to be overwritten. The
 * marker is written by `after`, only on success, so an interruption leaves the
 * check saying "not seeded" rather than a lie.
 */
async function runCopyTask(id: TaskId, plan: Plan, emit: Emit): Promise<TaskState> {
  const copies = plan.copies ?? [];
  const command = copies.map((c) => `copy "${c.from}" -> "${c.to}"`).join('; ');
  let state = applyTaskEvent(taskState(id), { id, kind: 'start', at: Date.now(), command });
  states.set(id, state);
  emit({ id, kind: 'start', at: state.startedAt, command });

  const push = (text: string) => {
    state = applyTaskEvent(state, { id, kind: 'line', text });
    states.set(id, state);
    emit({ id, kind: 'line', text });
  };

  try {
    for (const c of copies) {
      push(`${c.from} -> ${c.to}`);
      // Remove first rather than merge: `cp` overwrites files but keeps the ones
      // the new store no longer has, and chroma names its segment directories
      // after UUIDs — a re-seed after an app update would otherwise leave the
      // previous store's segments beside the current one forever.
      await fs.rm(c.to, { recursive: true, force: true });
      await fs.cp(c.from, c.to, { recursive: true, force: true });
      push(`copied ${await dirSize(c.to)} MB`);
    }
    if (plan.after) await plan.after();
  } catch (e) {
    return finish(id, emit, false, String((e as Error)?.message ?? e).slice(0, 400), state);
  }
  return finish(id, emit, true, '', state);
}

/** Rounded megabytes of a directory tree — for the one line the copy reports. */
async function dirSize(dir: string): Promise<number> {
  let total = 0;
  const walk = async (d: string): Promise<void> => {
    let entries;
    try {
      entries = await fs.readdir(d, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const p = path.join(d, entry.name);
      if (entry.isDirectory()) await walk(p);
      else
        try {
          total += (await fs.stat(p)).size;
        } catch {
          /* a file that vanished between readdir and stat is not worth failing a copy over */
        }
    }
  };
  await walk(dir);
  return Math.round(total / 1048576);
}

function finish(id: TaskId, emit: Emit, ok: boolean, error: string, from?: TaskState): TaskState {
  const at = Date.now();
  const next = applyTaskEvent(from ?? taskState(id), { id, kind: 'end', at, ok, error });
  states.set(id, next);
  emit({ id, kind: 'end', at, ok, error });
  return next;
}
