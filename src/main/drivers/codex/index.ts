// The Codex engine: one `codex app-server` process, one thread per chat.
//
// Why app-server and not `codex exec`: `exec` (and `@openai/codex-sdk`, which
// wraps it) starts a process per turn, so tvmcp and the knowledge base would
// boot on every message — the exact cost `src/main/session.ts` exists to avoid.
// app-server keeps the process, takes the system prompt as `baseInstructions`
// and applies MCP servers per thread through its free-form `config`.
//
// Everything this file assumes was measured on 2026-09-23 with codex-cli
// 0.154.0; see `protocol.ts` and `home.ts` for the two landmines (a request id
// of `0`, and a private CODEX_HOME being the only real isolation).
import { createHash } from 'node:crypto';
import type { AgentInit, PermissionPreset } from '@shared/api';
import * as store from '../../store';
import type { DriverEvent, EngineDriver, RunArgs, SessionSource, TurnSpec } from '../types';
import { codexHomeDir, ensureCodexHome } from './home';
import { AppServerError, startAppServer, type AppServer, type JsonValue, type Notification, type ServerRequest } from './protocol';
import { decide } from './gate';
import { serverNames, toCodexServers } from './servers';
import { CodexTurn } from './translate';

/** A thread the app holds open for a chat (or has prepared for the next one). */
interface ThreadState {
  threadId: string;
  /** Spec fingerprint the thread was started with. */
  key: string;
  servers: Set<string>;
  preset: PermissionPreset;
  init: AgentInit | null;
  turn: CodexTurn | null;
  reject: ((e: unknown) => void) | null;
  /** Where this chat's turn wants the engine's stderr; null between turns. */
  stderr: ((data: string) => void) | null;
  /** Startup status per MCP server, as the engine reports it for this thread. */
  serverStatus: Map<string, string>;
  /** Resolves once every server has settled (ready or failed), or the wait ran out. */
  ready: Promise<void>;
  settle: (() => void) | null;
}

/** How long a turn waits for the thread's MCP servers before going anyway. */
const SERVER_READY_TIMEOUT_MS = 30_000;

const threads = new Map<string, ThreadState>();
const byThread = new Map<string, ThreadState>();
let warm: { key: string; state: ThreadState } | null = null;

let server: AppServer | null = null;
let serverKey = '';
let serverVersion = '';
let starting: Promise<AppServer> | null = null;

/**
 * One process serves every chat, so its stderr goes to every turn in flight.
 * A single global sink would have let a second chat overwrite the first one's
 * and then null it on the way out — the same silent, two-chats-only class of
 * bug as a request id of `0`.
 */
function fanOutStderr(data: string): void {
  for (const [, state] of threads) state.stderr?.(data);
}

function specKey(spec: TurnSpec): string {
  const shape = {
    systemPrompt: spec.systemPrompt,
    model: spec.model,
    effort: spec.effort,
    cwd: spec.cwd,
    mcpServers: spec.mcpServers,
    permissionPreset: spec.permissionPreset,
  };
  return createHash('sha256').update(JSON.stringify(shape), 'utf8').digest('hex').slice(0, 16);
}

function sandboxFor(preset: PermissionPreset): 'read-only' | 'workspace-write' {
  return preset === 'extended' ? 'workspace-write' : 'read-only';
}

/** Answer an approval the engine asks for; the policy itself lives in `gate.ts`. */
async function approve(req: ServerRequest): Promise<Record<string, JsonValue>> {
  const threadId = String((req.params as { threadId?: string }).threadId ?? '');
  const state = byThread.get(threadId);
  return decide(req, state ? { servers: state.servers, preset: state.preset } : undefined);
}

function route(n: Notification): void {
  const threadId = String((n.params as { threadId?: string }).threadId ?? '');
  const state = threadId ? byThread.get(threadId) : undefined;
  if (!state) return;
  if (n.method === 'mcpServer/startupStatus/updated') {
    const name = String((n.params as { name?: string }).name ?? '');
    const status = String((n.params as { status?: string }).status ?? '');
    if (name) state.serverStatus.set(name, status);
    // Ready or failed are both answers; `starting` is not.
    const settled = [...state.servers].every((s) => {
      const st = state.serverStatus.get(s);
      return st === 'ready' || st === 'failed';
    });
    if (settled) {
      state.settle?.();
      state.settle = null;
    }
  }
  state.turn?.handle(n);
}

function dropServer(error?: unknown): void {
  const dead = server;
  server = null;
  serverKey = '';
  for (const [, state] of threads) state.reject?.(error ?? new AppServerError('app-server ended'));
  threads.clear();
  byThread.clear();
  warm = null;
  dead?.kill();
}

async function ensureServer(): Promise<AppServer> {
  const cfg = await store.getSettings();
  const command = (cfg.codexPath || '').trim() || 'codex';
  const home = await ensureCodexHome();
  const key = `${command}|${home}`;
  if (server?.alive && serverKey === key) return server;
  if (server) dropServer();
  if (starting) return starting;

  starting = (async () => {
    const started = startAppServer({
      command,
      codexHome: home,
      onNotification: route,
      onServerRequest: approve,
      onStderr: fanOutStderr,
      onExit: () => {
        if (server === started) dropServer();
      },
    });
    const res = await started.request('initialize', {
      clientInfo: { name: 'strategy-desk', title: 'Strategy Desk', version: '0.1.0' },
      // `experimentalApi` unlocks the v2 thread/turn methods; the elicitation
      // capability is what makes MCP tool approvals reach `approve()` at all -
      // without it a tool call just stalls.
      capabilities: { experimentalApi: true, requestAttestation: false, mcpServerOpenaiFormElicitation: true },
    });
    const home2 = String((res as { codexHome?: string }).codexHome ?? '');
    if (home2 && home2 !== home) {
      started.kill();
      throw new AppServerError(`codex-home-mismatch:${home2}`);
    }
    serverVersion = String((res as { userAgent?: string }).userAgent ?? '').match(/\/(\d+\.\d+\.\d+)/)?.[1] ?? '';
    server = started;
    serverKey = key;
    return started;
  })();
  try {
    return await starting;
  } finally {
    starting = null;
  }
}

async function startThread(spec: TurnSpec, resume: string | null): Promise<ThreadState> {
  const s = await ensureServer();
  const params: Record<string, JsonValue> = {
    cwd: spec.cwd,
    baseInstructions: spec.systemPrompt,
    sandbox: sandboxFor(spec.permissionPreset),
    // MCP tool calls need an approval channel; `never` refuses them outright
    // ("MCP tool call requires approval, but approval policy is never").
    approvalPolicy: 'on-request',
    // The user's own personality setting lives in their config, not ours.
    personality: 'none',
    config: { mcp_servers: toCodexServers(spec.mcpServers) },
    ...(spec.model ? { model: spec.model } : {}),
  };
  const res = resume ? await s.request('thread/resume', { threadId: resume, ...params }) : await s.request('thread/start', params);
  const thread = (res as { thread?: { id?: string } }).thread;
  const threadId = String(thread?.id ?? '');
  if (!threadId) throw new AppServerError('app-server returned no thread id');

  const state: ThreadState = {
    threadId,
    key: specKey(spec),
    servers: serverNames(spec.mcpServers),
    preset: spec.permissionPreset,
    init: {
      model: String((res as { model?: string }).model ?? spec.model ?? ''),
      engineVersion: serverVersion,
      mcpServers: Object.keys(spec.mcpServers).map((name) => ({ name, status: 'pending' })),
      // Codex hands the model its tools without telling the client how many.
      toolCount: 0,
      sessionId: threadId,
    },
    turn: null,
    reject: null,
    stderr: null,
    serverStatus: new Map(),
    ready: Promise.resolve(),
    settle: null,
  };
  // A turn must not start before the thread's MCP servers have settled: the
  // model is handed the tool list once, and tvmcp needs ~3.5 s to come up.
  // Measured the hard way — a turn sent too early answered "the TradingView
  // tool isn't available" and never called it.
  state.ready = new Promise<void>((resolve) => {
    state.settle = resolve;
    if (!state.servers.size) resolve();
    setTimeout(() => {
      state.settle = null;
      resolve();
    }, SERVER_READY_TIMEOUT_MS).unref?.();
  });
  byThread.set(threadId, state);
  return state;
}

/** The thread for this turn: the chat's own, else a prepared one, else new. */
async function acquire(chatKey: string, spec: TurnSpec, resume: string | null): Promise<{ state: ThreadState; source: SessionSource }> {
  const key = specKey(spec);
  const existing = threads.get(chatKey);
  if (existing && existing.key === key && (!resume || existing.threadId === resume)) return { state: existing, source: 'live' };
  if (existing) closeChat(chatKey);

  if (!resume && warm?.key === key) {
    const state = warm.state;
    warm = null;
    threads.set(chatKey, state);
    return { state, source: 'warm' };
  }

  const state = await startThread(spec, resume);
  threads.set(chatKey, state);
  return { state, source: 'cold' };
}

function closeChat(chatKey: string): void {
  const state = threads.get(chatKey);
  if (!state) return;
  threads.delete(chatKey);
  byThread.delete(state.threadId);
  state.reject?.(new AppServerError('thread closed'));
}

export const codexDriver: EngineDriver = {
  id: 'codex',

  fingerprint(spec) {
    return `codex:${specKey(spec)}`;
  },

  hasLive(chatKey, spec) {
    const state = threads.get(chatKey);
    return Boolean(state && state.key === specKey(spec) && server?.alive);
  },

  async prewarm(spec, resume) {
    // A prepared thread is worth exactly what it costs elsewhere: starting one
    // boots its MCP servers, so the first message does not wait for tvmcp.
    if (resume) return;
    const key = specKey(spec);
    if (warm?.key === key) return;
    const state = await startThread(spec, null);
    warm = { key, state };
  },

  async runTurn({ chatKey, spec, resume, text, emit, onStderr }: RunArgs) {
    const { state, source } = await acquire(chatKey, spec, resume);
    state.stderr = onStderr ?? null;
    const s = server;
    if (!s) throw new AppServerError('app-server is not running');

    // Wait for the tool list to exist before the model is asked to use it.
    await state.ready;

    if (state.init) {
      emit({ kind: 'init', init: { ...state.init, mcpServers: state.init.mcpServers.map((m) => ({ name: m.name, status: state.serverStatus.get(m.name) ?? m.status })) } });
      state.init = null;
    }
    for (const [name, status] of state.serverStatus) if (status === 'failed') emit({ kind: 'notice', message: `mcp:${name}:failed` });

    const done = new Promise<void>((resolve, reject) => {
      state.reject = reject;
      state.turn = new CodexTurn(state.threadId, (ev: DriverEvent) => {
        emit(ev);
        if (ev.kind === 'result') resolve();
      });
    });

    try {
      const res = await s.request('turn/start', {
        threadId: state.threadId,
        input: [{ type: 'text', text, text_elements: [] }],
        ...(spec.effort ? { effort: spec.effort } : {}),
      });
      const turnId = String((res as { turn?: { id?: string } }).turn?.id ?? '');
      if (turnId && state.turn) state.turn.turnId = turnId;
      await done;
    } finally {
      state.turn = null;
      state.reject = null;
      state.stderr = null;
    }
    return { source };
  },

  async stop(chatKey) {
    const state = threads.get(chatKey);
    if (!state || !server?.alive) return;
    const turnId = state.turn?.turnId;
    if (!turnId) return;
    try {
      await server.request('turn/interrupt', { threadId: state.threadId, turnId });
    } catch {
      /* the turn may have ended on its own between the press and here */
    }
    // The engine answers an interrupt with `turn/completed`; if it does not,
    // the turn's own accounting still closes the chat's UI state.
    state.turn?.finish();
  },

  closeFor(chatKey) {
    closeChat(chatKey);
  },

  closeAll() {
    dropServer();
  },

  isCrashError(e) {
    const msg = String((e as Error)?.message ?? e);
    return msg.includes('app-server exited') || msg.includes('app-server is not running') || msg.includes('app-server ended');
  },
};

export { codexHomeDir };
