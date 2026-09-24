// `codex app-server` notifications → `DriverEvent`.
//
// The shapes here were read off a live run on 2026-09-23 (codex-cli 0.154.0),
// not guessed: a turn emits `item/started` + `item/completed` per item, streams
// the answer through `item/agentMessage/delta`, reports usage in
// `thread/tokenUsage/updated` and ends with `turn/completed`.
import type { TokenUsage } from '@shared/api';
import type { DriverEvent } from '../types';
import type { Notification } from './protocol';

/** Tool results are for reading, not archiving: the model already saw the whole thing. */
const RESULT_TRUNCATE = 1200;
const INPUT_TRUNCATE = 600;

function truncate(s: string, n = RESULT_TRUNCATE): string {
  return s.length > n ? `${s.slice(0, n)}\n… (${s.length - n} more chars)` : s;
}

interface Item {
  type?: string;
  id?: string;
  server?: string;
  tool?: string;
  arguments?: unknown;
  status?: string;
  result?: { content?: { type?: string; text?: string }[] } | null;
  error?: { message?: string } | null;
  text?: string;
  command?: string;
  aggregatedOutput?: string;
  exitCode?: number | null;
  query?: string;
  changes?: unknown;
}

/** Text out of an MCP tool result's content blocks. */
function contentText(result: Item['result']): string {
  const content = result?.content ?? [];
  return content
    .map((c) => (c?.type === 'text' ? String(c.text ?? '') : c?.type ? `[${c.type}]` : ''))
    .filter(Boolean)
    .join('\n');
}

/** How the app names a tool call, in the same shape the Claude engine reports. */
export function toolName(item: Item): string {
  switch (item.type) {
    case 'mcpToolCall':
      return `mcp__${item.server ?? '?'}__${item.tool ?? '?'}`;
    case 'commandExecution':
      return 'shell';
    case 'webSearch':
      return 'web_search';
    case 'fileChange':
      return 'file_change';
    default:
      return item.type ?? 'tool';
  }
}

function toolInput(item: Item): string {
  if (item.type === 'commandExecution') return truncate(String(item.command ?? ''), INPUT_TRUNCATE);
  if (item.type === 'webSearch') return truncate(String(item.query ?? ''), INPUT_TRUNCATE);
  if (item.type === 'fileChange') return truncate(JSON.stringify(item.changes ?? {}), INPUT_TRUNCATE);
  return truncate(JSON.stringify(item.arguments ?? {}), INPUT_TRUNCATE);
}

function toolResult(item: Item): { ok: boolean; text: string } {
  const failed = item.status === 'failed' || item.status === 'declined' || (typeof item.exitCode === 'number' && item.exitCode !== 0);
  const text = item.error?.message ? String(item.error.message) : item.type === 'commandExecution' ? String(item.aggregatedOutput ?? '') : contentText(item.result);
  return { ok: !failed, text: truncate(text) };
}

const TOOL_ITEMS = new Set(['mcpToolCall', 'commandExecution', 'webSearch', 'fileChange']);

const ZERO: TokenUsage = { in: 0, out: 0, cacheRead: 0, cacheWrite: 0 };

/**
 * One turn's worth of notifications. Stateful because Codex reports usage as
 * running buckets and because an answer that never streamed has to be emitted
 * from its completed item instead.
 */
export class CodexTurn {
  private tokens: TokenUsage = { ...ZERO };
  private streamed = new Set<string>();
  private toolCalls = 0;
  /** Set once `turn/completed` (or a failure) arrived, so the caller can stop waiting. */
  finished = false;

  constructor(
    readonly threadId: string,
    private readonly emit: (ev: DriverEvent) => void,
  ) {}

  /** Turn id, learned from the first notification that carries one. */
  turnId: string | null = null;

  handle(n: Notification): void {
    const p = n.params as Record<string, unknown>;
    if (typeof p.turnId === 'string' && !this.turnId) this.turnId = p.turnId;

    switch (n.method) {
      case 'item/agentMessage/delta': {
        const delta = String(p.delta ?? '');
        if (typeof p.itemId === 'string') this.streamed.add(p.itemId);
        if (delta) this.emit({ kind: 'text', delta });
        return;
      }
      case 'item/reasoning/textDelta':
      case 'item/reasoning/summaryTextDelta': {
        const delta = String(p.delta ?? p.text ?? '');
        if (delta) this.emit({ kind: 'thinking', delta });
        return;
      }
      case 'item/started': {
        const item = (p.item ?? {}) as Item;
        if (item.type === 'reasoning') this.emit({ kind: 'thinking', delta: '' });
        else if (TOOL_ITEMS.has(item.type ?? '')) {
          this.toolCalls += 1;
          this.emit({ kind: 'tool', id: String(item.id ?? ''), name: toolName(item), input: toolInput(item) });
        }
        return;
      }
      case 'item/completed': {
        const item = (p.item ?? {}) as Item;
        if (TOOL_ITEMS.has(item.type ?? '')) {
          const { ok, text } = toolResult(item);
          this.emit({ kind: 'tool-result', id: String(item.id ?? ''), ok, text });
        } else if (item.type === 'agentMessage' && item.id && !this.streamed.has(item.id) && item.text) {
          // No deltas arrived for this message (a non-streaming path): the
          // answer would otherwise be lost, so emit it whole exactly once.
          this.emit({ kind: 'text', delta: String(item.text) });
        }
        return;
      }
      case 'thread/tokenUsage/updated': {
        // `last` is the most recent model request; a turn can make several, so
        // they add up the way the Claude engine's per-turn usage does.
        const usage = (p.tokenUsage ?? {}) as { last?: Record<string, number> };
        const last = usage.last ?? {};
        const cached = Number(last.cachedInputTokens ?? 0);
        this.tokens = {
          in: this.tokens.in + Math.max(0, Number(last.inputTokens ?? 0) - cached),
          out: this.tokens.out + Number(last.outputTokens ?? 0),
          cacheRead: this.tokens.cacheRead + cached,
          cacheWrite: this.tokens.cacheWrite + Number(last.cacheWriteInputTokens ?? 0),
        };
        return;
      }
      case 'mcpServer/startupStatus/updated': {
        const status = String(p.status ?? '');
        if (status === 'failed') this.emit({ kind: 'notice', message: `mcp:${String(p.name ?? '?')}:failed` });
        return;
      }
      case 'error': {
        const err = (p.error ?? {}) as { message?: string };
        this.emit({ kind: 'error', message: String(err.message ?? 'error') });
        // `willRetry` means the engine handles it; only a final error ends the turn.
        if (p.willRetry !== true) this.finish();
        return;
      }
      case 'turn/completed': {
        const turn = (p.turn ?? {}) as { status?: string; error?: { message?: string } | null };
        if (turn.status && turn.status !== 'completed') {
          this.emit({ kind: 'error', message: String(turn.error?.message ?? turn.status) });
        }
        this.finish();
        return;
      }
      default:
        return;
    }
  }

  /** Emit the turn's accounting exactly once. */
  finish(): void {
    if (this.finished) return;
    this.finished = true;
    this.emit({
      kind: 'result',
      sessionId: this.threadId,
      tokens: this.tokens,
      // A ChatGPT subscription reports no price, and inventing one would be worse
      // than showing none: the UI renders a dash for null.
      costUsd: null,
      turns: this.toolCalls + 1,
    });
  }
}
