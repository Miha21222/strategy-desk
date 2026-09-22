// Deterministic tvmcp calls without a model turn.
//
// Why this exists: MCP tools are called by the model, not by the app. "The app
// watches the session" must therefore NOT mean "spawn a model turn every
// fifteen seconds" — that burns the user's plan to learn that nothing has
// happened. The tvmcp server ships a CLI where every registered tool is a
// subcommand emitting JSON on stdout:
//
//   <python> -m tvmcp.cli <tool_name> '<json args>'
//
// (the `tv` console script, reached as a module so one spawn shape covers both
// the bundled interpreter and `uv run` on a developer's checkout — see
// runtime-spec.ts), so the app polls through that, with the same environment
// the MCP server gets, and spends a model turn only when there is something
// worth interpreting.
//
// Measured on this machine: ~2.5 s per call, all of it Python + fastmcp import,
// the same for every tool. That is fine against `next_poll_after_s` (floor 5 s)
// but means two things the callers must respect and this module enforces:
//   * a generous timeout (a `provider: session` bar load is slower than a
//     status read), and a timeout is a warning, not a crash;
//   * no pile-up — a small semaphore caps concurrent spawns, and the panel's
//     poll loop is chained (it schedules the next tick only after the previous
//     call resolved), so one run never has two polls in flight.
import { execFile } from 'node:child_process';
import type { Settings } from '@shared/api';
import { isAllowedTvTool, type TvCallResult } from '@shared/panel';
import { buildTvEnv } from './mcp';
import { modeOf, runtimePaths, tvmcpSpawn } from './runtime-spec';

/** Plenty of headroom over the ~2.5 s floor; a session-feed load is slower. */
export const DEFAULT_TV_TIMEOUT_MS = 30_000;
/** Level scans and replays walk a lot of bars. */
export const LONG_TV_TIMEOUT_MS = 120_000;
const MAX_CONCURRENT = 3;
const MAX_STDOUT = 16 * 1024 * 1024;

const LONG_TOOLS = new Set(['tv_scan_levels', 'tv_sentinel_start', 'tv_sentinel_poll']);

let inFlight = 0;
const waiting: (() => void)[] = [];

async function acquire(): Promise<void> {
  if (inFlight < MAX_CONCURRENT) {
    inFlight += 1;
    return;
  }
  await new Promise<void>((resolve) => waiting.push(resolve));
  inFlight += 1;
}

function release(): void {
  inFlight = Math.max(0, inFlight - 1);
  const next = waiting.shift();
  if (next) next();
}

function tidyStderr(raw: string): string[] {
  return raw
    .split('\n')
    .map((l) => l.trim())
    .filter(Boolean)
    .slice(-6)
    .map((l) => l.slice(0, 400));
}

export interface TvCallOptions {
  timeoutMs?: number;
  env?: NodeJS.ProcessEnv;
}

/**
 * Run one tvmcp tool through the CLI and parse its JSON.
 *
 * Never throws: a missing `uv`, a bad spec, a dead feed and a timeout all come
 * back as `{ ok: false, error }` so the renderer can render "не проверено"
 * instead of losing the panel.
 */
export async function callTool(
  cfg: Settings,
  name: string,
  args: Record<string, unknown> = {},
  opts: TvCallOptions = {},
): Promise<TvCallResult> {
  const started = Date.now();
  if (!isAllowedTvTool(name)) {
    return { ok: false, error: `tool ${String(name).slice(0, 60)} is not in the allowlist`, warnings: [], ms: 0 };
  }
  const paths = runtimePaths();
  const bundled = modeOf(paths) === 'bundled';
  if (!bundled && !(cfg.tvmcpPath || '').trim()) {
    return { ok: false, error: 'tvmcp path is not set in the settings', warnings: [], ms: 0 };
  }
  const spec = tvmcpSpawn(cfg, paths, 'cli');

  const timeout = opts.timeoutMs ?? (LONG_TOOLS.has(name) ? LONG_TV_TIMEOUT_MS : DEFAULT_TV_TIMEOUT_MS);
  const env = buildTvEnv(cfg, opts.env ?? process.env);
  let payload: string;
  try {
    payload = JSON.stringify(args ?? {});
  } catch {
    return { ok: false, error: 'arguments are not JSON-serialisable', warnings: [], ms: 0 };
  }

  await acquire();
  try {
    return await new Promise<TvCallResult>((resolve) => {
      execFile(
        spec.command,
        [...spec.args, name, payload],
        { env, timeout, windowsHide: true, maxBuffer: MAX_STDOUT },
        (err, stdout, stderr) => {
          const ms = Date.now() - started;
          const warnings = tidyStderr(String(stderr ?? ''));
          const out = String(stdout ?? '').trim();
          if (err) {
            const e = err as NodeJS.ErrnoException & { killed?: boolean; signal?: string };
            let error: string;
            if (e.code === 'ENOENT') {
              error = bundled
                ? 'the bundled runtime is not provisioned yet — open «Мастер настройки» and press «Развернуть рантайм»'
                : 'uv not found — install uv (astral.sh/uv) and restart the app';
            } else if (e.killed || e.signal === 'SIGTERM') error = `${name} timed out after ${Math.round(timeout / 1000)} s`;
            else error = warnings.join(' | ') || e.message || `${name} failed`;
            resolve({ ok: false, error: error.slice(0, 600), warnings, ms });
            return;
          }
          if (!out) {
            resolve({ ok: false, error: `${name} returned nothing on stdout`, warnings, ms });
            return;
          }
          try {
            resolve({ ok: true, data: JSON.parse(out), warnings, ms });
          } catch {
            resolve({ ok: false, error: `${name} did not return JSON: ${out.slice(0, 200)}`, warnings, ms });
          }
        },
      );
    });
  } finally {
    release();
  }
}
