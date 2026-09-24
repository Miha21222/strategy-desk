// Codex notifications → ChatEvent bodies. Every frame below is the real shape
// captured from `codex app-server` (codex-cli 0.154.0) on 2026-09-23, trimmed
// of the fields the app does not read.
import { describe, expect, it } from 'vitest';
import type { DriverEvent } from '../types';
import { CodexTurn, toolName } from './translate';

const collect = (frames: { method: string; params: Record<string, unknown> }[]): DriverEvent[] => {
  const out: DriverEvent[] = [];
  const turn = new CodexTurn('thread-1', (ev) => out.push(ev));
  for (const f of frames) turn.handle(f);
  return out;
};

describe('tool naming', () => {
  it('reads like the Claude engine, so one transcript renders both', () => {
    expect(toolName({ type: 'mcpToolCall', server: 'tradingview', tool: 'tv_scan_levels' })).toBe('mcp__tradingview__tv_scan_levels');
    expect(toolName({ type: 'commandExecution' })).toBe('shell');
    expect(toolName({ type: 'webSearch' })).toBe('web_search');
  });
});

describe('codex turn', () => {
  it('streams the answer and the reasoning on their own channels', () => {
    const out = collect([
      { method: 'item/started', params: { item: { type: 'reasoning', id: 'rs_1' } } },
      { method: 'item/reasoning/summaryTextDelta', params: { delta: 'weighing' } },
      { method: 'item/agentMessage/delta', params: { itemId: 'msg_1', delta: 'hel' } },
      { method: 'item/agentMessage/delta', params: { itemId: 'msg_1', delta: 'lo' } },
      { method: 'item/completed', params: { item: { type: 'agentMessage', id: 'msg_1', text: 'hello' } } },
    ]);
    expect(out).toEqual([
      { kind: 'thinking', delta: '' },
      { kind: 'thinking', delta: 'weighing' },
      { kind: 'text', delta: 'hel' },
      { kind: 'text', delta: 'lo' },
    ]);
  });

  it('emits a message that never streamed, exactly once', () => {
    const out = collect([{ method: 'item/completed', params: { item: { type: 'agentMessage', id: 'msg_2', text: 'whole answer' } } }]);
    expect(out).toEqual([{ kind: 'text', delta: 'whole answer' }]);
  });

  it('pairs an MCP tool call with its result', () => {
    const out = collect([
      { method: 'item/started', params: { item: { type: 'mcpToolCall', id: 'exec-1', server: 'mini', tool: 'ping', arguments: { text: 'desk' }, status: 'inProgress' } } },
      {
        method: 'item/completed',
        params: { item: { type: 'mcpToolCall', id: 'exec-1', server: 'mini', tool: 'ping', arguments: { text: 'desk' }, status: 'completed', result: { content: [{ type: 'text', text: 'pong:desk' }] } } },
      },
    ]);
    expect(out[0]).toEqual({ kind: 'tool', id: 'exec-1', name: 'mcp__mini__ping', input: '{"text":"desk"}' });
    expect(out[1]).toEqual({ kind: 'tool-result', id: 'exec-1', ok: true, text: 'pong:desk' });
  });

  it('reports a refused tool call as a failed result, not as silence', () => {
    // This is what `approvalPolicy: "never"` produced in the live run.
    const out = collect([
      {
        method: 'item/completed',
        params: { item: { type: 'mcpToolCall', id: 'exec-2', server: 'mini', tool: 'ping', status: 'failed', error: { message: 'MCP tool call requires approval, but approval policy is never' } } },
      },
    ]);
    expect(out[0]).toMatchObject({ kind: 'tool-result', ok: false });
    expect((out[0] as { text: string }).text).toContain('requires approval');
  });

  it('adds up the usage of every model request in the turn', () => {
    const usage = (input: number, cached: number, output: number) => ({
      method: 'thread/tokenUsage/updated',
      params: { tokenUsage: { last: { inputTokens: input, cachedInputTokens: cached, cacheWriteInputTokens: 0, outputTokens: output } } },
    });
    const out = collect([usage(16561, 0, 33), usage(18686, 18432, 15), { method: 'turn/completed', params: { turn: { status: 'completed' } } }]);
    const result = out.at(-1) as Extract<DriverEvent, { kind: 'result' }>;
    expect(result.tokens).toEqual({ in: 16561 + (18686 - 18432), out: 48, cacheRead: 18432, cacheWrite: 0 });
    // A ChatGPT subscription quotes no price; the UI shows a dash for null.
    expect(result.costUsd).toBeNull();
    expect(result.sessionId).toBe('thread-1');
  });

  it('turns a failed turn into an error and still closes the turn', () => {
    const out = collect([{ method: 'turn/completed', params: { turn: { status: 'failed', error: { message: 'stream disconnected' } } } }]);
    expect(out[0]).toEqual({ kind: 'error', message: 'stream disconnected' });
    expect(out[1]).toMatchObject({ kind: 'result' });
  });

  it('keeps the turn open while the engine says it will retry', () => {
    const out = collect([{ method: 'error', params: { error: { message: 'rate limited' }, willRetry: true } }]);
    expect(out).toEqual([{ kind: 'error', message: 'rate limited' }]);
  });

  it('flags an MCP server that failed to start', () => {
    const out = collect([
      { method: 'mcpServer/startupStatus/updated', params: { name: 'tradingview', status: 'starting' } },
      { method: 'mcpServer/startupStatus/updated', params: { name: 'tradingview', status: 'failed' } },
    ]);
    expect(out).toEqual([{ kind: 'notice', message: 'mcp:tradingview:failed' }]);
  });

  it('reports the result only once, however the turn ended', () => {
    const out = collect([
      { method: 'turn/completed', params: { turn: { status: 'completed' } } },
      { method: 'turn/completed', params: { turn: { status: 'completed' } } },
    ]);
    expect(out.filter((e) => e.kind === 'result')).toHaveLength(1);
  });
});
