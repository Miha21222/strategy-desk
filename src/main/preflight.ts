// Preflight probes: the impure half of the diagnosis.
//
// `@shared/preflight` owns the contract and the pure facts -> checks mapping.
// This file only gathers the facts: shell-outs, HTTP to the CDP port, file
// stats. Every probe has a timeout and is wrapped, so a probe never throws into
// the renderer — a failed probe becomes a fact, not an exception.
//
// It reuses the existing probes (status.ts) rather than re-implementing them,
// and it caches nothing: `invalidateTvmcpCache()` runs first, because a user
// pressing "проверить заново" expects a real re-check.
//
// It never CHANGES anything: every probe here reads. Where something must be
// installed, provisioned or logged in to, the diagnosis hands back either
// copyable text (the user's own toolchain and apps) or a `run-task` fix (what
// the app itself owns: its bundled Python environment, the official MCP
// registration). Tasks live in `tasks.ts` and run only when a button is pressed.
import { execFile } from 'node:child_process';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Settings } from '@shared/api';
import { DEFAULT_CDP_PORT, DEFAULT_TVMCP_REPO } from '@shared/defaults';
import { emptyOfficialFact, OFFICIAL_MCP_NAME, parseMcpGet, type OfficialMcpFact } from '@shared/official-mcp';
import {
  buildChecks,
  firstLine,
  PORT_CANDIDATES,
  resultOf,
  SCHEDULER_TASKS,
  type CodexFact,
  type FileFact,
  type PreflightFacts,
  type PreflightResult,
  type SchedulerFact,
  type ToolsPythonFact,
  type VaultFact,
} from '@shared/preflight';
import { authFile, codexHomeDir, hasCodexAuth, userCodexHome } from './drivers/codex/home';
import { macroPython, TOOLS_IMPORT_CODE } from './macro-env';
import { claudeBin, initRuntimePaths, kbFact, runtimeFact, vaultTemplateDir } from './runtime';
import { runtimeEnv, type RuntimePaths } from './runtime-spec';
import { readTemplateVersion, VAULT_MARKERS } from './vault-template';
import { cdpProbe, claudeAuthStatus, invalidateTvmcpCache, isTradingViewRunning, tvmcpStatus } from './status';

function run(cmd: string, args: string[], timeout: number): Promise<{ ok: boolean; notFound: boolean; out: string }> {
  return new Promise((resolve) => {
    try {
      execFile(cmd, args, { timeout, windowsHide: true, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) => {
        resolve({
          ok: !err,
          notFound: Boolean(err && (err as NodeJS.ErrnoException).code === 'ENOENT'),
          out: `${stdout ?? ''}${stderr ?? ''}`.trim(),
        });
      });
    } catch {
      resolve({ ok: false, notFound: true, out: '' });
    }
  });
}

/**
 * Same as `run`, with extra environment and a shell on Windows.
 *
 * Both matter for Codex: the probes need `CODEX_HOME` pointed at the app's own
 * home, and `codex` installed from npm is a `.cmd` shim — `execFile` cannot
 * start one, so without the shell the probe reported "Codex CLI not found" on
 * a machine that has it (caught by the preflight smoke, 2026-09-23).
 */
function runWithEnv(cmd: string, args: string[], timeout: number, env: Record<string, string>): Promise<{ ok: boolean; notFound: boolean; out: string }> {
  return new Promise((resolve) => {
    try {
      execFile(cmd, args, { timeout, windowsHide: true, maxBuffer: 1024 * 1024, shell: process.platform === 'win32', env: { ...process.env, ...env } }, (err, stdout, stderr) => {
        resolve({
          ok: !err,
          notFound: Boolean(err && (err as NodeJS.ErrnoException).code === 'ENOENT'),
          out: `${stdout ?? ''}${stderr ?? ''}`.trim(),
        });
      });
    } catch {
      resolve({ ok: false, notFound: true, out: '' });
    }
  });
}

async function exists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

async function uvFact(): Promise<PreflightFacts['uv']> {
  const r = await run('uv', ['--version'], 10_000);
  const m = r.out.match(/uv\s+([0-9][^\s(]*)/i);
  return { found: r.ok && Boolean(m), version: m ? m[1] : '' };
}

/** The configured path must exist and hold a pyproject.toml whose project is `tvmcp`. */
async function cloneFact(dir: string, repo: string): Promise<PreflightFacts['clone']> {
  const fact: PreflightFacts['clone'] = { path: dir, repo, dirExists: false, pyproject: 'missing', projectName: '' };
  if (!dir.trim()) return fact;
  fact.dirExists = await exists(dir);
  if (!fact.dirExists) return fact;
  try {
    const raw = await fs.readFile(path.join(dir, 'pyproject.toml'), 'utf8');
    const m = raw.match(/^\s*name\s*=\s*["']([^"']+)["']/m);
    fact.projectName = m ? m[1] : '';
    fact.pyproject = fact.projectName === 'tvmcp' ? 'ok' : 'wrong-name';
  } catch {
    fact.pyproject = 'missing';
  }
  return fact;
}

/** The first candidate port nobody answers on, so "switch the port" is a real offer. */
async function findFreePort(current: number): Promise<number | null> {
  for (const port of PORT_CANDIDATES) {
    if (port === current) continue;
    if (!(await cdpProbe(port, 700)).up) return port;
  }
  return null;
}

async function workflowFacts(): Promise<PreflightFacts['workflows']> {
  try {
    // Dynamic import: workflows.ts reaches for electron's app path, and this
    // module must stay importable from a plain node test run.
    const { listWorkflows, workflowProblems } = await import('./workflows');
    return { loaded: (await listWorkflows()).map((w) => w.id), failed: workflowProblems() };
  } catch (e) {
    return { loaded: [], failed: [firstLine(String((e as Error)?.message ?? e))] };
  }
}

async function fileFact(p: string): Promise<FileFact> {
  const clean = (p || '').trim();
  return { path: clean, exists: clean ? await exists(clean) : false };
}

/**
 * Is the official TradingView MCP registered in Claude Code's own store, and is
 * it authenticated?
 *
 * Read-only: `claude mcp get` prints the registration and health-checks it. It
 * runs with the bundled Claude binary, because on a packaged install there may
 * be no `claude` on PATH and an ENOENT would otherwise read as "you have not
 * set it up" — a lie that would push the user into registering it twice.
 */
async function officialFact(cfg: Settings): Promise<OfficialMcpFact> {
  const enabled = Boolean(cfg.officialMcp);
  const r = await run(claudeBin(), ['mcp', 'get', OFFICIAL_MCP_NAME], 20_000);
  if (r.notFound) return { ...emptyOfficialFact(enabled), status: 'unknown' };
  return { ...parseMcpGet(r.out, r.ok), enabled };
}

/**
 * The Codex engine: is the CLI there, is the APP's own home logged in, and is
 * the official TradingView MCP registered in that home. `codex mcp get` runs
 * with `CODEX_HOME` pointed at the app's home on purpose — the answer for the
 * user's own home would be about a different store entirely.
 */
async function codexFact(cfg: Settings): Promise<CodexFact> {
  const home = codexHomeDir();
  const command = (cfg.codexPath || '').trim() || 'codex';
  const fact: CodexFact = {
    found: false,
    version: '',
    home,
    hasAuth: hasCodexAuth(home),
    userAuth: await exists(authFile(userCodexHome())),
    official: { ...emptyOfficialFact(Boolean(cfg.officialMcp)), status: 'unknown' },
  };
  const v = await runWithEnv(command, ['--version'], 15_000, { CODEX_HOME: home });
  const m = v.out.match(/([0-9]+\.[0-9]+\.[0-9]+)/);
  fact.found = v.ok && Boolean(m);
  fact.version = m ? m[1] : '';
  if (!fact.found) return fact;

  const get = await runWithEnv(command, ['mcp', 'get', OFFICIAL_MCP_NAME], 20_000, { CODEX_HOME: home });
  if (!get.notFound) fact.official = { ...parseMcpGet(get.out, get.ok), enabled: Boolean(cfg.officialMcp) };
  return fact;
}

/**
 * The vault folder: is it there, is anything in it, and does it hold the
 * three files that make it a vault. Reads only.
 */
async function vaultFact(dir: string): Promise<VaultFact> {
  const clean = (dir || '').trim();
  const fact: VaultFact = {
    path: clean,
    exists: false,
    nonEmpty: false,
    hasSkill: false,
    hasScaleTool: false,
    hasScaleConfig: false,
    templateVersion: '',
    templateAvailable: Boolean(await vaultTemplateDir()),
  };
  if (!clean) return fact;
  fact.exists = await exists(clean);
  if (!fact.exists) return fact;
  try {
    fact.nonEmpty = (await fs.readdir(clean)).length > 0;
  } catch {
    fact.nonEmpty = false;
  }
  const [skill, tool, config] = await Promise.all(VAULT_MARKERS.map((m) => exists(path.join(clean, ...m.split('/')))));
  fact.hasSkill = skill;
  fact.hasScaleTool = tool;
  fact.hasScaleConfig = config;
  fact.templateVersion = await readTemplateVersion(clean);
  return fact;
}

/**
 * The Python the vault tools run on, and whether `import pandas, tvmcp` passes
 * in it. The import is the one probe here that starts a process (~1 s); it
 * runs only when the interpreter is there, and with the runtime's cache
 * locations so a first import does not write into the installation directory.
 */
async function toolsPythonFact(cfg: Settings, paths: RuntimePaths): Promise<ToolsPythonFact> {
  const python = macroPython(cfg, paths);
  const fact: ToolsPythonFact = { python, exists: false, checked: false, importOk: false, error: '' };
  if (!python) return fact;
  fact.exists = await exists(python);
  if (!fact.exists) return fact;
  const r = await runWithEnv(python, ['-c', TOOLS_IMPORT_CODE], 30_000, { ...runtimeEnv(paths), PYTHONIOENCODING: 'utf-8' });
  fact.checked = true;
  fact.importOk = r.ok;
  fact.error = r.ok ? '' : r.out;
  return fact;
}

/**
 * The three Task Scheduler jobs, by name. `Get-ScheduledTask` is read-only and
 * needs no elevation for the current user's tasks; on a non-Windows machine
 * the question has no answer and the check says so.
 */
async function schedulerFact(vault: string): Promise<SchedulerFact> {
  const script = vault ? path.join(vault, 'indices-kb', 'tools', 'macro_schedule.ps1') : '';
  const fact: SchedulerFact = { queried: false, tasks: [], script: script && (await exists(script)) ? script : '' };
  if (process.platform !== 'win32') return fact;
  const names = SCHEDULER_TASKS.map((n) => `'${n}'`).join(',');
  // The action's arguments too: that is where the `-Claude` path lives, and a
  // 0.2.1 registration points it into app.asar (issue #12).
  const ps = `Get-ScheduledTask -TaskName ${names} -ErrorAction SilentlyContinue | ForEach-Object { $_.TaskName + '|' + $_.State + '|' + ($_.Actions | ForEach-Object { $_.Arguments }) }`;
  const r = await run('powershell', ['-NoProfile', '-NonInteractive', '-Command', ps], 20_000);
  if (r.notFound) return fact;
  fact.queried = true;
  for (const line of r.out.split(/\r?\n/)) {
    const [name, state, ...rest] = line.split('|').map((x) => x.trim());
    if (name && (SCHEDULER_TASKS as readonly string[]).includes(name)) fact.tasks.push({ name, state: state || '', arguments: rest.join('|') });
  }
  return fact;
}

/** The PC's zone as ICU names it (`Europe/Kyiv`; older ICU data says `Europe/Kiev`). */
function timezoneFact(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || '';
  } catch {
    return '';
  }
}

export async function gatherFacts(cfg: Settings): Promise<PreflightFacts> {
  const port = cfg.cdpPort || DEFAULT_CDP_PORT;
  const repo = (cfg.tvmcpRepo || DEFAULT_TVMCP_REPO).trim();
  invalidateTvmcpCache();
  // Cheap, and it must happen before the runtime fact: a fresh install has not
  // resolved its paths yet when the wizard opens on the very first frame.
  const paths = await initRuntimePaths();

  const [uv, clone, runtime, kb, claude, official, codex, running, cdp, workflows, riskRules, journal, vault, toolsPython, scheduler] = await Promise.all([
    uvFact(),
    cloneFact((cfg.tvmcpPath || '').trim(), repo),
    runtimeFact(cfg, paths),
    kbFact(cfg, paths),
    claudeAuthStatus(cfg),
    officialFact(cfg),
    codexFact(cfg),
    isTradingViewRunning(),
    cdpProbe(port),
    workflowFacts(),
    fileFact(cfg.riskRulesPath),
    fileFact(cfg.journalPath),
    vaultFact(cfg.vaultPath),
    toolsPythonFact(cfg, paths),
    schedulerFact((cfg.vaultPath || '').trim()),
  ]);

  // The boot probe is the expensive one (~2.5 s of Python) and is pointless
  // before there is anything to boot: a provisioned environment in bundled
  // mode, uv plus a real tvmcp checkout in external mode.
  const bootable = runtime.mode === 'bundled' ? runtime.provisioned || runtime.stale : uv.found && clone.pyproject === 'ok';
  const boot = bootable ? { ran: true, ...(await tvmcpStatus(cfg)) } : { ran: false, ok: false, tools: 0, error: '' };
  const freePort = cdp.up && !cdp.tradingview ? await findFreePort(port) : null;

  return {
    uv,
    clone,
    runtime,
    kb,
    boot,
    claude,
    official,
    codex,
    tv: { running, port, cdp },
    freePort,
    workflows,
    files: { riskRules, journal },
    platform: process.platform,
    vault,
    toolsPython,
    tvSession: { set: Boolean(cfg.hasTvSession), verifiedAt: cfg.tvSessionVerifiedAt || 0 },
    scheduler,
    timezone: timezoneFact(),
  };
}

/** The whole diagnosis: probe everything, map it, never throw. */
export async function runPreflight(cfg: Settings): Promise<PreflightResult> {
  try {
    return resultOf(buildChecks(cfg, await gatherFacts(cfg)));
  } catch (e) {
    // A probe that still managed to blow up must not take the renderer with it.
    return resultOf([{ id: 'uv', status: 'unknown', detailKey: 'preflight.crashed', vars: { error: firstLine(String((e as Error)?.message ?? e)) } }]);
  }
}
