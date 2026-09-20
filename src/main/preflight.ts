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
// It never runs a network or destructive command by itself. `git clone`, the uv
// installer, `claude` login and "close TradingView" are handed back as copyable
// text; the user presses the button.
import { execFile } from 'node:child_process';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Settings } from '@shared/api';
import { DEFAULT_CDP_PORT, DEFAULT_TVMCP_REPO } from '@shared/defaults';
import { buildChecks, firstLine, PORT_CANDIDATES, resultOf, type FileFact, type PreflightFacts, type PreflightResult } from '@shared/preflight';
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

export async function gatherFacts(cfg: Settings): Promise<PreflightFacts> {
  const port = cfg.cdpPort || DEFAULT_CDP_PORT;
  const repo = (cfg.tvmcpRepo || DEFAULT_TVMCP_REPO).trim();
  invalidateTvmcpCache();

  const [uv, clone, claude, running, cdp, workflows, riskRules, journal] = await Promise.all([
    uvFact(),
    cloneFact((cfg.tvmcpPath || '').trim(), repo),
    claudeAuthStatus(cfg),
    isTradingViewRunning(),
    cdpProbe(port),
    workflowFacts(),
    fileFact(cfg.riskRulesPath),
    fileFact(cfg.journalPath),
  ]);

  // The boot probe is the expensive one (~2.5 s of Python) and is pointless
  // when uv is missing or the clone is not tvmcp.
  const boot = uv.found && clone.pyproject === 'ok' ? { ran: true, ...(await tvmcpStatus(cfg)) } : { ran: false, ok: false, tools: 0, error: '' };
  const freePort = cdp.up && !cdp.tradingview ? await findFreePort(port) : null;

  return { uv, clone, boot, claude, tv: { running, port, cdp }, freePort, workflows, files: { riskRules, journal }, platform: process.platform };
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
