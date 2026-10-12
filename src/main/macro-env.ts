// The environment a macro session runs with — the pure half.
//
// The vault's own scripts (`indices-kb/tools/macro_review.ps1`, the Python
// tools behind the skill) honour a small contract: MACRO_VAULT, MACRO_PYTHON,
// MACRO_CLAUDE, TV_SESSIONID, TV_CDP_URL and PYTHONIOENCODING. Whoever opens a
// session — the desk's chat, a `-p` run from the Task Scheduler — must put the
// same six things in the environment, or the tools import the wrong Python or
// fetch nothing. This file is the ONE place that spells them, so the chat, the
// scheduler registration and the session check cannot drift apart.
//
// Electron-free on purpose: the chat driver, the tasks and vitest all call it.
import path from 'node:path';
import type { Settings } from '@shared/api';
import { DEFAULT_CDP_PORT } from '@shared/defaults';
import type { RuntimePaths } from './runtime-spec';

export interface MacroEnvInput {
  /** The vault folder (settings.vaultPath). */
  vault: string;
  /** Full path to the interpreter the vault tools use; '' = leave the tools to PATH. */
  python: string;
  /** Full path to the Claude Code binary; '' = leave it to PATH. */
  claude: string;
  /** The TradingView sessionid cookie; null = not stored. */
  sessionId: string | null;
  cdpPort: number;
  platform?: string;
}

/**
 * The variables only — no inheritance, so a test sees exactly what is added.
 * The caller spreads them over `process.env` (or over the chat's auth env).
 */
export function macroEnvVars(input: MacroEnvInput): Record<string, string> {
  const env: Record<string, string> = {
    MACRO_VAULT: input.vault,
    TV_CDP_URL: `http://127.0.0.1:${input.cdpPort || DEFAULT_CDP_PORT}`,
    PYTHONIOENCODING: 'utf-8',
  };
  if (input.python) env.MACRO_PYTHON = input.python;
  if (input.claude) env.MACRO_CLAUDE = input.claude;
  if (input.sessionId) env.TV_SESSIONID = input.sessionId;
  return env;
}

/**
 * The whole environment: the base, the six variables, and the venv's
 * `Scripts` (or `bin`) directory in front of PATH, so `python` and the
 * entry points the venv installed are the ones the tools find.
 */
export function macroEnv(input: MacroEnvInput, baseEnv: NodeJS.ProcessEnv): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(baseEnv)) if (typeof v === 'string') env[k] = v;
  Object.assign(env, macroEnvVars(input));
  if (input.python) {
    const platform = input.platform ?? process.platform;
    // Spelled per platform (not per host) so the shape is testable anywhere.
    const scripts = (platform === 'win32' ? path.win32 : path.posix).dirname(input.python);
    // Windows spells the variable `Path`; Node merges the two spellings when
    // spawning, but only the one that is set here is the one we prepend to.
    const key = Object.keys(env).find((k) => k.toLowerCase() === 'path') ?? (platform === 'win32' ? 'Path' : 'PATH');
    const current = env[key] ?? '';
    env[key] = current ? `${scripts}${platform === 'win32' ? ';' : ':'}${current}` : scripts;
  }
  return env;
}

/**
 * The interpreter the vault tools run on. Bundled mode: the venv the app
 * provisioned. External (a dev run): the checkout's own `.venv`, which is
 * where `uv sync` in a tvmcp clone puts it — and '' when there is none, so the
 * scripts fall back to whatever `python` PATH holds, as they always did.
 */
export function macroPython(cfg: Settings, paths: RuntimePaths, platform: string = process.platform): string {
  const P = platform === 'win32' ? path.win32 : path.posix;
  if (paths.venvDir) return platform === 'win32' ? P.join(paths.venvDir, 'Scripts', 'python.exe') : P.join(paths.venvDir, 'bin', 'python');
  const clone = (cfg.tvmcpPath || '').trim();
  if (!clone) return '';
  return platform === 'win32' ? P.join(clone, '.venv', 'Scripts', 'python.exe') : P.join(clone, '.venv', 'bin', 'python');
}

/** The sessionid cookie's shape: TradingView issues a 32-char alphanumeric id. */
export function looksLikeSessionId(value: string): boolean {
  return /^[A-Za-z0-9]{20,64}$/.test((value || '').trim());
}

/**
 * The one cheap proof that a sessionid works: exchange it for the websocket
 * token, exactly as tvmcp's `SessionClient` does before every bar fetch. One
 * HTTPS call, no bars, nothing written. The secret travels in the environment
 * (TV_SESSIONID), never on the command line where a process list would show it.
 */
export const SESSION_CHECK_CODE =
  'import os; from tvmcp.session.client import fetch_auth_token; fetch_auth_token(os.environ["TV_SESSIONID"]); print("sessionid ok: token issued")';

/** `python -c "import pandas, tvmcp"` — the two imports every vault tool starts with. */
export const TOOLS_IMPORT_CODE = 'import pandas, tvmcp; print("pandas", pandas.__version__, "tvmcp ok")';
