// Parsing `claude mcp get`. The good case is the one observed on this machine
// (claude 2.1.278, 2026-09-22); the bad cases are deliberately read loosely,
// because their exact wording was never seen here and guessing one would turn
// "registered but not logged in" into "not registered" — which sends the user
// to register a server they already have.
import { describe, expect, it } from 'vitest';
import { parseMcpGet } from './official-mcp';

const connected = [
  'mcp-tradingview:',
  '  Scope: Local config (private to you in this project)',
  '  Status: ✔ Connected',
  '  Type: http',
  '  URL: https://mcp.tradingview.com/mcp',
  '',
  'To remove this server, run: claude mcp remove mcp-tradingview -s local',
].join('\n');

describe('a registered, authenticated server', () => {
  it('reads as connected, with its scope and url', () => {
    const f = parseMcpGet(connected, true);
    expect(f.registered).toBe(true);
    expect(f.status).toBe('connected');
    expect(f.url).toBe('https://mcp.tradingview.com/mcp');
    expect(f.scope).toContain('Local config');
  });
});

describe('a name claude does not know', () => {
  it('is absent, not an error', () => {
    const out = 'No MCP server named "mcp-tradingview". Configured servers: codegraph, chrome-devtools';
    const f = parseMcpGet(out, true);
    expect(f.registered).toBe(false);
    expect(f.status).toBe('absent');
  });
});

describe('a registration that is not usable yet', () => {
  it('treats an authentication status as needs-auth and keeps the wording', () => {
    const out = ['mcp-tradingview:', '  Scope: User config', '  Status: ✗ Needs authentication', '  Type: http'].join('\n');
    const f = parseMcpGet(out, true);
    expect(f.registered).toBe(true);
    expect(f.status).toBe('needs-auth');
    expect(f.raw).toContain('Needs authentication');
  });

  it('treats any other status as an error rather than as "fine"', () => {
    const out = ['mcp-tradingview:', '  Scope: User config', '  Status: ✗ Failed to connect', '  Type: http'].join('\n');
    expect(parseMcpGet(out, false).status).toBe('error');
  });

  // "Connected" must not be matched inside "not connected": that would report a
  // dead server as working and hide the only button that fixes it.
  it('does not read "Not connected" as connected', () => {
    const out = ['mcp-tradingview:', '  Status: Not connected', '  Type: http'].join('\n');
    expect(parseMcpGet(out, false).status).not.toBe('connected');
  });
});

describe('output that is neither shape', () => {
  it('is unknown when the command failed, so the check says "not checked"', () => {
    expect(parseMcpGet('', false).status).toBe('unknown');
    expect(parseMcpGet('some unrelated crash', false).status).toBe('unknown');
  });
});
