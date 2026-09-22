// The fingerprint decides whether a session may serve a turn. Too loose and a
// turn runs with the wrong prompt or tools; too tight and the process is
// rebuilt for every message, which is what this module exists to avoid.
import { describe, expect, it } from 'vitest';
import type { Options } from '@anthropic-ai/claude-agent-sdk';
import { fingerprintOptions, warmKey } from './session';

const base: Options = {
  systemPrompt: 'base prompt',
  model: 'claude-opus-5',
  allowedTools: ['mcp__tradingview', 'Read'],
  disallowedTools: ['Bash'],
  permissionMode: 'dontAsk',
  maxTurns: 100,
  cwd: 'C:/workspace',
  mcpServers: { tradingview: { type: 'stdio', command: 'uv', args: ['run', 'python', '-m', 'tvmcp'], env: {} } },
};

describe('session fingerprint', () => {
  it('matches for identical options', () => {
    expect(fingerprintOptions({ ...base })).toBe(fingerprintOptions(base));
  });

  it('ignores the stderr sink, which is per turn and not part of the session', () => {
    expect(fingerprintOptions({ ...base, stderr: () => undefined })).toBe(fingerprintOptions(base));
  });

  it.each([
    ['system prompt', { systemPrompt: 'other prompt' }],
    ['model', { model: 'claude-sonnet-5' }],
    ['allowed tools', { allowedTools: ['Read'] }],
    ['mcp servers', { mcpServers: {} }],
    ['cwd', { cwd: 'C:/elsewhere' }],
  ])('changes with the %s', (_name, patch) => {
    expect(fingerprintOptions({ ...base, ...(patch as Partial<Options>) })).not.toBe(fingerprintOptions(base));
  });

  it('ignores the resumed id: a live session already holds the conversation', () => {
    expect(fingerprintOptions(base)).toBe(fingerprintOptions({ ...base }));
  });
});

describe('warm key', () => {
  it('adds the conversation a prepared session was resumed from', () => {
    expect(warmKey(base, 'sess-1')).not.toBe(warmKey(base, 'sess-2'));
    expect(warmKey(base, null)).not.toBe(warmKey(base, 'sess-1'));
    expect(warmKey(base, 'sess-1')).toBe(warmKey({ ...base }, 'sess-1'));
  });

  it('is built on the options fingerprint', () => {
    expect(warmKey(base, null).startsWith(fingerprintOptions(base))).toBe(true);
  });
});
