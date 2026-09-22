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
    tradingview: { type: 'stdio', command: spec.command, args: spec.args, env },
  };

  // The course knowledge base, out of the same environment as tvmcp. It is
  // registered whenever it CAN start: the model's own `gt_health` diagnoses a
  // stale or unseeded store far better than anything we could decide here, and
  // a server that refuses to boot costs one failed init, not a broken chat.
  if (kbMode(cfg, paths) !== 'off') {
    const kb = gtRagSpawn(cfg, paths);
    if (kb?.command) servers[KB_MCP_NAME] = { type: 'stdio', command: kb.command, args: kb.args, env: buildKbEnv(cfg, baseEnv, paths) };
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
