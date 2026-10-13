// The pure half of the updater's busy check. The desk knows the chats and tasks
// it started itself, but the Task Scheduler's review (09:30 / 16:00 / 20:00)
// runs `claude.exe` straight from the install folder, outside the desk. So
// before a silent install the main process lists the processes whose
// executable lives under the install root and asks this function whether any
// of them is not the app itself. Electron-free, so vitest covers the rule.

/** A process as the probe reports it: its name (no path, no extension) and the path of its executable. */
export interface RunningProcess {
  name: string;
  path: string;
}

const norm = (p: string) => p.replace(/\//g, '\\').replace(/\\+$/, '').toLowerCase();

/**
 * Names of processes running from `root` that are not `ownName` (the app's own
 * exe name: Electron's renderer, GPU and utility processes all carry it). An
 * empty list means the installer may replace the files.
 */
export function foreignProcessesIn(root: string, ownName: string, procs: RunningProcess[]): string[] {
  const base = norm(root) + '\\';
  const own = ownName.toLowerCase().replace(/\.exe$/, '');
  const out = new Set<string>();
  for (const p of procs) {
    if (!p.path) continue;
    if (!norm(p.path).startsWith(base)) continue;
    const name = p.name.toLowerCase().replace(/\.exe$/, '');
    if (name === own) continue;
    out.add(p.name);
  }
  return [...out].sort();
}

/** The PowerShell that lists every process with a path, as `name\tpath` lines; the filtering is done here, not in PowerShell. */
export const LIST_PROCESSES_PS = "Get-Process | Where-Object { $_.Path } | ForEach-Object { $_.ProcessName + \"`t\" + $_.Path }";

/** Parse the probe's output into processes; blank and malformed lines are dropped. */
export function parseProcessList(text: string): RunningProcess[] {
  const out: RunningProcess[] = [];
  for (const line of text.split(/\r?\n/)) {
    const i = line.indexOf('\t');
    if (i <= 0) continue;
    const name = line.slice(0, i).trim();
    const path = line.slice(i + 1).trim();
    if (name && path) out.push({ name, path });
  }
  return out;
}
