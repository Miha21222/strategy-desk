// Launch TradingView Desktop with --remote-debugging-port. Small on purpose:
// this later moves into the tvmcp tool `tv_desktop_launch`.
import { spawn, execFile } from 'node:child_process';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { TvLaunchResult } from '@shared/api';
import { cdpStatus, isTradingViewRunning } from './status';

async function exists(p: string): Promise<boolean> {
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

function powershell(script: string): Promise<string> {
  return new Promise((resolve) => {
    execFile('powershell', ['-NoProfile', '-NonInteractive', '-Command', script], { timeout: 15_000, windowsHide: true }, (_e, stdout) =>
      resolve(String(stdout ?? '').trim()),
    );
  });
}

export async function findTradingViewExe(configured: string): Promise<string | null> {
  if (configured && (await exists(configured))) return configured;
  if (process.platform !== 'win32') return null;
  const appx = await powershell("(Get-AppxPackage -Name 'TradingView.Desktop' | Select-Object -First 1).InstallLocation");
  if (appx) {
    const candidate = path.join(appx, 'TradingView.exe');
    if (await exists(candidate)) return candidate;
  }
  const local = process.env.LOCALAPPDATA;
  if (local) {
    const candidate = path.join(local, 'Programs', 'TradingView', 'TradingView.exe');
    if (await exists(candidate)) return candidate;
  }
  return null;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitForCdp(port: number, timeoutMs = 30_000): Promise<boolean> {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if ((await cdpStatus(port)).cdpUp) return true;
    await sleep(1000);
  }
  return false;
}

export async function launchTradingView(port: number, configuredExe: string): Promise<TvLaunchResult> {
  const flag = `--remote-debugging-port=${port}`;
  if ((await cdpStatus(port)).cdpUp) return { ok: true };
  if (await isTradingViewRunning()) {
    // Never kill the user's TradingView silently; the UI explains what to do.
    return { ok: false, error: 'running-without-flag' };
  }

  try {
    if (process.platform === 'darwin') {
      spawn('open', ['-a', 'TradingView', '--args', flag], { detached: true, stdio: 'ignore' }).unref();
    } else if (process.platform === 'win32') {
      const exe = await findTradingViewExe(configuredExe);
      if (!exe) return { ok: false, error: 'exe-not-found' };
      spawn(exe, [flag], { detached: true, stdio: 'ignore' }).unref();
    } else {
      spawn(configuredExe || 'tradingview', [flag], { detached: true, stdio: 'ignore' }).unref();
    }
  } catch (e) {
    return { ok: false, error: 'spawn-failed', detail: String((e as Error).message ?? e) };
  }

  const up = await waitForCdp(port);
  return up ? { ok: true } : { ok: false, error: 'timeout' };
}
