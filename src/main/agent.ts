// Bridge between the renderer chat and the Claude Agent SDK.
// One query() per chat at a time; SDKMessages are translated into ChatEvents
// and streamed to the renderer over the `chat:event` channel.
import { promises as fs } from 'node:fs';
import type { Options, Query, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { AgentInit, AnswerLang, ChatEvent, ChatMeta, ChatMode, EffortLevel, Settings, TokenUsage, ToolCallItem } from '@shared/api';
import { buildMcpServers } from './mcp';
import { acquire, closeAll, closeFor, fingerprintOptions, hasLive, prewarm, runTurn } from './session';
import { taskInFlight } from './tasks';
import { assemblePrompt, promptForChat } from './prompt';
import { getWorkflow } from './workflows';
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

/** Mode suffix appended AFTER the frozen prompt so the cached prefix stays stable while the mode is unchanged. */
export function modeSuffix(mode: ChatMode): string {
  if (mode !== 'saver') return '';
  return ['', '', '## Mode: saver', 'Keep tool calls minimal: one status check, one screenshot at most, no exploratory calls.'].join('\n');
}

/**
 * Language suffix, also appended AFTER the frozen prompt. It has to be a
 * suffix rather than part of the frozen text: the app sends many prompts on
 * the user's behalf in English (quick actions, workflow prompts, report
 * catalog), so "answer in the language the user writes in" alone makes the
 * copilot reply in English to a Russian-speaking owner. Changing the setting
 * then reaches every existing chat at once, and the cached prefix stays stable
 * as long as the setting does.
 */
export function langSuffix(lang: AnswerLang): string {
  if (lang === 'auto') return '';
  const name = lang === 'ru' ? 'Russian' : 'English';
  return [
    '',
    '',
    '## Answer language',
    `Always answer in ${name}, whatever language the incoming prompt is written in: the app sends quick actions, workflow prompts and report requests in English on the user's behalf, and those are wiring, not a language choice.`,
    `Strings inside blocks (titles, labels, notes, next-step labels) are ${name} too. Tool names, tickers, code, Pine identifiers and file paths stay verbatim.`,
  ].join('\n');
}

/**
 * Effort for the turn. Saver caps it at medium instead of setting it: a user
 * who chose "low" asked for something cheaper than medium, and the saver
 * button must never make a turn more expensive than the full one.
 */
export function saverEffort(mode: ChatMode, configured: EffortLevel): EffortLevel | undefined {
  if (mode !== 'saver') return configured || undefined;
  return configured === 'low' ? 'low' : 'medium';
}

/** Frozen prompt + the two per-turn suffixes, in the one order every caller uses. */
export function composePrompt(base: string, lang: AnswerLang, mode: ChatMode): string {
  return `${base}${langSuffix(lang)}${modeSuffix(mode)}`;
}

export async function buildSystemPrompt(chat: ChatMeta, mode: ChatMode, lang: AnswerLang = 'auto'): Promise<string> {
  return composePrompt(await promptForChat(chat), lang, mode);
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

// `mcp__gt-database` is the bundled course knowledge base; `Read` is part of
// the same capability, because `gt_image_path` answers with a path to a chart
// screenshot that the model then has to open.
const STANDARD_ALLOWED = ['mcp__tradingview', 'mcp__mcp-tradingview', 'mcp__gt-database', 'Read', 'Grep', 'Glob', 'WebSearch', 'WebFetch'];
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
  // A reused session whose process went away ends the turn the same way a
  // crash at startup does: nothing was produced, so one retry is fair.
  return CRASH_CODES.some((c) => msg.includes(c)) || msg.includes('session ended');
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export interface StartArgs {
  chatKey: string;
  text: string;
  mode: ChatMode;
}

/**
 * The SDK options for one turn of this chat. Prewarming calls it too, with the
 * prompt a fresh chat would get, so the fingerprints of a prepared session and
 * of the real turn come out identical and the session can be adopted.
 */
export async function buildTurnOptions(chat: ChatMeta, mode: ChatMode, onStderr?: (data: string) => void): Promise<Options> {
  const cfg = await store.getSettings();
  return optionsForPrompt(await buildSystemPrompt(chat, mode, cfg.answerLang ?? 'auto'), mode, onStderr);
}

/** The same options around an already-composed system prompt. */
export async function optionsForPrompt(systemPrompt: string, mode: ChatMode, onStderr?: (data: string) => void): Promise<Options> {
  // If the user pressed «Развернуть рантайм» and started typing while it ran,
  // wait for it instead of spawning the MCP server against a half-built venv.
  // Provisioning is never STARTED here: that is the wizard's button, not a side
  // effect of sending a message.
  await taskInFlight('provision-runtime');
  const [cfg, secrets] = await Promise.all([store.getSettings(), store.getSecrets()]);
  const cwd = store.chatWorkspaceDir();
  await fs.mkdir(cwd, { recursive: true });
  const { allowedTools, disallowedTools } = permissionTools(cfg.permissionPreset);
  const effort = saverEffort(mode, cfg.effort);
  return {
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
    ...(onStderr ? { stderr: onStderr } : {}),
  };
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

  const stderrTail: string[] = [];
  const baseOptions = await buildTurnOptions(chat, mode, (data) => {
    stderrTail.push(data);
    if (stderrTail.length > 30) stderrTail.shift();
  });

  const now = Date.now();
  await store.appendTranscript(chatKey, { role: 'user', text, at: now });
  if (!chat.title) await store.updateChat(chatKey, { title: text.trim().slice(0, 60), updatedAt: now });

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
    // The chat's own session when it is still up, else the one prepared while
    // the user was typing, else a cold start. Either way it stays open
    // afterwards: the next turn reuses the process and its MCP servers.
    const { session, source } = await acquire(chatKey, baseOptions, sessionId);
    // Dev-only: proves what the composer's toggle actually did to this turn.
    if (process.env.NODE_ENV !== 'production') console.log(`[agent] turn: mode=${mode} effort=${baseOptions.effort ?? 'default'} session=${source}`);
    active.set(chatKey, { query: session.query, abort: session.abort, stopped: false });
    try {
      await runTurn(session, text, handle);
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
        closeFor(chatKey);
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

/**
 * Prepare the session the next turn will most likely need: this chat's, or -
 * with no chat open - the one a brand-new chat on the active workflow gets.
 * Best effort: a failure here only means the turn pays for its own boot.
 */
export async function prewarmFor(chatKey: string | null, mode: ChatMode): Promise<void> {
  try {
    if (chatKey) {
      if (active.has(chatKey)) return;
      const chat = await store.getChat(chatKey);
      if (!chat) return;
      const options = await buildTurnOptions(chat, mode);
      // The chat's own session is still up: nothing to prepare.
      if (hasLive(chatKey, fingerprintOptions(options))) return;
      await prewarm(options, chat.sessionId);
      return;
    }
    const cfg = await store.getSettings();
    const [profile, workflow] = await Promise.all([store.getProfile(), getWorkflow(cfg.activeWorkflow || null)]);
    const assembled = await assemblePrompt(profile, workflow, cfg.fieldNotesPath);
    const systemPrompt = composePrompt(assembled.text, cfg.answerLang ?? 'auto', mode);
    await prewarm(await optionsForPrompt(systemPrompt, mode), null);
  } catch {
    /* prewarming is an optimisation, never an error path */
  }
}

/** Drop every session; their options no longer describe the next turn. */
export function dropWarm(): void {
  closeAll();
}

export async function stopAll(): Promise<void> {
  closeAll();
  for (const [key, a] of active) {
    a.stopped = true;
    a.abort.abort();
    active.delete(key);
  }
}
