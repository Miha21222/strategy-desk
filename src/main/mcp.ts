// Builds the `mcpServers` map passed to the Agent SDK.
//
// The command itself comes from `runtime-spec.ts`, which every tvmcp spawn in
// the app goes through — the MCP server here, the `--check` probe in status.ts
// and the deterministic CLI in tv-cli.ts. One shape, so a bundled install and a
// developer's own checkout cannot drift apart between them.
import type { Settings } from '@shared/api';
import { OFFICIAL_MCP_NAME, OFFICIAL_MCP_URL } from '@shared/official-mcp';
import { buildKbEnv, buildTvEnv as buildEnvWith, gtRagSpawn, kbMode, runtimePaths, tvmcpSpawn } from './runtime-spec';

/** The name the knowledge base's tools carry: `mcp__gt-database__gt_*`. */
export const KB_MCP_NAME = 'gt-database';

export interface StdioServer {
  type: 'stdio';
  command: string;
  args: string[];
  env: Record<string, string>;
  /**
   * Put this server's tools in the prompt instead of behind tool search.
   *
   * With three servers registered the CLI turns tool search on, and a deferred
   * tool costs the model a `ToolSearch` round trip before it may call anything
   * — measured in a real chat: "read my chart" spent whole turns discovering
   * `tv_desktop_*` before the first screenshot. These two servers ARE the
   * product, so their schemas belong in the prompt: ~22k tokens for tvmcp's 64
   * tools and ~2k for the knowledge base's 8, paid once into the cached prefix
   * of a session that outlives the turn.
   *
   * The flag also makes startup wait for the server to connect (5 s cap), which
   * is why gt-rag imports chromadb lazily — the ~1 s it now takes to answer
   * `list_tools` fits, the ~14 s of a cold import did not.
   */
  alwaysLoad?: boolean;
}
export interface HttpServer {
  type: 'http';
  url: string;
}
export type ServerMap = Record<string, StdioServer | HttpServer>;

/**
 * The environment tvmcp runs with: the app's own, the two knobs the settings
 * own, and — in bundled mode — the cache locations that must not be written
 * inside the installation directory. See `runtime-spec.ts`.
 *
 * `tv-cli.ts` reuses this, so a deterministic CLI poll sees exactly what the
 * model's own tool call would have seen.
 */
export function buildTvEnv(cfg: Settings, baseEnv: NodeJS.ProcessEnv = process.env): Record<string, string> {
  return buildEnvWith(cfg, baseEnv, runtimePaths());
}

export function buildMcpServers(cfg: Settings, baseEnv: NodeJS.ProcessEnv = process.env): ServerMap {
  const paths = runtimePaths();
  const env = buildTvEnv(cfg, baseEnv);
  const spec = tvmcpSpawn(cfg, paths, 'server');

  const servers: ServerMap = {
    tradingview: { type: 'stdio', command: spec.command, args: spec.args, env, alwaysLoad: true },
  };

  // The course knowledge base, out of the same environment as tvmcp. It is
  // registered whenever it CAN start: the model's own `gt_health` diagnoses a
  // stale or unseeded store far better than anything we could decide here, and
  // a server that refuses to boot costs one failed init, not a broken chat.
  if (kbMode(cfg, paths) !== 'off') {
    const kb = gtRagSpawn(cfg, paths);
    if (kb?.command) servers[KB_MCP_NAME] = { type: 'stdio', command: kb.command, args: kb.args, env: buildKbEnv(cfg, baseEnv, paths), alwaysLoad: true };
  }

  if (cfg.officialMcp) {
    // Auth for the official server is Claude Code's own OAuth store, and that
    // store IS consulted even though we pass the server programmatically with
    // `strictMcpConfig: true` and `settingSources: []` — verified 2026-09-22:
    // the init event reports `{"name":"mcp-tradingview","status":"connected",
    // "source":"dynamic"}`. Creating and authenticating that registration is
    // what the preflight's `officialMcp` check offers; see shared/official-mcp.ts.
    servers[OFFICIAL_MCP_NAME] = { type: 'http', url: OFFICIAL_MCP_URL };
  }
  return servers;
}
