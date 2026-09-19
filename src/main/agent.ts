// Bridge between the renderer chat and the Claude Agent SDK.
// One query() per chat at a time; SDKMessages are translated into ChatEvents
// and streamed to the renderer over the `chat:event` channel.
import { app } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Options, Query, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { AgentInit, ChatEvent, ChatMode, Settings, TokenUsage, ToolCallItem, TraderProfile } from '@shared/api';
import { buildMcpServers } from './mcp';
import * as store from './store';

type Send = (ev: ChatEvent) => void;

interface Active {
  query: Query;
  abort: AbortController;
  stopped: boolean;
}

const active = new Map<string, Active>();

// Windows access-violation style exit codes: the CLI occasionally dies at
// startup with one of these; a single retry after a short pause usually works.
const CRASH_CODES = ['3221225477', '3221226505', '3221225725', '-1073741819', '-1073740791', '-1073741571'];
const RETRY_DELAY_MS = 1200;
const RESULT_TRUNCATE = 1200;

const FALLBACK_PROMPT =
  'You are a trading research copilot inside Strategy Desk, connected to TradingView Desktop through the `tradingview` MCP server. Call `tv_desktop_status` first when the user asks about their chart. Be concise. Educational only. Answer in the user\'s language.';

async function loadBasePrompt(): Promise<string> {
  const candidates = [
    path.join(app.getAppPath(), 'prompt', 'system-prompt.md'),
    path.join(process.cwd(), 'prompt', 'system-prompt.md'),
  ];
  for (const p of candidates) {
    try {
      return (await fs.readFile(p, 'utf8')).trim();
    } catch {
      /* try next */
    }
  }
  return FALLBACK_PROMPT;
}

function profileBlock(p: TraderProfile): string {
  const lines: string[] = [];
  if (p.callName) lines.push(`- Call the user: ${p.callName}`);
  if (p.traderType) lines.push(`- Trader type: ${p.traderType}`);
  if (p.markets) lines.push(`- Markets: ${p.markets}`);
  if (p.style) lines.push(`- Style: ${p.style}`);
  if (p.riskRules) lines.push(`- Risk rules: ${p.riskRules}`);
  if (p.propFirms) lines.push(`- Prop firms: ${p.propFirms}`);
  if (p.morningChecklist.length) lines.push(`- Morning checklist:\n${p.morningChecklist.map((s) => `  - ${s}`).join('\n')}`);
  return lines.length ? `\n\n## Trader profile\n${lines.join('\n')}` : '';
}

export async function buildSystemPrompt(profile: TraderProfile, mode: ChatMode): Promise<string> {
  const base = await loadBasePrompt();
  const saver = mode === 'saver' ? '\n\n## Mode: saver\nKeep tool calls minimal: one status check, one screenshot at most, no exploratory calls.' : '';
  return `${base}${profileBlock(profile)}${saver}\n\n## Hub\nStrategy Desk app version ${app.getVersion()}.`;
}

export function buildEnv(cfg: Settings, secrets: { apiKey: string | null; oauthToken: string | null }): Record<string, string | undefined> {
  const env: Record<string, string | undefined> = { ...process.env };
  delete env.ANTHROPIC_API_KEY;
  delete env.ANTHROPIC_AUTH_TOKEN;
  delete env.CLAUDE_CODE_OAUTH_TOKEN;
  if (cfg.authMode === 'api-key' && secrets.apiKey) env.ANTHROPIC_API_KEY = secrets.apiKey;
  else if (cfg.authMode === 'setup-token' && secrets.oauthToken) env.CLAUDE_CODE_OAUTH_TOKEN = secrets.oauthToken;
  // subscription: nothing set, the CLI reuses ~/.claude credentials.
  env.DISABLE_AUTOUPDATER = '1';
  return env;
}

const STANDARD_ALLOWED = ['mcp__tradingview', 'mcp__mcp-tradingview', 'Read', 'Grep', 'Glob', 'WebSearch', 'WebFetch'];
const STANDARD_DISALLOWED = ['Bash', 'PowerShell', 'Write', 'Edit', 'NotebookEdit', 'Skill', 'Task', 'TodoWrite'];

export function permissionTools(preset: Settings['permissionPreset']): { allowedTools: string[]; disallowedTools: string[] } {
  if (preset === 'extended') {
    // TODO: add a canUseTool callback that scopes Write/Edit to chat-workspace.
    return {
      allowedTools: [...STANDARD_ALLOWED, 'Write', 'Edit'],
      disallowedTools: STANDARD_DISALLOWED.filter((t) => t !== 'Write' && t !== 'Edit'),
    };
  }
  return { allowedTools: STANDARD_ALLOWED, disallowedTools: STANDARD_DISALLOWED };
}

function truncate(s: string, n = RESULT_TRUNCATE): string {
  return s.length > n ? `${s.slice(0, n)}\n… (${s.length - n} more chars)` : s;
}

function blockText(content: unknown): string {
  if (typeof content === 'string') return content;
  if (Array.isArray(content)) {
    return content
      .map((b) => (b && typeof b === 'object' && 'text' in b ? String((b as { text: unknown }).text) : b && typeof b === 'object' && (b as { type?: string }).type === 'image' ? '[image]' : ''))
      .filter(Boolean)
      .join('\n');
  }
  return '';
}

function isCrashError(e: unknown): boolean {
  const msg = String((e as Error)?.message ?? e);
  return CRASH_CODES.some((c) => msg.includes(c));
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface StartArgs {
  chatKey: string;
  text: string;
  mode: ChatMode;
}

export async function startChat(args: StartArgs, send: Send): Promise<void> {
  const { chatKey, text, mode } = args;
  if (active.has(chatKey)) {
    send({ chatKey, kind: 'error', message: 'busy' });
    return;
  }
  const chat = await store.getChat(chatKey);
  if (!chat) {
    send({ chatKey, kind: 'error', message: 'chat-not-found' });
    return;
  }

  const [cfg, secrets, profile] = await Promise.all([store.getSettings(), store.getSecrets(), store.getProfile()]);
  const systemPrompt = await buildSystemPrompt(profile, mode);
  const cwd = store.chatWorkspaceDir();
  await fs.mkdir(cwd, { recursive: true });

  const now = Date.now();
  await store.appendTranscript(chatKey, { role: 'user', text, at: now });
  if (!chat.title) await store.updateChat(chatKey, { title: text.trim().slice(0, 60), updatedAt: now });

  const stderrTail: string[] = [];
  const { allowedTools, disallowedTools } = permissionTools(cfg.permissionPreset);
  const effort = mode === 'saver' ? 'medium' : cfg.effort || undefined;

  const baseOptions: Options = {
    systemPrompt,
    mcpServers: buildMcpServers(cfg),
    allowedTools,
    disallowedTools,
    permissionMode: 'dontAsk',
    maxTurns: 100,
    cwd,
    env: buildEnv(cfg, secrets),
    includePartialMessages: true,
    settingSources: [],
    strictMcpConfig: true,
    ...(cfg.model.trim() ? { model: cfg.model.trim() } : {}),
    ...(effort ? { effort } : {}),
    stderr: (data: string) => {
      stderrTail.push(data);
      if (stderrTail.length > 30) stderrTail.shift();
    },
  };

  // Accumulated state of this turn (used for the transcript + done event).
  let assistantText = '';
  let sawThinking = false;
  const tools: ToolCallItem[] = [];
  const toolIndex = new Map<string, ToolCallItem>();
  let init: AgentInit | null = null;
  let gotResult = false;
  let sessionId: string | null = chat.sessionId;
  let tokens: TokenUsage = { in: 0, out: 0, cacheRead: 0, cacheWrite: 0 };
  let costUsd: number | null = null;
  let turns = 0;
  let stopped = false;

  const handle = (m: SDKMessage): void => {
    // Subagent frames are ignored (Task is disallowed anyway).
    if ('parent_tool_use_id' in m && m.parent_tool_use_id) return;

    if (m.type === 'system' && m.subtype === 'init') {
      sessionId = m.session_id;
      init = {
        model: m.model,
        claudeCodeVersion: m.claude_code_version,
        mcpServers: m.mcp_servers.map((s) => ({ name: s.name, status: s.status })),
        toolCount: m.tools.length,
        sessionId: m.session_id,
      };
      send({ chatKey, kind: 'init', init });
      const needsAuth = m.mcp_servers.filter((s) => s.status === 'needs-auth' || s.status === 'failed');
      for (const s of needsAuth) send({ chatKey, kind: 'notice', message: `mcp:${s.name}:${s.status}` });
      return;
    }

    if (m.type === 'stream_event') {
      const ev = m.event;
      if (ev.type === 'content_block_start') {
        if (ev.content_block.type === 'thinking' || ev.content_block.type === 'redacted_thinking') {
          sawThinking = true;
          send({ chatKey, kind: 'thinking', delta: '' });
        }
      } else if (ev.type === 'content_block_delta') {
        if (ev.delta.type === 'text_delta') {
          assistantText += ev.delta.text;
          send({ chatKey, kind: 'text', delta: ev.delta.text });
        } else if (ev.delta.type === 'thinking_delta' && ev.delta.thinking) {
          send({ chatKey, kind: 'thinking', delta: ev.delta.thinking });
        }
      }
      return;
    }

    if (m.type === 'assistant') {
      for (const block of m.message.content) {
        if (block.type === 'tool_use') {
          const item: ToolCallItem = { id: block.id, name: block.name, input: truncate(JSON.stringify(block.input ?? {}), 600), ok: null, result: '' };
          tools.push(item);
          toolIndex.set(block.id, item);
          send({ chatKey, kind: 'tool', id: item.id, name: item.name, input: item.input });
        }
      }
      return;
    }

    if (m.type === 'user') {
      const content = m.message.content;
      if (!Array.isArray(content)) return;
      for (const block of content) {
        if (block && typeof block === 'object' && block.type === 'tool_result') {
          const ok = !block.is_error;
          const textOut = truncate(blockText(block.content));
          const item = toolIndex.get(block.tool_use_id);
          if (item) {
            item.ok = ok;
            item.result = textOut;
          }
          send({ chatKey, kind: 'tool-result', id: block.tool_use_id, ok, text: textOut });
        }
      }
      return;
    }

    if (m.type === 'result') {
      gotResult = true;
      sessionId = m.session_id ?? sessionId;
      turns = m.num_turns;
      const u = m.usage;
      tokens = {
        in: u?.input_tokens ?? 0,
        out: u?.output_tokens ?? 0,
        cacheRead: u?.cache_read_input_tokens ?? 0,
        cacheWrite: u?.cache_creation_input_tokens ?? 0,
      };
      costUsd = typeof m.total_cost_usd === 'number' ? m.total_cost_usd : null;
      if (m.subtype !== 'success') {
        const errs = 'errors' in m && Array.isArray(m.errors) ? m.errors.join('\n') : '';
        send({ chatKey, kind: 'error', message: `${m.subtype}${errs ? `: ${errs}` : ''}` });
      }
      return;
    }
  };

  const runOnce = async (): Promise<void> => {
    const sdk = await import('@anthropic-ai/claude-agent-sdk');
    const abort = new AbortController();
    const options: Options = {
      ...baseOptions,
      abortController: abort,
      ...(sessionId ? { resume: sessionId } : { continue: false }),
    };
    const query = sdk.query({ prompt: text, options });
    active.set(chatKey, { query, abort, stopped: false });
    try {
      for await (const m of query) handle(m);
    } finally {
      stopped = active.get(chatKey)?.stopped ?? false;
      active.delete(chatKey);
    }
  };

  try {
    try {
      await runOnce();
    } catch (e) {
      if (!gotResult && !stopped && isCrashError(e)) {
        send({ chatKey, kind: 'notice', message: 'retry-after-crash' });
        await sleep(RETRY_DELAY_MS);
        await runOnce();
      } else if (!stopped) {
        throw e;
      }
    }
  } catch (e) {
    const msg = String((e as Error)?.message ?? e);
    const tail = stderrTail.join('').trim().split('\n').slice(-5).join('\n');
    send({ chatKey, kind: 'error', message: tail ? `${msg}\n${tail}` : msg });
  }

  // Persist the turn regardless of how it ended.
  const finishedAt = Date.now();
  if (assistantText || tools.length) {
    await store.appendTranscript(chatKey, { role: 'assistant', text: assistantText, at: finishedAt, tools, thinking: sawThinking });
  }
  const fresh = await store.getChat(chatKey);
  if (fresh) {
    // total_cost_usd is a running total for a resumed session (per SDK docs);
    // when it is smaller than what we have, treat it as per-call and add.
    const nextCost = costUsd == null ? fresh.costUsd : costUsd >= fresh.costUsd ? costUsd : fresh.costUsd + costUsd;
    await store.updateChat(chatKey, {
      sessionId,
      updatedAt: finishedAt,
      tokens: { in: fresh.tokens.in + tokens.in + tokens.cacheRead + tokens.cacheWrite, out: fresh.tokens.out + tokens.out },
      costUsd: nextCost,
    });
  }
  send({ chatKey, kind: 'done', tokens, costUsd, sessionId, turns, stopped });
}

export async function stopChat(chatKey: string): Promise<void> {
  const a = active.get(chatKey);
  if (!a) return;
  a.stopped = true;
  try {
    await a.query.interrupt();
  } catch {
    /* interrupt is best-effort; fall through to abort */
  }
  // If the process is still alive after a grace period, abort it outright.
  setTimeout(() => {
    if (active.get(chatKey) === a) a.abort.abort();
  }, 1500);
}

export function isChatActive(chatKey: string): boolean {
  return active.has(chatKey);
}

export async function stopAll(): Promise<void> {
  for (const [key, a] of active) {
    a.stopped = true;
    a.abort.abort();
    active.delete(key);
  }
}
