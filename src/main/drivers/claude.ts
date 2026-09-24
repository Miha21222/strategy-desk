// The Claude engine: the Agent SDK's `query()` behind the driver seam.
//
// Everything here was `agent.ts` until stage 6 — the SDK options, the auth
// environment, the permission presets and the translation of `SDKMessage` into
// `ChatEventBody`. It moved unchanged; what `agent.ts` kept is the part a Codex
// turn needs just as much (transcript, tallies, retry).
import type { Options, Query, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import type { AgentInit, Settings, ToolCallItem } from '@shared/api';
import { acquire, closeAll, closeFor, fingerprintOptions, hasLive, prewarm, runTurn } from '../session';
import * as store from '../store';
import type { DriverEvent, EngineDriver, RunArgs, TurnSpec } from './types';

/** Tool results are for reading, not archiving: the model already saw the whole thing. */
const RESULT_TRUNCATE = 1200;

// Windows access-violation style exit codes: the CLI occasionally dies at
// startup with one of these; a single retry after a short pause usually works.
const CRASH_CODES = ['3221225477', '3221226505', '3221225725', '-1073741819', '-1073740791', '-1073741571'];

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

/**
 * The SDK options for a spec, without the environment and the stderr sink.
 * Neither is part of the session's identity (`fingerprintOptions` ignores both),
 * so this stays synchronous and the fingerprint costs no store read.
 */
export function optionsFor(spec: TurnSpec): Options {
  const { allowedTools, disallowedTools } = permissionTools(spec.permissionPreset);
  return {
    systemPrompt: spec.systemPrompt,
    mcpServers: spec.mcpServers,
    allowedTools,
    disallowedTools,
    permissionMode: 'dontAsk',
    maxTurns: 100,
    cwd: spec.cwd,
    includePartialMessages: true,
    settingSources: [],
    strictMcpConfig: true,
    ...(spec.model.trim() ? { model: spec.model.trim() } : {}),
    ...(spec.effort ? { effort: spec.effort } : {}),
  };
}

/** The same options with the auth environment and, for a turn, a stderr sink. */
async function liveOptions(spec: TurnSpec, onStderr?: (data: string) => void): Promise<Options> {
  const [cfg, secrets] = await Promise.all([store.getSettings(), store.getSecrets()]);
  return {
    ...optionsFor(spec),
    env: buildEnv(cfg, secrets),
    ...(onStderr ? { stderr: onStderr } : {}),
  };
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

/**
 * One turn's worth of `SDKMessage` → `DriverEvent`. The only state it keeps is
 * the tool call index, because a result frame names a call by id.
 */
export function translate(emit: (ev: DriverEvent) => void): (m: SDKMessage) => void {
  const toolIndex = new Map<string, ToolCallItem>();

  return (m: SDKMessage): void => {
    // Subagent frames are ignored (Task is disallowed anyway).
    if ('parent_tool_use_id' in m && m.parent_tool_use_id) return;

    if (m.type === 'system' && m.subtype === 'init') {
      const init: AgentInit = {
        model: m.model,
        engineVersion: m.claude_code_version,
        mcpServers: m.mcp_servers.map((s) => ({ name: s.name, status: s.status })),
        toolCount: m.tools.length,
        sessionId: m.session_id,
      };
      emit({ kind: 'init', init });
      const needsAuth = m.mcp_servers.filter((s) => s.status === 'needs-auth' || s.status === 'failed');
      for (const s of needsAuth) emit({ kind: 'notice', message: `mcp:${s.name}:${s.status}` });
      return;
    }

    if (m.type === 'stream_event') {
      const ev = m.event;
      if (ev.type === 'content_block_start') {
        if (ev.content_block.type === 'thinking' || ev.content_block.type === 'redacted_thinking') emit({ kind: 'thinking', delta: '' });
      } else if (ev.type === 'content_block_delta') {
        if (ev.delta.type === 'text_delta') emit({ kind: 'text', delta: ev.delta.text });
        else if (ev.delta.type === 'thinking_delta' && ev.delta.thinking) emit({ kind: 'thinking', delta: ev.delta.thinking });
      }
      return;
    }

    if (m.type === 'assistant') {
      for (const block of m.message.content) {
        if (block.type === 'tool_use') {
          const item: ToolCallItem = { id: block.id, name: block.name, input: truncate(JSON.stringify(block.input ?? {}), 600), ok: null, result: '' };
          toolIndex.set(block.id, item);
          emit({ kind: 'tool', id: item.id, name: item.name, input: item.input });
        }
      }
      return;
    }

    if (m.type === 'user') {
      const content = m.message.content;
      if (!Array.isArray(content)) return;
      for (const block of content) {
        if (block && typeof block === 'object' && block.type === 'tool_result') {
          emit({ kind: 'tool-result', id: block.tool_use_id, ok: !block.is_error, text: truncate(blockText(block.content)) });
        }
      }
      return;
    }

    if (m.type === 'result') {
      const u = m.usage;
      if (m.subtype !== 'success') {
        const errs = 'errors' in m && Array.isArray(m.errors) ? m.errors.join('\n') : '';
        emit({ kind: 'error', message: `${m.subtype}${errs ? `: ${errs}` : ''}` });
      }
      emit({
        kind: 'result',
        sessionId: m.session_id ?? null,
        tokens: {
          in: u?.input_tokens ?? 0,
          out: u?.output_tokens ?? 0,
          cacheRead: u?.cache_read_input_tokens ?? 0,
          cacheWrite: u?.cache_creation_input_tokens ?? 0,
        },
        costUsd: typeof m.total_cost_usd === 'number' ? m.total_cost_usd : null,
        turns: m.num_turns,
      });
    }
  };
}

/** The query running a turn, so `stop()` can interrupt it. */
const active = new Map<string, { query: Query; abort: AbortController }>();

export const claudeDriver: EngineDriver = {
  id: 'claude',

  fingerprint(spec) {
    return `claude:${fingerprintOptions(optionsFor(spec))}`;
  },

  hasLive(chatKey, spec) {
    return hasLive(chatKey, fingerprintOptions(optionsFor(spec)));
  },

  async prewarm(spec, resume) {
    await prewarm(await liveOptions(spec), resume);
  },

  async runTurn({ chatKey, spec, resume, text, emit, onStderr }: RunArgs) {
    const options = await liveOptions(spec, onStderr);
    // The chat's own session when it is still up, else the one prepared while
    // the user was typing, else a cold start. Either way it stays open
    // afterwards: the next turn reuses the process and its MCP servers.
    const { session, source } = await acquire(chatKey, options, resume);
    active.set(chatKey, { query: session.query, abort: session.abort });
    try {
      await runTurn(session, text, translate(emit));
    } finally {
      active.delete(chatKey);
    }
    return { source };
  },

  async stop(chatKey) {
    const a = active.get(chatKey);
    if (!a) return;
    try {
      await a.query.interrupt();
    } catch {
      /* interrupt is best-effort; fall through to abort */
    }
    // If the process is still alive after a grace period, abort it outright.
    setTimeout(() => {
      if (active.get(chatKey) === a) a.abort.abort();
    }, 1500);
  },

  closeFor(chatKey) {
    active.delete(chatKey);
    closeFor(chatKey);
  },

  closeAll() {
    for (const [, a] of active) a.abort.abort();
    active.clear();
    closeAll();
  },

  isCrashError(e) {
    const msg = String((e as Error)?.message ?? e);
    // A reused session whose process went away ends the turn the same way a
    // crash at startup does: nothing was produced, so one retry is fair.
    return CRASH_CODES.some((c) => msg.includes(c)) || msg.includes('session ended');
  },
};
