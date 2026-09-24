// The app's own CODEX_HOME.
//
// Measured on 2026-09-23: an `app-server` started with the user's own
// `CODEX_HOME` loads their whole world — every MCP server in their
// `config.toml` (nine of them on this machine), their `~/.codex/AGENTS.md`,
// their `notify` hook, their personality and their memories. `-c` overrides do
// NOT undo that (`-c mcp_servers={}` left all nine running), and `app-server`
// has no `--ignore-user-config` — that flag exists only on `codex exec`.
//
// So the app gets a CODEX_HOME of its own under userData, the same way it owns
// its Python runtime. That is also why authentication is a separate, explicit
// step: a private home starts logged out (`getAuthStatus` → `authMethod: null`),
// and the app must never copy the user's credentials behind their back.
import { app } from 'electron';
import { promises as fs } from 'node:fs';
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

/** What the app writes into its own `config.toml`; per-thread settings are sent over the protocol. */
const CONFIG_TOML = [
  '# Written by Strategy Desk. The app keeps its own CODEX_HOME so that nothing',
  "# from the user's personal ~/.codex leaks into a chat: no MCP servers, no",
  '# AGENTS.md, no notify hook, no memories. Per-thread settings (model, effort,',
  '# sandbox, MCP servers, system prompt) are sent over the app-server protocol.',
  'notify = []',
  '',
  '[features]',
  'memories = false',
  '',
].join('\n');

export function codexHomeDir(): string {
  return join(app.getPath('userData'), 'codex-home');
}

/** The user's own Codex home, only ever read to tell them where their login lives. */
export function userCodexHome(): string {
  return process.env.CODEX_HOME || join(homedir(), '.codex');
}

export function authFile(home: string): string {
  return join(home, 'auth.json');
}

/** True when the app's own home holds a login. */
export function hasCodexAuth(home = codexHomeDir()): boolean {
  return existsSync(authFile(home));
}

/** Create the private home and keep its `config.toml` current. Idempotent. */
export async function ensureCodexHome(): Promise<string> {
  const dir = codexHomeDir();
  await fs.mkdir(dir, { recursive: true });
  const cfgPath = join(dir, 'config.toml');
  // Written once, never rewritten: `codex mcp add` appends its own
  // `[mcp_servers.…]` section to this file when the user registers the official
  // TradingView MCP, and rewriting the baseline would silently delete it.
  if (!existsSync(cfgPath)) await fs.writeFile(cfgPath, CONFIG_TOML, 'utf8');
  return dir;
}

/**
 * Copy the user's own login into the app's home. Only ever called from the
 * preflight's own button, never on a launch: it moves credentials, and the user
 * has to be the one who asks for that.
 */
export async function importCodexAuth(): Promise<{ ok: boolean; reason?: string }> {
  const source = authFile(userCodexHome());
  if (!existsSync(source)) return { ok: false, reason: 'no-user-auth' };
  const dir = await ensureCodexHome();
  await fs.copyFile(source, authFile(dir));
  return { ok: true };
}
