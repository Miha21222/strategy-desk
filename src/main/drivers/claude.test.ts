// The Claude driver's two halves that a Codex driver will have to match: the
// fingerprint that decides whether a live session may serve a turn, and the
// translation of the engine's own frames into `ChatEventBody`.
import { describe, expect, it, vi } from 'vitest';
import type { SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { DriverEvent, TurnSpec } from './types';

vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getAppPath: () => process.cwd(), getPath: () => process.cwd(), isPackaged: false },
  safeStorage: { isEncryptionAvailable: () => false, encryptString: () => Buffer.from(''), decryptString: () => '' },
}));

const spec: TurnSpec = {
  engine: 'claude',
  systemPrompt: 'base prompt',
  model: 'claude-opus-5',
  effort: 'medium',
  cwd: 'C:/workspace',
  mcpServers: { tradingview: { type: 'stdio', command: 'uv', args: ['run', 'python', '-m', 'tvmcp'], env: {} } },
  permissionPreset: 'standard',
};

describe('claude driver fingerprint', () => {
  it('names the engine, so a session can never be adopted by another one', async () => {
    const { claudeDriver } = await import('./claude');
    expect(claudeDriver.fingerprint(spec).startsWith('claude:')).toBe(true);
  });

  it.each([
    ['system prompt', { systemPrompt: 'other' }],
    ['model', { model: 'claude-sonnet-5' }],
    ['effort', { effort: 'high' as const }],
    ['cwd', { cwd: 'C:/elsewhere' }],
    ['mcp servers', { mcpServers: {} }],
    ['permission preset', { permissionPreset: 'extended' as const }],
    ['macro profile', { profile: 'macro' as const }],
  ])('changes with the %s', async (_name, patch) => {
    const { claudeDriver } = await import('./claude');
    expect(claudeDriver.fingerprint({ ...spec, ...patch })).not.toBe(claudeDriver.fingerprint(spec));
  });

  it('matches for an identical spec', async () => {
    const { claudeDriver } = await import('./claude');
    expect(claudeDriver.fingerprint({ ...spec })).toBe(claudeDriver.fingerprint(spec));
  });
});

describe('permission presets', () => {
  it('standard forbids the shell and the file writers', async () => {
    const { permissionTools } = await import('./claude');
    const std = permissionTools('standard');
    expect(std.disallowedTools).toContain('Bash');
    expect(std.disallowedTools).toContain('Write');
    expect(std.allowedTools).toContain('mcp__tradingview');
  });

  it('extended adds writing but keeps the shell out', async () => {
    const { permissionTools } = await import('./claude');
    const ext = permissionTools('extended');
    expect(ext.allowedTools).toContain('Write');
    expect(ext.disallowedTools).toContain('Bash');
    expect(ext.disallowedTools).not.toContain('Write');
  });
});

describe('translate', () => {
  const run = async (messages: unknown[]): Promise<DriverEvent[]> => {
    const { translate } = await import('./claude');
    const out: DriverEvent[] = [];
    const handle = translate((ev) => out.push(ev));
    for (const m of messages) handle(m as SDKMessage);
    return out;
  };

  it('turns the init frame into an init event and flags a server that needs auth', async () => {
    const out = await run([
      {
        type: 'system',
        subtype: 'init',
        session_id: 'sess-1',
        model: 'claude-opus-5',
        claude_code_version: '2.0.0',
        tools: ['Read', 'Grep'],
        mcp_servers: [
          { name: 'tradingview', status: 'connected' },
          { name: 'mcp-tradingview', status: 'needs-auth' },
        ],
      },
    ]);
    expect(out[0]).toMatchObject({ kind: 'init', init: { sessionId: 'sess-1', toolCount: 2 } });
    expect(out[1]).toEqual({ kind: 'notice', message: 'mcp:mcp-tradingview:needs-auth' });
  });

  it('streams text and reasoning as separate channels', async () => {
    const out = await run([
      { type: 'stream_event', event: { type: 'content_block_start', content_block: { type: 'thinking' } } },
      { type: 'stream_event', event: { type: 'content_block_delta', delta: { type: 'thinking_delta', thinking: 'weighing' } } },
      { type: 'stream_event', event: { type: 'content_block_delta', delta: { type: 'text_delta', text: 'hello' } } },
    ]);
    expect(out).toEqual([
      { kind: 'thinking', delta: '' },
      { kind: 'thinking', delta: 'weighing' },
      { kind: 'text', delta: 'hello' },
    ]);
  });

  it('pairs a tool call with its result and marks a failure', async () => {
    const out = await run([
      { type: 'assistant', message: { content: [{ type: 'tool_use', id: 'call-1', name: 'tv_scan_levels', input: { symbol: 'US500' } }] } },
      { type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'call-1', is_error: true, content: 'NO_DATA' }] } },
    ]);
    expect(out[0]).toEqual({ kind: 'tool', id: 'call-1', name: 'tv_scan_levels', input: '{"symbol":"US500"}' });
    expect(out[1]).toEqual({ kind: 'tool-result', id: 'call-1', ok: false, text: 'NO_DATA' });
  });

  it('ignores subagent frames', async () => {
    const out = await run([{ type: 'stream_event', parent_tool_use_id: 'task-1', event: { type: 'content_block_delta', delta: { type: 'text_delta', text: 'inner' } } }]);
    expect(out).toEqual([]);
  });

  it('reports the turn accounting, and an unsuccessful result also as an error', async () => {
    const out = await run([
      {
        type: 'result',
        subtype: 'error_max_turns',
        session_id: 'sess-2',
        num_turns: 7,
        total_cost_usd: 0.42,
        usage: { input_tokens: 10, output_tokens: 20, cache_read_input_tokens: 30, cache_creation_input_tokens: 40 },
      },
    ]);
    expect(out[0]).toEqual({ kind: 'error', message: 'error_max_turns' });
    expect(out[1]).toEqual({
      kind: 'result',
      sessionId: 'sess-2',
      tokens: { in: 10, out: 20, cacheRead: 30, cacheWrite: 40 },
      costUsd: 0.42,
      turns: 7,
    });
  });

  it('truncates a long tool result instead of carrying it into the transcript whole', async () => {
    const out = await run([{ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'c', content: 'x'.repeat(5000) }] } }]);
    const ev = out[0] as { text: string };
    expect(ev.text.length).toBeLessThan(1400);
    expect(ev.text).toContain('more chars');
  });
});

describe('macro chat options', () => {
  it('runs the owner\'s own Claude Code in the vault: preset prompt + appendix, user/project settings, skills, no desk MCP', async () => {
    const { optionsFor, MACRO_ALLOWED } = await import('./claude');
    const o = optionsFor({ ...spec, profile: 'macro', cwd: 'C:/vault', mcpServers: {} });
    expect(o.systemPrompt).toEqual({ type: 'preset', preset: 'claude_code', append: 'base prompt' });
    expect(o.settingSources).toEqual(['user', 'project', 'local']);
    expect(o.skills).toBe('all');
    expect(o.mcpServers).toEqual({});
    expect(o.strictMcpConfig).toBeUndefined();
    expect(o.cwd).toBe('C:/vault');
    expect(o.allowedTools).toEqual(MACRO_ALLOWED);
    expect(o.allowedTools).toContain('Skill');
    expect(o.allowedTools).toContain('Bash');
    expect(o.disallowedTools).toContain('Task');
  });

  it('keeps the desk chat isolated as before', async () => {
    const { optionsFor } = await import('./claude');
    const o = optionsFor(spec);
    expect(o.settingSources).toEqual([]);
    expect(o.strictMcpConfig).toBe(true);
    expect(o.disallowedTools).toContain('Skill');
  });
});
