// Long-running operations the user starts with one press: provisioning the
// bundled Python runtime, downloading the headless browser, seeding the course
// knowledge base and fetching its embedding model, registering and
// authenticating the official TradingView MCP.
//
// Not all of them spawn a process — `kb-import` copies ~55 MB out of the
// installation directory into userData — but they all take long enough to need
// a progress view, and they all go through the same runner for that reason.
//
// The rule the whole feature keeps: the RENDERER NAMES A TASK ID, NEVER A
// COMMAND. The command is assembled in the main process from this closed list,
// exactly like `tv:call` and its tool allowlist — a compromised renderer can
// ask for "provision-runtime" and nothing else.
//
// The second rule: nothing here starts at launch. The app may install,
// download and log in — but only from the wizard or the diagnosis, which show
// what will run and stream the output. The wizard page presses for the user
// on the tasks in `AUTO_TASKS` (renderer/setup.ts): laying down what the
// installer carried; a login or a write into the user's environment waits
// for the button.
export const TASK_IDS = [
  'provision-runtime',
  'install-chromium',
  'kb-import',
  'kb-model',
  'official-mcp-add',
  'official-mcp-login',
  // The Codex engine: the app's own CODEX_HOME starts logged out, and the
  // official TradingView MCP needs its own OAuth there — Claude Code's store
  // is a different store.
  'codex-auth-import',
  'codex-mcp-add',
  'codex-mcp-login',
  // The macro-review MVP on another trader's PC: lay the vault template down
  // in the chosen folder, prove the TradingView sessionid works, and register
  // or remove the three Task Scheduler jobs (EU 09:30 / US 16:00 / review 20:00).
  'vault-create',
  'session-check',
  'scheduler-register',
  'scheduler-unregister',
  // The bundled Claude Code binary's own OAuth login (`claude auth login`):
  // opens the browser, waits for the redirect, writes ~/.claude/.credentials.json
  // — the same file the Agent SDK reads. The one press a fresh install needs.
  'claude-login',
] as const;
export type TaskId = (typeof TASK_IDS)[number];

export function isTaskId(x: unknown): x is TaskId {
  return typeof x === 'string' && (TASK_IDS as readonly string[]).includes(x);
}

/** Output kept per task. Old lines are dropped: this is a progress view, not a log file. */
export const MAX_TASK_LINES = 300;

export interface TaskState {
  id: TaskId;
  running: boolean;
  /** null = never finished in this app run. */
  ok: boolean | null;
  lines: string[];
  error: string;
  startedAt: number;
  finishedAt: number;
}

export type TaskEvent =
  | { id: TaskId; kind: 'start'; at: number; command: string }
  | { id: TaskId; kind: 'line'; text: string }
  | { id: TaskId; kind: 'end'; at: number; ok: boolean; error: string };

export function emptyTask(id: TaskId): TaskState {
  return { id, running: false, ok: null, lines: [], error: '', startedAt: 0, finishedAt: 0 };
}

/** Split whatever a child process wrote into display lines, tidied and capped. */
export function splitLines(chunk: string): string[] {
  return String(chunk ?? '')
    .split(/\r?\n/)
    .map((l) => l.replace(/\r/g, '').trimEnd())
    .filter((l) => l.trim().length > 0)
    .map((l) => l.slice(0, 400));
}

/** Pure reducer: the renderer and the main process keep the same state the same way. */
export function applyTaskEvent(prev: TaskState, ev: TaskEvent): TaskState {
  if (ev.id !== prev.id) return prev;
  if (ev.kind === 'start') return { ...prev, running: true, ok: null, error: '', lines: [], startedAt: ev.at, finishedAt: 0 };
  if (ev.kind === 'line') {
    const lines = [...prev.lines, ev.text];
    return { ...prev, lines: lines.length > MAX_TASK_LINES ? lines.slice(lines.length - MAX_TASK_LINES) : lines };
  }
  return { ...prev, running: false, ok: ev.ok, error: ev.error, finishedAt: ev.at };
}
