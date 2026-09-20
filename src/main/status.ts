// Connection status for the sidebar lights.
import { execFile } from 'node:child_process';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import type { ClaudeAuthStatus, Settings, Status } from '@shared/api';
import { DEFAULT_TOOLSETS } from '@shared/defaults';
import { EMPTY_CDP, type CdpProbe } from '@shared/preflight';

export type { CdpProbe };

const TVMCP_CACHE_MS = 60_000;
let tvmcpCache: { at: number; key: string; ok: boolean; tools: number; error: string } | null = null;

interface ExecResult {
  ok: boolean;
  notFound: boolean;
  out: string;
}

function exec(cmd: string, args: string[], opts: { env?: NodeJS.ProcessEnv; timeout?: number } = {}): Promise<ExecResult> {
  return new Promise((resolve) => {
    execFile(
      cmd,
      args,
      { env: opts.env, timeout: opts.timeout ?? 15_000, windowsHide: true, maxBuffer: 4 * 1024 * 1024 },
      (err, stdout, stderr) => {
        const notFound = Boolean(err && (err as NodeJS.ErrnoException).code === 'ENOENT');
        resolve({ ok: !err, notFound, out: `${stdout ?? ''}\n${stderr ?? ''}` });
      },
    );
  });
}

export async function isTradingViewRunning(): Promise<boolean> {
  if (process.platform === 'win32') {
    const { out } = await exec('tasklist', ['/FI', 'IMAGENAME eq TradingView.exe', '/NH', '/FO', 'CSV'], { timeout: 8000 });
    return /TradingView\.exe/i.test(out);
  }
  const { ok } = await exec('pgrep', ['-f', 'TradingView'], { timeout: 5000 });
  return ok;
}

async function fetchJson(url: string, timeoutMs = 1500): Promise<unknown | null> {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    const res = await fetch(url, { signal: ctl.signal });
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  } finally {
    clearTimeout(t);
  }
}

/**
 * Who answers on a CDP port, not merely whether someone does. A 200 from
 * /json/version proves only that *some* Chromium-based app owns the port —
 * 9222 on this machine belongs to a browser panel, and TradingView silently
 * skips a busy port at launch.
 */
export async function cdpProbe(port: number, timeoutMs = 1500): Promise<CdpProbe> {
  const version = (await fetchJson(`http://127.0.0.1:${port}/json/version`, timeoutMs)) as { Browser?: string } | null;
  if (!version) return EMPTY_CDP;
  const list = await fetchJson(`http://127.0.0.1:${port}/json/list`, timeoutMs);
  const pages = Array.isArray(list) ? (list as { url?: string; title?: string }[]) : [];
  const targets = pages.slice(0, 8).map((p) => ({ url: String(p.url ?? ''), title: String(p.title ?? '') }));
  const chartTab = targets.some((p) => /tradingview\.com\/chart/i.test(p.url));
  const tradingview = chartTab || targets.some((p) => /tradingview/i.test(p.url) || /tradingview/i.test(p.title));
  return { up: true, browser: String(version.Browser ?? '').trim(), chartTab, tradingview, targets };
}

export async function cdpStatus(port: number): Promise<{ cdpUp: boolean; chartTab: boolean }> {
  const probe = await cdpProbe(port);
  return { cdpUp: probe.up, chartTab: probe.chartTab };
}

export async function tvmcpStatus(cfg: Settings): Promise<{ ok: boolean; tools: number; error: string }> {
  const key = `${cfg.tvmcpPath}|${cfg.toolsets}`;
  if (tvmcpCache && tvmcpCache.key === key && Date.now() - tvmcpCache.at < TVMCP_CACHE_MS) {
    return { ok: tvmcpCache.ok, tools: tvmcpCache.tools, error: tvmcpCache.error };
  }
  const env = { ...process.env, TV_TOOLSETS: cfg.toolsets?.trim() || DEFAULT_TOOLSETS };
  const r = await exec('uv', ['--directory', cfg.tvmcpPath, 'run', 'python', '-m', 'tvmcp', '--check'], { env, timeout: 60_000 });
  const m = r.out.match(/(\d+)\s+tools registered/);
  const ok = r.ok && Boolean(m);
  let error = '';
  if (!ok) {
    error = r.notFound ? 'uv not found' : r.out.trim().split('\n').filter(Boolean).slice(-3).join('\n').slice(0, 400);
  }
  const result = { ok, tools: m ? Number(m[1]) : 0, error };
  tvmcpCache = { at: Date.now(), key, ...result };
  return result;
}

export function invalidateTvmcpCache(): void {
  tvmcpCache = null;
}

export async function claudeAuthStatus(cfg: Settings): Promise<ClaudeAuthStatus> {
  if (cfg.authMode === 'api-key' && cfg.hasApiKey) return 'api-key';
  if (cfg.authMode === 'setup-token' && cfg.hasOauthToken) return 'setup-token';
  try {
    const raw = await fs.readFile(path.join(os.homedir(), '.claude', '.credentials.json'), 'utf8');
    if (raw.includes('accessToken')) return 'subscription';
  } catch {
    /* no credentials file */
  }
  return 'none';
}

export async function getStatus(cfg: Settings): Promise<Status> {
  const [tvRunning, cdp, tvmcp, claude] = await Promise.all([
    isTradingViewRunning(),
    cdpStatus(cfg.cdpPort),
    tvmcpStatus(cfg),
    claudeAuthStatus(cfg),
  ]);
  return {
    tvRunning,
    cdpUp: cdp.cdpUp,
    chartTab: cdp.chartTab,
    tvmcpOk: tvmcp.ok,
    tvmcpTools: tvmcp.tools,
    tvmcpError: tvmcp.error,
    claude,
    checkedAt: Date.now(),
  };
}
