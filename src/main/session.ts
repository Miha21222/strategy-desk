// Agent sessions: one live CLI process per chat, plus one prepared in advance.
//
// The Agent SDK spawns a Claude Code process per `query()`, and that process
// spawns our MCP server. Measured here, that boot costs ~3.5 s, nearly all of
// it tvmcp coming up (Python + fastmcp). Paying it per message is what made the
// chat feel slow, so a session now lives as long as its chat:
//
//   * `prewarm()` starts an unbound session before there is any text, so the
//     first message of a chat does not wait for the boot either;
//   * `acquire()` hands that session to a chat when the options match exactly;
//   * the session's pump keeps reading the stream between turns, so the
//     process, its MCP servers and the cached prompt prefix stay alive.
//
// A session may only serve a turn whose options match the ones it started
// with - system prompt, model, tools, MCP servers, cwd - hence the fingerprint.
// Anything else (settings changed, another workflow, another chat) starts its
// own session; nothing here is ever a hard failure, only a slower turn.
import { createHash } from 'node:crypto';
import type { Options, Query, SDKMessage } from '@anthropic-ai/claude-agent-sdk';
import { createInputChannel, type InputChannel } from './stream-input';

/** A prepared session nobody adopted is closed after this long. */
export const WARM_TTL_MS = 10 * 60_000;
/** A chat's session is closed after this long without a turn. */
export const IDLE_TTL_MS = 30 * 60_000;
/** Live sessions kept at once (each holds a CLI process and an MCP server). */
export const MAX_LIVE = 3;

export interface TurnSink {
  handle: (m: SDKMessage) => void;
  /** Called once per turn: on the result frame, or when the stream dies. */
  finish: (error?: unknown) => void;
}

export interface Session {
  /** Options fingerprint, without the resumed id: what the process was started with. */
  key: string;
  chatKey: string | null;
  query: Query;
  abort: AbortController;
  input: InputChannel;
  turn: TurnSink | null;
  closed: boolean;
  startedAt: number;
  lastUsedAt: number;
}

/** Everything that must match for a session to be reusable for a turn. */
export function fingerprintOptions(options: Options): string {
  const shape = {
    systemPrompt: typeof options.systemPrompt === 'string' ? options.systemPrompt : JSON.stringify(options.systemPrompt ?? null),
    model: options.model ?? '',
    effort: options.effort ?? '',
    allowedTools: options.allowedTools ?? [],
    disallowedTools: options.disallowedTools ?? [],
    permissionMode: options.permissionMode ?? '',
    maxTurns: options.maxTurns ?? 0,
    cwd: options.cwd ?? '',
    mcpServers: options.mcpServers ?? {},
    settingSources: options.settingSources ?? null,
  };
  return createHash('sha256').update(JSON.stringify(shape), 'utf8').digest('hex').slice(0, 16);
}

/** A prepared session also carries the conversation it was resumed from. */
export function warmKey(options: Options, resume: string | null): string {
  return `${fingerprintOptions(options)}:${resume ?? ''}`;
}

const live = new Map<string, Session>();
let warm: { key: string; session: Session } | null = null;
let warming: Promise<void> | null = null;
let warmTimer: ReturnType<typeof setTimeout> | null = null;

function stopSession(s: Session): void {
  if (s.closed) return;
  s.closed = true;
  s.turn = null;
  s.input.end();
  try {
    s.abort.abort();
  } catch {
    /* already gone */
  }
}

/** The stream ended or threw: fail the turn in flight and forget the session. */
function endSession(session: Session, error?: unknown): void {
  const turn = session.turn;
  session.turn = null;
  session.closed = true;
  if (warm?.session === session) warm = null;
  for (const [key, s] of live) if (s === session) live.delete(key);
  turn?.finish(error ?? new Error('session ended'));
}

/**
 * Read the session's stream for as long as it lives, handing every frame to
 * whichever turn is running. Without this the process would die between turns:
 * breaking out of a `for await` returns the iterator and ends the query.
 */
function pump(session: Session): void {
  void (async () => {
    try {
      for await (const m of session.query) {
        session.turn?.handle(m);
        if (m.type === 'result') {
          const turn = session.turn;
          session.turn = null;
          session.lastUsedAt = Date.now();
          turn?.finish();
        }
      }
      endSession(session);
    } catch (error) {
      endSession(session, error);
    }
  })();
}

async function spawn(options: Options, resume: string | null): Promise<Session> {
  const sdk = await import('@anthropic-ai/claude-agent-sdk');
  const abort = new AbortController();
  const input = createInputChannel();
  const query = sdk.query({ prompt: input.stream, options: { ...options, abortController: abort, ...(resume ? { resume } : { continue: false }) } });
  const now = Date.now();
  const session: Session = { key: fingerprintOptions(options), chatKey: null, query, abort, input, turn: null, closed: false, startedAt: now, lastUsedAt: now };
  pump(session);
  return session;
}

/** Drop closed and long-idle sessions, then the oldest ones over the budget. */
function trim(): void {
  const cutoff = Date.now() - IDLE_TTL_MS;
  for (const [key, s] of [...live]) {
    if (!s.turn && (s.closed || s.lastUsedAt < cutoff)) {
      live.delete(key);
      stopSession(s);
    }
  }
  while (live.size > MAX_LIVE) {
    const idle = [...live.entries()].filter(([, s]) => !s.turn).sort((a, b) => a[1].lastUsedAt - b[1].lastUsedAt)[0];
    if (!idle) break;
    live.delete(idle[0]);
    stopSession(idle[1]);
  }
}

export function closeWarm(): void {
  if (warmTimer) clearTimeout(warmTimer);
  warmTimer = null;
  const w = warm;
  warm = null;
  if (w) stopSession(w.session);
}

/** Close everything: settings changed under us, or the app is quitting. */
export function closeAll(): void {
  closeWarm();
  for (const [key, s] of [...live]) {
    live.delete(key);
    stopSession(s);
  }
}

/** Close the session of one chat (it crashed, or the chat is gone). */
export function closeFor(chatKey: string): void {
  const s = live.get(chatKey);
  if (!s) return;
  live.delete(chatKey);
  stopSession(s);
}

/** True when this chat already has a usable session for these options. */
export function hasLive(chatKey: string, key: string): boolean {
  const s = live.get(chatKey);
  return Boolean(s && !s.closed && s.key === key);
}

/** Start a session now and keep it idle until a turn adopts it. */
export async function prewarm(options: Options, resume: string | null): Promise<void> {
  const key = warmKey(options, resume);
  if (warm?.key === key) return;
  if (warming) await warming.catch(() => undefined);
  if (warm?.key === key) return;
  closeWarm();
  warming = (async () => {
    const session = await spawn(options, resume);
    warm = { key, session };
    warmTimer = setTimeout(() => closeWarm(), WARM_TTL_MS);
  })();
  try {
    await warming;
  } catch {
    warm = null;
  } finally {
    warming = null;
  }
}

export type SessionSource = 'live' | 'warm' | 'cold';

/**
 * The session for this turn: the chat's own one, else the prepared one, else a
 * fresh one. The caller runs the turn on it and leaves it open.
 */
export async function acquire(chatKey: string, options: Options, resume: string | null): Promise<{ session: Session; source: SessionSource }> {
  const key = fingerprintOptions(options);
  const existing = live.get(chatKey);
  if (existing && !existing.closed && existing.key === key) {
    existing.lastUsedAt = Date.now();
    return { session: existing, source: 'live' };
  }
  if (existing) closeFor(chatKey);

  if (warm && warm.key === warmKey(options, resume) && !warm.session.closed) {
    const session = warm.session;
    if (warmTimer) clearTimeout(warmTimer);
    warmTimer = null;
    warm = null;
    session.chatKey = chatKey;
    session.lastUsedAt = Date.now();
    live.set(chatKey, session);
    trim();
    return { session, source: 'warm' };
  }

  const session = await spawn(options, resume);
  session.chatKey = chatKey;
  live.set(chatKey, session);
  trim();
  return { session, source: 'cold' };
}

/** Push one user turn into a session and wait for its result frame. */
export function runTurn(session: Session, text: string, handle: (m: SDKMessage) => void): Promise<void> {
  return new Promise<void>((resolve, reject) => {
    session.turn = {
      handle,
      finish: (error) => (error ? reject(error) : resolve()),
    };
    session.lastUsedAt = Date.now();
    session.input.push(text);
  });
}
