// Types shared by the main process, the preload bridge and the renderer.
// Keep this file free of runtime imports from either side.
import type { Grade, StressReport } from './blocks';
import type { PanelDayState, PanelDef, TvCallResult } from './panel';
import type { PreflightResult } from './preflight';
import type { TaskEvent, TaskId, TaskState } from './tasks';
import type { EngineId } from './engine';
import type { DayAnswers, HumanRead, HumanSide, MacroDay, MacroIndex, Region } from './macro';

export type { PanelDayState, PanelDef, TvCallResult, PreflightResult, TaskEvent, TaskId, TaskState, EngineId };

/**
 * `macro` is the shelf of the macro-review MVP: one chat per day that runs the
 * owner's `macro-review` skill inside the vault (see `macroSpec` in agent.ts).
 */
export type ChatKind = 'chat' | 'strategy' | 'indicator' | 'performance' | 'journal' | 'ib-day' | 'macro';
export const CHAT_KINDS: ChatKind[] = ['chat', 'strategy', 'indicator', 'performance', 'journal', 'ib-day', 'macro'];

export interface ChatMeta {
  key: string;
  /**
   * The engine's own conversation id (a Claude Code session, a Codex thread);
   * set after the first successful turn and resumed on the next one.
   */
  sessionId: string | null;
  /**
   * Engine this chat was started on. Conversations do not move between engines
   * — the resumed id belongs to one of them — so a chat keeps the engine it
   * was opened with. Absent on chats created before stage 6 = `claude`.
   */
  engine?: EngineId;
  title: string;
  createdAt: number;
  updatedAt: number;
  pinned: boolean;
  kind: ChatKind;
  tokens: { in: number; out: number };
  costUsd: number;
  /** Workflow whose prompt block was frozen into this chat's system prompt (null = none). */
  workflowId?: string | null;
  /** sha256 prefix of the frozen system prompt; the text lives in userData/prompts/<hash>.md. */
  promptHash?: string | null;
  /** Structured reports captured from `stressreport` blocks, newest last (≤ MAX_REPORTS). */
  reports?: StoredReport[];
}

export const MAX_REPORTS = 20;

export interface StoredReport {
  id: string;
  kind: 'stressreport';
  version: string;
  grade: Grade;
  strategy: string;
  createdAt: number;
  data: StressReport;
}

export type NewReport = Omit<StoredReport, 'id' | 'createdAt'>;

/** A workflow = prompt block + quick prompts + report catalog, loaded from workflows/<id>/workflow.yaml. */
export interface WorkflowPrompt {
  id: string;
  label: string;
  prompt: string;
}

export interface WorkflowDef {
  id: string;
  title: string;
  description: string;
  /** Contents of the promptBlock file (English), appended to the system prompt. */
  prompt: string;
  quickPrompts: WorkflowPrompt[];
  reports: WorkflowPrompt[];
  shelves: ChatKind[];
  /** Declares a day panel (session watch / risk desk); absent = no «День» tab content. */
  panel?: PanelDef | null;
}

export type AuthMode = 'subscription' | 'api-key' | 'setup-token';
export type EffortLevel = '' | 'low' | 'medium' | 'high' | 'xhigh' | 'max';
export type PermissionPreset = 'standard' | 'extended';
export type Theme = 'system' | 'dark' | 'light';
/** Language the copilot must answer in; 'auto' = follow the user's own message. */
export type AnswerLang = 'auto' | 'ru' | 'en';
export type ChatMode = 'saver' | 'full';

/** Settings as seen by the renderer. Secrets never cross the bridge; only presence flags do. */
export interface Settings {
  /** Which agent engine runs the chat: the Claude Agent SDK or the Codex CLI. */
  engine: EngineId;
  /** How to start the Codex CLI; empty = `codex` from PATH. */
  codexPath: string;
  authMode: AuthMode;
  hasApiKey: boolean;
  hasOauthToken: boolean;
  tvmcpPath: string;
  /** Repository the preflight's `git clone` fix names; the app never runs it. */
  tvmcpRepo: string;
  cdpPort: number;
  toolsets: string;
  officialMcp: boolean;
  /** Offer the bundled course knowledge base to the model as the `gt-database` server. */
  knowledgeBase: boolean;
  /** A gt-rag checkout for a dev run; ignored once the installer carries the knowledge base. */
  gtRagPath: string;
  model: string;
  effort: EffortLevel;
  permissionPreset: PermissionPreset;
  theme: Theme;
  /** Zoom factor of the whole window (Electron webContents zoom), 0.8-2.0. */
  uiScale: number;
  /** Language rule appended to the system prompt every turn. */
  answerLang: AnswerLang;
  tvDesktopExe: string;
  /** Sidebar sections collapsed state, persisted. */
  sidebar: Record<string, boolean>;
  /** Workflow whose quick prompts fill the sidebar and whose prompt block new chats inherit. */
  activeWorkflow: string;
  /** Per-shelf workflow override: a chat opened on that shelf inherits this workflow. */
  shelfWorkflows: Partial<Record<ChatKind, string>>;
  /** Path to the user's own risk rules JSON handed to tv_risk_guard (never numbers in code). */
  riskRulesPath: string;
  /** Path to the user's trade journal handed to tv_journal_load / tv_risk_guard. */
  journalPath: string;
  /** Override for data/copilot-notes.md — the "Live field notes" prompt section. */
  fieldNotesPath: string;
  /** When the setup wizard was finished; 0 = never, so the wizard opens on start. */
  setupCompletedAt: number;
  /** Wizard steps the user chose to skip; the affected feature says why it is limited. */
  setupSkipped: string[];
  /**
   * The owner's Obsidian vault folder (the one holding `indices-kb/` and
   * `.claude/skills/macro-review`). The macro tab reads its state files, and a
   * macro chat runs Claude Code with this folder as its working directory.
   */
  vaultPath: string;
  /**
   * Show the pre-MVP tabs (День, Воркфлоу) and the workflow quick prompts.
   * Off by default since the macro-review MVP; the code stays, only hidden.
   */
  legacyUi: boolean;
}

export type SettingsPatch = Partial<Omit<Settings, 'hasApiKey' | 'hasOauthToken'>>;

export interface TraderProfile {
  callName: string;
  traderType: string;
  markets: string;
  style: string;
  riskRules: string;
  propFirms: string;
  morningChecklist: string[];
}

export type ClaudeAuthStatus = 'subscription' | 'api-key' | 'setup-token' | 'none';

export interface Status {
  tvRunning: boolean;
  cdpUp: boolean;
  chartTab: boolean;
  /** Non-empty when the port answers but the owner is NOT TradingView; names it. */
  cdpOwner: string;
  /** null = check not finished / uv missing; see tvmcpError. */
  tvmcpOk: boolean | null;
  tvmcpTools: number;
  tvmcpError: string;
  claude: ClaudeAuthStatus;
  checkedAt: number;
}

export interface AgentInit {
  model: string;
  /** Version of the engine runtime that answered (Claude Code, the Codex CLI). */
  engineVersion: string;
  mcpServers: { name: string; status: string }[];
  toolCount: number;
  sessionId: string;
}

export interface TokenUsage {
  in: number;
  out: number;
  cacheRead: number;
  cacheWrite: number;
}

/** Streamed over the single `chat:event` IPC channel. */
/**
 * One thing that happened during a turn, without saying whose turn it was.
 * Engine drivers (`src/main/drivers`) speak this: whatever the engine's own
 * protocol looks like, the renderer only ever sees these.
 */
export type ChatEventBody =
  | { kind: 'text'; delta: string }
  | { kind: 'thinking'; delta: string }
  | { kind: 'tool'; id: string; name: string; input: string }
  | { kind: 'tool-result'; id: string; ok: boolean; text: string }
  | { kind: 'init'; init: AgentInit }
  | { kind: 'done'; tokens: TokenUsage; costUsd: number | null; sessionId: string | null; turns: number; stopped: boolean }
  | { kind: 'error'; message: string }
  | { kind: 'notice'; message: string };

export type ChatEvent = { chatKey: string } & ChatEventBody;

/** Persisted transcript entries (renderer-facing shape, not the model context). */
export type TranscriptItem =
  | { role: 'user'; text: string; at: number }
  | { role: 'assistant'; text: string; at: number; tools: ToolCallItem[]; thinking: boolean };

export interface ToolCallItem {
  id: string;
  name: string;
  input: string;
  ok: boolean | null;
  result: string;
}

export interface TvLaunchResult {
  ok: boolean;
  error?: 'running-without-flag' | 'exe-not-found' | 'timeout' | 'spawn-failed' | 'unsupported-platform';
  detail?: string;
}

export interface SendArgs {
  chatKey: string;
  text: string;
  mode: ChatMode;
}

/** What `dialog.pick` should open; the main process owns the filters. */
export type PickKind = 'tvmcp-dir' | 'gt-rag-dir' | 'risk-rules' | 'journal' | 'field-notes' | 'vault-dir';

export interface DeskApi {
  status: { get(): Promise<Status> };
  /** The full setup diagnosis; never throws, nothing is cached between runs. */
  preflight: { run(): Promise<PreflightResult> };
  /** A native open dialog; returns '' when the user cancels. Never writes anything. */
  dialog: { pick(kind: PickKind): Promise<string> };
  tv: {
    launch(): Promise<TvLaunchResult>;
    /**
     * Deterministic tvmcp call through the `tv` CLI — no model turn, no tokens.
     * `name` must be in TV_TOOL_ALLOWLIST; anything else is refused in the main
     * process, so the renderer can never run an arbitrary tool.
     */
    call(name: string, args: Record<string, unknown>): Promise<TvCallResult>;
  };
  settings: {
    get(): Promise<Settings>;
    set(patch: SettingsPatch): Promise<Settings>;
    setSecret(name: 'apiKey' | 'oauthToken', value: string): Promise<Settings>;
    clearSecret(name: 'apiKey' | 'oauthToken'): Promise<Settings>;
  };
  /**
   * The four operations the app may perform for the user, each on one press:
   * provisioning the bundled Python runtime, fetching the headless browser,
   * registering and authenticating the official TradingView MCP.
   *
   * The renderer names a task id and gets a state back; the command lives in
   * the main process. `command()` is what the UI shows before the press, so
   * nothing is ever run without the user seeing what it is.
   */
  tasks: {
    run(id: TaskId): Promise<TaskState>;
    stop(id: TaskId): void;
    state(id: TaskId): Promise<TaskState>;
    list(): Promise<TaskState[]>;
    command(id: TaskId): Promise<string>;
    onEvent(cb: (ev: TaskEvent) => void): () => void;
  };
  /**
   * Delete the provisioned Python environment (~600 MB in userData). The
   * uninstaller leaves it behind on purpose — it sits next to the chats and
   * journals it must not touch — so this is the only way to reclaim it.
   */
  runtime: { clear(): Promise<{ removed: string }> };
  /** The prop-firm rule book (data/prop-firms.json); null when it is missing. */
  rules: { get(): Promise<unknown | null> };
  profile: { get(): Promise<TraderProfile>; set(p: TraderProfile): Promise<TraderProfile> };
  chats: {
    list(): Promise<ChatMeta[]>;
    create(kind?: ChatKind, workflowId?: string | null): Promise<ChatMeta>;
    open(key: string): Promise<{ chat: ChatMeta; messages: TranscriptItem[] } | null>;
    rename(key: string, title: string): Promise<ChatMeta[]>;
    delete(key: string): Promise<ChatMeta[]>;
    pin(key: string, pinned: boolean): Promise<ChatMeta[]>;
    setKind(key: string, kind: ChatKind): Promise<ChatMeta[]>;
    addReport(key: string, report: NewReport): Promise<ChatMeta[]>;
    deleteReport(key: string, reportId: string): Promise<ChatMeta[]>;
  };
  workflows: { list(): Promise<WorkflowDef[]> };
  /** Per workflow + date panel state, so a restart reattaches instead of restarting. */
  panel: {
    get(workflowId: string, date: string): Promise<PanelDayState | null>;
    set(state: PanelDayState): Promise<PanelDayState>;
    clear(workflowId: string, date: string): Promise<void>;
  };
  chat: {
    send(args: SendArgs): void;
    stop(chatKey: string): void;
    /** Start the session this chat's next turn will need, before the text exists. */
    prewarm(args: { chatKey: string | null; mode: ChatMode }): void;
    onEvent(cb: (ev: ChatEvent) => void): () => void;
  };
  /**
   * The macro-review files in the vault, read-only. `onChanged` fires with the
   * dates whose state files moved (a run from the desk, the Task Scheduler or
   * a terminal — the desk does not care who wrote them).
   */
  macro: {
    index(): Promise<MacroIndex>;
    day(date: string): Promise<MacroDay>;
    /** Open a note under `Макро-обзоры/` in the default app (Obsidian); false when refused. */
    openNote(rel: string): Promise<boolean>;
    /**
     * A chart screenshot (`MacroShot.path`, a PNG under `indices-kb/shots/`) as a
     * `data:image/png;base64,…` URL; null when it is missing or refused.
     */
    image(rel: string): Promise<string | null>;
    /**
     * The owner's own two files (note 23) — the only writes the desk makes
     * into the vault. His read of a window, given before the gauge is shown
     * (`beforeScale`: no gauge was on screen), and his two evening answers.
     * Each returns what was written, or null when refused.
     */
    humanRead(args: { date: string; region: Region; side: HumanSide; beforeScale: boolean }): Promise<HumanRead | null>;
    dayAnswers(args: { date: string; regions: DayAnswers['regions']; note: string }): Promise<DayAnswers | null>;
    onChanged(cb: (dates: string[]) => void): () => void;
  };
  /** Applies the window zoom factor (settings.uiScale); returns the clamped value. */
  ui: { zoom(factor: number): Promise<number> };
  open: { external(url: string): Promise<boolean> };
  app: { version(): Promise<string> };
}

declare global {
  interface Window {
    desk: DeskApi;
  }
}
