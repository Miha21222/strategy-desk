// Builds the `mcpServers` map passed to the Agent SDK.
import type { Settings } from '@shared/api';
import { DEFAULT_TOOLSETS, DEFAULT_CDP_PORT } from '@shared/defaults';

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

export function buildMcpServers(cfg: Settings, baseEnv: NodeJS.ProcessEnv = process.env): ServerMap {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(baseEnv)) if (typeof v === 'string') env[k] = v;
  env.TV_TOOLSETS = cfg.toolsets?.trim() || DEFAULT_TOOLSETS;
  env.TV_CDP_URL = `http://127.0.0.1:${cfg.cdpPort || DEFAULT_CDP_PORT}`;

  const servers: ServerMap = {
    tradingview: {
      type: 'stdio',
      command: 'uv',
      args: ['--directory', cfg.tvmcpPath, 'run', 'python', '-m', 'tvmcp'],
      env,
    },
  };

  if (cfg.officialMcp) {
    // TODO: auth for the official server is handled by Claude Code's own OAuth
    // store (the user runs `claude mcp add --transport http mcp-tradingview
    // https://mcp.tradingview.com/mcp` once and authenticates there). If the
    // SDK reports `needs-auth` for this server, the UI shows a notice.
    servers['mcp-tradingview'] = { type: 'http', url: 'https://mcp.tradingview.com/mcp' };
  }
  return servers;
}
