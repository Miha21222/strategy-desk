import { describe, expect, it } from 'vitest';
import { foreignProcessesIn, parseProcessList } from './update-guard';

const ROOT = 'C:\\Users\\trader\\AppData\\Local\\Programs\\Strategy Desk';

describe('foreignProcessesIn', () => {
  it('ignores the app itself, however many Electron processes carry its name', () => {
    const procs = [
      { name: 'Strategy Desk', path: `${ROOT}\\Strategy Desk.exe` },
      { name: 'Strategy Desk', path: `${ROOT}\\Strategy Desk.exe` },
      { name: 'chrome', path: 'C:\\Program Files\\Google\\Chrome\\chrome.exe' },
    ];
    expect(foreignProcessesIn(ROOT, 'Strategy Desk.exe', procs)).toEqual([]);
  });

  it("reports the scheduler's claude.exe running from the install folder", () => {
    const procs = [
      { name: 'Strategy Desk', path: `${ROOT}\\Strategy Desk.exe` },
      { name: 'claude', path: `${ROOT}\\resources\\app.asar.unpacked\\node_modules\\@anthropic-ai\\claude-agent-sdk-win32-x64\\claude.exe` },
      { name: 'claude', path: `${ROOT}\\resources\\app.asar.unpacked\\node_modules\\@anthropic-ai\\claude-agent-sdk-win32-x64\\claude.exe` },
    ];
    expect(foreignProcessesIn(ROOT, 'Strategy Desk.exe', procs)).toEqual(['claude']);
  });

  it('does not match a sibling folder that merely shares the prefix, and is case-insensitive', () => {
    const procs = [
      { name: 'other', path: `${ROOT} 2\\other.exe` },
      { name: 'uv', path: `${ROOT.toUpperCase()}\\RESOURCES\\UV.EXE` },
    ];
    expect(foreignProcessesIn(ROOT, 'Strategy Desk.exe', procs)).toEqual(['uv']);
  });

  it('parses the probe output and drops malformed lines', () => {
    expect(parseProcessList('claude\tC:\\x\\claude.exe\r\n\r\nnopath\r\n\tC:\\y\r\nuv\tC:\\x\\uv.exe\n')).toEqual([
      { name: 'claude', path: 'C:\\x\\claude.exe' },
      { name: 'uv', path: 'C:\\x\\uv.exe' },
    ]);
  });
});
