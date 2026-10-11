// The seam between the chat and whichever agent engine answers it.
//
// `agent.ts` owns everything that is true for every engine: the system prompt,
// the transcript, the token and cost tallies, the retry after a crash, and the
// `ChatEvent` stream the renderer listens to. A driver owns everything that is
// true for one engine: how a process is started and kept alive, how a turn is
// pushed into it, and how its own frames become `ChatEventBody`.
//
// Nothing engine-specific may leak through this file — no SDK types, no CLI
// flags. If something has to cross, it belongs in `TurnSpec`.
import type { ChatEventBody, EffortLevel, PermissionPreset, TokenUsage } from '@shared/api';
import type { EngineId } from '@shared/engine';
import type { ServerMap } from '../mcp';

/**
 * Everything a turn needs, in engine-neutral terms. Two specs that are equal
 * may share one live session; that is what the fingerprint is for.
 */
export interface TurnSpec {
  engine: EngineId;
  /** Frozen prompt plus the language and mode suffixes, already composed. */
  systemPrompt: string;
  /** Empty = the engine's own default. */
  model: string;
  /** Empty = the engine's own default. */
  effort: EffortLevel;
  /** Working directory of the chat (the model's file tools are scoped to it). */
  cwd: string;
  /** MCP servers in the app's own shape; the driver translates. */
  mcpServers: ServerMap;
  /** What the model may do; each engine expresses it with what it has. */
  permissionPreset: PermissionPreset;
  /**
   * `macro`: the turn runs the owner's own Claude Code setup inside the vault —
   * its CLAUDE.md, skills and MCP servers — instead of the desk's isolated one.
   * `systemPrompt` is then appended to Claude Code's preset rather than
   * replacing it, because the skills need the preset to be found and run.
   * Claude-only: a macro chat never runs on another engine.
   */
  profile?: 'macro';
}

/**
 * What a driver reports while a turn runs. Everything except `result` is a
 * `ChatEvent` minus its `chatKey`; `result` is the turn's own accounting,
 * which `agent.ts` folds into the chat before sending `done`.
 */
export type DriverEvent =
  | ChatEventBody
  | { kind: 'result'; sessionId: string | null; tokens: TokenUsage; costUsd: number | null; turns: number };

/** Where the session serving this turn came from — dev logging only. */
export type SessionSource = 'live' | 'warm' | 'cold';

export interface RunArgs {
  chatKey: string;
  spec: TurnSpec;
  /** The engine's conversation id to resume, or null for a fresh one. */
  resume: string | null;
  text: string;
  emit: (ev: DriverEvent) => void;
  /** Tail of the engine process's stderr, used to explain a failed turn. */
  onStderr?: (data: string) => void;
}

export interface EngineDriver {
  id: EngineId;
  /** Identity of a session that could serve this spec; always starts with the engine id. */
  fingerprint(spec: TurnSpec): string;
  /** True when this chat already holds a usable session for this spec. */
  hasLive(chatKey: string, spec: TurnSpec): boolean;
  /** Start a session before there is any text. Best effort: failure only costs a slower turn. */
  prewarm(spec: TurnSpec, resume: string | null): Promise<void>;
  /** Run one turn, resolving when the engine reported its result. */
  runTurn(args: RunArgs): Promise<{ source: SessionSource }>;
  /** Interrupt the turn in flight, if any. */
  stop(chatKey: string): Promise<void>;
  /** Drop this chat's session (it crashed, or the chat is gone). */
  closeFor(chatKey: string): void;
  /** Drop every session this driver holds. */
  closeAll(): void;
  /** True when a turn died without producing anything, so one retry is fair. */
  isCrashError(e: unknown): boolean;
}
