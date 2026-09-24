// The app's MCP map in the shape `codex app-server` wants.
//
// Verified 2026-09-23: MCP servers are NOT a thread parameter (`ThreadStartParams`
// has no `mcpServers`), but the free-form `config` object IS applied to the
// thread — a server passed there started for that thread and its tool was
// callable. That is what lets one app-server process serve chats with different
// tool sets without a restart.
import type { ServerMap, StdioServer } from '../../mcp';

export interface CodexStdioServer {
  command: string;
  args: string[];
  env: Record<string, string>;
}

export interface CodexHttpServer {
  url: string;
  bearer_token_env_var?: string;
}

export type CodexServerMap = Record<string, CodexStdioServer | CodexHttpServer>;

function isStdio(s: ServerMap[string]): s is StdioServer {
  return s.type === 'stdio';
}

export function toCodexServers(servers: ServerMap): CodexServerMap {
  const out: CodexServerMap = {};
  for (const [name, s] of Object.entries(servers)) {
    if (isStdio(s)) out[name] = { command: s.command, args: s.args, env: s.env };
    // An http server carries its own OAuth in Codex's store, exactly as the
    // official TradingView MCP does in Claude Code's — the app only names it.
    else out[name] = { url: s.url };
  }
  return out;
}

/** Names the model is allowed to call through: our own servers, nothing else. */
export function serverNames(servers: ServerMap): Set<string> {
  return new Set(Object.keys(servers));
}
