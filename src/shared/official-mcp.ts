// The official TradingView MCP server (mcp.tradingview.com), as Claude Code's
// own store sees it.
//
// Why the store matters at all when Strategy Desk passes `mcpServers`
// programmatically and runs with `strictMcpConfig: true` and
// `settingSources: []`: the server list is ours, but the OAuth tokens are not.
// Verified on 2026-09-22 — a dynamic HTTP server passed on the command line
// comes up `{"name":"mcp-tradingview","status":"connected","source":"dynamic"}`
// only because `claude` found credentials for it in its own store. So the app
// must be able to see whether that registration exists, and offer to create it.
export const OFFICIAL_MCP_NAME = 'mcp-tradingview';
export const OFFICIAL_MCP_URL = 'https://mcp.tradingview.com/mcp';

export type OfficialMcpStatus = 'connected' | 'needs-auth' | 'error' | 'absent' | 'unknown';

export interface OfficialMcpFact {
  /** `settings.officialMcp` — whether the chat is told to use it at all. */
  enabled: boolean;
  registered: boolean;
  /** 'user' / 'local' / 'project' as claude prints it; informational. */
  scope: string;
  status: OfficialMcpStatus;
  url: string;
  /** The status line verbatim, so an unseen wording still reaches the user. */
  raw: string;
}

export function emptyOfficialFact(enabled = false): OfficialMcpFact {
  return { enabled, registered: false, scope: '', status: 'absent', url: '', raw: '' };
}

/**
 * Parse `claude mcp get <name>`.
 *
 * Observed good output (2026-09-22, claude 2.1.278):
 *
 *   mcp-tradingview:
 *     Scope: Local config (private to you in this project)
 *     Status: ✔ Connected
 *     Type: http
 *     URL: https://mcp.tradingview.com/mcp
 *
 * and, for a name it does not know, a single line starting `No MCP server
 * named "..."`. Anything else is read defensively: a `Status:` line that does
 * not say "connected" is treated as "needs authentication" and carries its own
 * text into the UI, because the exact wording of the bad cases was never
 * observed here and must not be guessed at.
 */
export function parseMcpGet(out: string, exitOk: boolean): OfficialMcpFact {
  const text = String(out ?? '');
  const fact = emptyOfficialFact();
  if (/no mcp server named/i.test(text)) return fact;

  // A plain line scan, not a built regex: the labels are fixed, and an escaped
  // pattern in a template string is one more thing to get wrong.
  const lines = text.split(/\r?\n/).map((l) => l.trim());
  const pick = (label: string): string => {
    const prefix = `${label.toLowerCase()}:`;
    const hit = lines.find((l) => l.toLowerCase().startsWith(prefix));
    return hit ? hit.slice(prefix.length).trim() : '';
  };
  const statusLine = pick('Status');
  fact.scope = pick('Scope');
  fact.url = pick('URL');
  fact.raw = statusLine;

  if (!statusLine && !fact.url) {
    // Neither shape: the command itself failed (claude missing, crash, timeout).
    return { ...fact, status: exitOk ? 'absent' : 'unknown' };
  }
  fact.registered = true;
  if (/connected/i.test(statusLine) && !/not connected/i.test(statusLine)) fact.status = 'connected';
  else if (/auth/i.test(statusLine)) fact.status = 'needs-auth';
  else if (statusLine) fact.status = 'error';
  else fact.status = 'unknown';
  return fact;
}
