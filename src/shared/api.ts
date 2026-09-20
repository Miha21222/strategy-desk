// Types shared by the main process, the preload bridge and the renderer.
// Keep this file free of runtime imports from either side.
import type { Grade, StressReport } from './blocks';
import type { PanelDayState, PanelDef, TvCallResult } from './panel';
import type { PreflightResult } from './preflight';

export type { PanelDayState, PanelDef, TvCallResult, PreflightResult };

export type ChatKind = 'chat' | 'strategy' | 'indicator' | 'performance' | 'journal' | 'ib-day';
export const CHAT_KINDS: ChatKind[] = ['chat', 'strategy', 'indicator', 'performance', 'journal', 'ib-day'];

export interface ChatMeta {
  key: string;
  /** Claude Code session id; set after the first successful turn. */
  sessionId: string | null;
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
export type ChatMode = 'saver' | 'full';

/** Settings as seen by the renderer. Secrets never cross the bridge; only presence flags do. */
export interface Settings {
  authMode: AuthMode;
  hasApiKey: boolean;
  hasOauthToken: boolean;
  tvmcpPath: string;
  /** Repository the preflight's `git clone` fix names; the app never runs it. */
  tvmcpRepo: string;
  cdpPort: number;
  toolsets: string;
  officialMcp: boolean;
  model: string;
  effort: EffortLevel;
  permissionPreset: PermissionPreset;
  theme: Theme;
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
  /** null = check not finished / uv missing; see tvmcpError. */
  tvmcpOk: boolean | null;
  tvmcpTools: number;
  tvmcpError: string;
  claude: ClaudeAuthStatus;
  checkedAt: number;
}

export interface AgentInit {
  model: string;
  claudeCodeVersion: string;
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
export type ChatEvent = { chatKey: string } & (
  | { kind: 'text'; delta: string }
  | { kind: 'thinking'; delta: string }
  | { kind: 'tool'; id: string; name: string; input: string }
  | { kind: 'tool-result'; id: string; ok: boolean; text: string }
  | { kind: 'init'; init: AgentInit }
  | { kind: 'done'; tokens: TokenUsage; costUsd: number | null; sessionId: string | null; turns: number; stopped: boolean }
  | { kind: 'error'; message: string }
  | { kind: 'notice'; message: string }
);

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
export type PickKind = 'tvmcp-dir' | 'risk-rules' | 'journal' | 'field-notes';

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
    onEvent(cb: (ev: ChatEvent) => void): () => void;
  };
  open: { external(url: string): Promise<boolean> };
  app: { version(): Promise<string> };
}

declare global {
  interface Window {
    desk: DeskApi;
  }
}
