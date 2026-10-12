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
import { chromiumSpec, claudeBin, initRuntimePaths, kbSeedPlan, provisionSpec, runtimeRoot, vaultTemplateDir, writeMarker, type RuntimePaths } from './runtime';
import { kbModelSpec, modeOf, runtimeEnv, type SpawnSpec } from './runtime-spec';
import { authFile, ensureCodexHome, userCodexHome } from './drivers/codex/home';
import { macroEnv, macroPython, SESSION_CHECK_CODE } from './macro-env';
import { invalidateTvmcpCache } from './status';
import * as store from './store';
import { copyVaultTemplate } from './vault-template';

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
  // One 4 KB file.
  'codex-auth-import': 30_000,
  'codex-mcp-add': 60_000,
  'codex-mcp-login': 10 * 60_000,
  // A few hundred small files from the installation directory.
  'vault-create': 2 * 60_000,
  // One HTTPS round trip plus a Python import.
  'session-check': 60_000,
  // Three Register-ScheduledTask calls.
  'scheduler-register': 2 * 60_000,
  'scheduler-unregister': 60_000,
  // Opens a browser and waits for the redirect, like the MCP logins.
  'claude-login': 10 * 60_000,
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
  /**
   * A task that neither spawns nor copies whole directories: laying the vault
   * template down file by file, skipping what the user already has. Same
   * rule as `copies` — planned here, shown before it runs, reported through
   * the same events; `work` gets the line sink and throws to fail.
   */
  work?: (push: (line: string) => void) => Promise<void>;
  /** The one line the UI shows for a `work` task before the press. */
  describe?: string;
  /**
   * Keep stdin open: the task asks the user for one line (the OAuth code the
   * login page shows), and `writeTaskInput` delivers it. Everything else runs
   * with stdin closed so nothing can wait on a prompt nobody sees.
   */
  stdin?: boolean;
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

/** One line into a running task that was planned with `stdin: true`; anything else is ignored. */
export function writeTaskInput(id: TaskId, text: string): void {
  const child = running.get(id)?.child;
  if (!child?.stdin || child.stdin.destroyed) return;
  child.stdin.write(text.trim() + '\n');
}

/** What a task will run, as one line — shown in the UI before the press. */
export async function taskCommand(cfg: Settings, id: TaskId): Promise<string> {
  const plan = await planFor(cfg, id);
  if (!plan) return '';
  if (plan.copies) return plan.copies.map((c) => `copy "${c.from}" -> "${c.to}"`).join('; ');
  if (plan.work) return plan.describe ?? '';
  return [plan.spec.command, ...plan.spec.args].join(' ');
}

/**
 * The macro-review cycle on this PC. The vault's own PowerShell scripts do the
 * registering (`macro_schedule.ps1`, idempotent: same names are replaced); the
 * app only hands them the paths — vault, the provisioned Python, the bundled
 * Claude binary — and the environment the contract names (`macro-env.ts`).
 *
 * The sessionid also goes into the USER's environment variables (HKCU, as
 * `[Environment]::SetEnvironmentVariable(…, 'User')`): a scheduled run starts
 * from the Task Scheduler with the user's environment block, not from the
 * desk, and the Python tools read `TV_SESSIONID` from nowhere else. That is
 * the one place a secret of ours is stored outside safeStorage; the wizard
 * says so before the press, and `scheduler-unregister` removes it again.
 */
async function macroPlanFor(cfg: Settings, id: 'vault-create' | 'session-check' | 'scheduler-register' | 'scheduler-unregister', paths: RuntimePaths, cwd: string): Promise<Plan | null> {
  const vault = (cfg.vaultPath || '').trim();
  if (id === 'vault-create') {
    const template = await vaultTemplateDir();
    if (!template || !vault) return null;
    return {
      spec: { command: '', args: [] },
      env: {},
      cwd,
      describe: `copy the vault template "${template}" -> "${vault}" (existing files are kept)`,
      work: async (push) => {
        const r = await copyVaultTemplate(template, vault, push);
        push(`${r.mode === 'fresh' ? 'created' : 'completed'} the vault at ${vault}`);
      },
    };
  }

  const python = macroPython(cfg, paths);
  const pythonExists = Boolean(python) && (await exists(python));
  const claude = claudeBin();
  const claudeExists = claude !== 'claude' && (await exists(claude));
  const secrets = await store.getSecrets();
  const env = macroEnv({ vault, python: pythonExists ? python : '', claude: claudeExists ? claude : '', sessionId: secrets.tvSession, cdpPort: cfg.cdpPort }, { ...process.env, ...runtimeEnv(paths) });

  if (id === 'session-check') {
    if (!pythonExists) return null;
    if (!secrets.tvSession) return null;
    return {
      spec: { command: python, args: ['-c', SESSION_CHECK_CODE] },
      env,
      cwd,
      after: async () => {
        await store.setSettings({ tvSessionVerifiedAt: Date.now() });
      },
    };
  }

  const script = vault ? path.join(vault, 'indices-kb', 'tools', 'macro_schedule.ps1') : '';
  if (!script || !(await exists(script))) return null;
  const q = (v: string) => `'${v.replace(/'/g, "''")}'`;
  // The user-level variables the scheduled runs need. The cookie is NOT
  // spelled in the command: the renderer shows this line before the press,
  // so PowerShell copies it from its own environment (`$env:TV_SESSIONID`,
  // set by `macroEnv` above) instead.
  const userVars: Record<string, string | null> = {
    MACRO_VAULT: vault,
    MACRO_PYTHON: pythonExists ? python : null,
    MACRO_CLAUDE: claudeExists ? claude : null,
    TV_CDP_URL: env.TV_CDP_URL,
    PYTHONIOENCODING: 'utf-8',
    TV_SESSIONID: secrets.tvSession ? '$env:TV_SESSIONID' : null,
  };
  if (id === 'scheduler-register') {
    const sets = Object.entries(userVars)
      .filter((kv): kv is [string, string] => Boolean(kv[1]))
      .map(([k, v]) => `[Environment]::SetEnvironmentVariable(${q(k)}, ${v.startsWith('$env:') ? v : q(v)}, 'User')`);
    const args = [`-Vault ${q(vault)}`, pythonExists ? `-Python ${q(python)}` : '', claudeExists ? `-Claude ${q(claude)}` : ''].filter(Boolean).join(' ');
    const ps = [...sets, `& ${q(script)} ${args}`].join('; ');
    return { spec: { command: 'powershell', args: ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', ps] }, env, cwd: path.dirname(script) };
  }
  const clears = Object.keys(userVars).map((k) => `[Environment]::SetEnvironmentVariable(${q(k)}, $null, 'User')`);
  const ps = [`& ${q(script)} -Unregister -Vault ${q(vault)}`, ...clears].join('; ');
  return { spec: { command: 'powershell', args: ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-Command', ps] }, env, cwd: path.dirname(script) };
}

async function exists(p: string): Promise<boolean> {
  if (!p) return false;
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function planFor(cfg: Settings, id: TaskId): Promise<Plan | null> {
  const paths = await initRuntimePaths();
  const cwd = app.getPath('userData');

  if (id === 'vault-create' || id === 'session-check' || id === 'scheduler-register' || id === 'scheduler-unregister') return macroPlanFor(cfg, id, paths, cwd);

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

  // The Codex engine runs out of the app's own CODEX_HOME (see
  // `drivers/codex/home.ts`), so both its login and its MCP registrations live
  // there — the user's ~/.codex is never written to.
  if (id === 'codex-auth-import' || id === 'codex-mcp-add' || id === 'codex-mcp-login') {
    const home = await ensureCodexHome();
    const codex = (cfg.codexPath || '').trim() || 'codex';
    const env = { CODEX_HOME: home };
    if (id === 'codex-auth-import') {
      // A copy, like the knowledge base seed: planned here, shown before it
      // runs, and only ever started by the user pressing the button — it moves
      // credentials, which the app must never do on its own.
      return { spec: { command: '', args: [] }, env, cwd, copies: [{ from: authFile(userCodexHome()), to: authFile(home) }] };
    }
    if (id === 'codex-mcp-add') {
      return { spec: { command: codex, args: ['mcp', 'add', OFFICIAL_MCP_NAME, '--url', OFFICIAL_MCP_URL] }, env, cwd };
    }
    return { spec: { command: codex, args: ['mcp', 'login', OFFICIAL_MCP_NAME] }, env, cwd };
  }

  // The official MCP lives in Claude Code's store, not ours. `-s user` on
  // purpose: the default scope is `local`, which is private to one working
  // directory, and the app's own directory is not where the user works.
  const claude = claudeBin();
  if (id === 'claude-login') {
    // The SDK's own binary, so a machine with no `claude` on PATH logs in all
    // the same. Verified 2026-10-12 with stdin closed: it opens the browser and
    // then waits on «Paste code here if prompted >» — so stdin stays open and
    // the wizard's field delivers the code (`writeTaskInput`).
    return { spec: { command: claude, args: ['auth', 'login', '--claudeai'] }, env: {}, cwd, stdin: true };
  }
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
  if (!plan) return finish(id, emit, false, REFUSALS[id] ?? 'this task needs the bundled runtime, and this is a development run');
  if (plan.copies) return runCopyTask(id, plan, emit);
  if (plan.work) return runWorkTask(id, plan, emit);
  if (!plan.spec.command) return finish(id, emit, false, 'nothing to run: the runtime paths are not resolved');

  const command = [plan.spec.command, ...plan.spec.args].join(' ');
  let state = applyTaskEvent(taskState(id), { id, kind: 'start', at: Date.now(), command });
  states.set(id, state);
  emit({ id, kind: 'start', at: state.startedAt, command });

  const child = spawn(plan.spec.command, plan.spec.args, {
    cwd: plan.cwd,
    env: { ...process.env, ...plan.env },
    windowsHide: true,
    // stdin closed unless the plan asks for it: nothing may wait on a prompt
    // nobody sees. The login is the one task with a line to deliver.
    stdio: [plan.stdin ? 'pipe' : 'ignore', 'pipe', 'pipe'],
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

/** Why a task had no plan, per id — the renderer shows this as the failure. */
const REFUSALS: Partial<Record<TaskId, string>> = {
  'vault-create': 'no vault template shipped with this build, or no vault path is set',
  'session-check': 'needs the provisioned Python and a stored sessionid',
  'scheduler-register': 'the vault has no indices-kb/tools/macro_schedule.ps1 — create the vault first',
  'scheduler-unregister': 'the vault has no indices-kb/tools/macro_schedule.ps1',
};

/** The task that does its own work in-process (laying the vault template down). */
async function runWorkTask(id: TaskId, plan: Plan, emit: Emit): Promise<TaskState> {
  const command = plan.describe ?? '';
  let state = applyTaskEvent(taskState(id), { id, kind: 'start', at: Date.now(), command });
  states.set(id, state);
  emit({ id, kind: 'start', at: state.startedAt, command });
  const push = (text: string) => {
    state = applyTaskEvent(state, { id, kind: 'line', text });
    states.set(id, state);
    emit({ id, kind: 'line', text });
  };
  try {
    await plan.work?.(push);
    if (plan.after) await plan.after();
  } catch (e) {
    return finish(id, emit, false, String((e as Error)?.message ?? e).slice(0, 400), state);
  }
  return finish(id, emit, true, '', state);
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
