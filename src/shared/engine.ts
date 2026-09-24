// The engines the chat can run on, and what each one can actually do.
//
// The app is built around the Claude Agent SDK, but the whole domain layer —
// the day panel, the deterministic `tv` CLI, analytics, workflows, cards — sits
// below `tv:call` and does not care which model answers. What differs is the
// shell around a turn, and those differences are facts, not preferences: the UI
// asks this table instead of branching on the engine id, so a new engine is a
// row here plus a driver in `src/main/drivers`.
export const ENGINE_IDS = ['claude', 'codex'] as const;
export type EngineId = (typeof ENGINE_IDS)[number];

export interface EngineCapabilities {
  /**
   * The process survives between turns, so the MCP servers boot once per chat
   * rather than once per message. tvmcp needs ~3.5 s to come up; an engine
   * without this pays that on every send and cannot be prewarmed.
   */
  persistentSession: boolean;
  /** A real system prompt is handed to the engine (not smuggled into the first user message). */
  systemPrompt: boolean;
  /** Tools can be forbidden by name. Without it the shell tool can only be sandboxed, never removed. */
  toolDenyList: boolean;
  /** The engine reports what a turn cost in USD. On a subscription this is usually false. */
  costReported: boolean;
  /** Reasoning is streamed as its own channel and can be shown live. */
  thinkingStream: boolean;
  /** Ships inside the installer; false = an external dependency the preflight only detects. */
  bundled: boolean;
}

export const ENGINES: Record<EngineId, EngineCapabilities> = {
  claude: {
    persistentSession: true,
    systemPrompt: true,
    toolDenyList: true,
    costReported: true,
    thinkingStream: true,
    bundled: true,
  },
  // Filled in with what `codex app-server` actually gives us (stage 6.1); the
  // pessimistic entries are the ones measured on 2026-09-23: no tool deny list
  // (only a sandbox), and no USD figure on a ChatGPT subscription.
  codex: {
    persistentSession: true,
    systemPrompt: true,
    toolDenyList: false,
    costReported: false,
    thinkingStream: true,
    bundled: false,
  },
};

export function isEngineId(x: unknown): x is EngineId {
  return typeof x === 'string' && (ENGINE_IDS as readonly string[]).includes(x);
}

export const DEFAULT_ENGINE: EngineId = 'claude';

/** The engine a setting names, falling back to the default rather than throwing. */
export function engineOf(value: unknown): EngineId {
  return isEngineId(value) ? value : DEFAULT_ENGINE;
}
