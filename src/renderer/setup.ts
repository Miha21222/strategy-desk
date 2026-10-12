// The setup wizard's own logic, kept out of the component so it can be tested
// without a DOM: which checks belong to which step, what "skipped" means and
// how it survives a restart.
//
// Skipping is a real answer, not a silent one: the step is remembered in
// `settings.setupSkipped`, and every feature it weakens says so (the day panel
// explains why it runs on the shipped example rules, the sidebar says why the
// lights are red) instead of failing mysteriously an hour later.
import type { PickKind, Settings, SettingsPatch } from '@shared/api';
import { worstStatus, type CheckId, type CheckStatus, type PathField, type PreflightCheck, type PreflightFix } from '@shared/preflight';
import type { TaskId, TaskState } from '@shared/tasks';

export type StepId = 'vault' | 'claude' | 'codex' | 'tvmcp' | 'knowledge' | 'official' | 'tradingview' | 'session' | 'schedule' | 'files';

export interface WizardStep {
  id: StepId;
  /** Checks shown on this step, in list order. */
  checks: CheckId[];
}

/**
 * Claude first (nothing works without it), then the market server, then the
 * optional official MCP, then the chart, then the files.
 *
 * The tvmcp step lists both shapes of the same question: `uv` and `tvmcpClone`
 * exist only in a development run, `tvmcpRuntime` only in a packaged one, and
 * `checksOfStep` drops whichever the diagnosis did not produce — so the step
 * shows two rows or three, never an empty one.
 */
export const WIZARD_STEPS: WizardStep[] = [
  // The macro-review MVP starts with the folder everything else lives in.
  { id: 'vault', checks: ['vault'] },
  { id: 'claude', checks: ['claude'] },
  // Only the selected engine produces checks, so exactly one of these two
  // steps has rows; `visibleStepIds` drops the other one entirely.
  { id: 'codex', checks: ['codex', 'codexAuth'] },
  { id: 'tvmcp', checks: ['uv', 'tvmcpClone', 'tvmcpRuntime', 'tvmcpBoot', 'chromium', 'toolsPython'] },
  { id: 'knowledge', checks: ['knowledgeBase', 'kbModel'] },
  { id: 'official', checks: ['officialMcp', 'codexOfficialMcp'] },
  { id: 'tradingview', checks: ['tvDesktop', 'cdpPort'] },
  { id: 'session', checks: ['tvSession'] },
  { id: 'schedule', checks: ['scheduler', 'timezone'] },
  { id: 'files', checks: ['workflows', 'riskRules', 'journal'] },
];

export const STEP_IDS: StepId[] = WIZARD_STEPS.map((s) => s.id);

/**
 * The steps the macro-review MVP hides: the second engine, the course, the
 * official MCP and the Strategy Lab's files are the legacy desk's business
 * (`settings.legacyUi`). Their checks still run and still show in Settings →
 * «Диагностика»; only the wizard pages go.
 */
export const LEGACY_ONLY_STEPS: StepId[] = ['codex', 'knowledge', 'official', 'files'];

/** The step order of this install: six pages for the MVP, every page for the legacy desk. */
export function stepIdsFor(settings: Pick<Settings, 'legacyUi'>): StepId[] {
  return settings.legacyUi ? STEP_IDS : STEP_IDS.filter((id) => !LEGACY_ONLY_STEPS.includes(id));
}

/**
 * The steps this diagnosis actually has something to say about. The engine the
 * user did not choose produces no checks, and a wizard page with an empty list
 * (and a "skip" button for nothing) would be worse than no page.
 *
 * Before the first diagnosis arrives there is nothing to filter on, so every
 * step is shown rather than none.
 */
export function visibleStepIds(checks: PreflightCheck[], ids: StepId[] = STEP_IDS): StepId[] {
  if (!checks.length) return ids;
  return ids.filter((id) => checksOfStep(id, checks).length > 0);
}

/** Every check id appears on exactly one step; a new check must be placed. */
export function checksOfStep(id: StepId, checks: PreflightCheck[]): PreflightCheck[] {
  const wanted = WIZARD_STEPS.find((s) => s.id === id)?.checks ?? [];
  return wanted.map((c) => checks.find((x) => x.id === c)).filter((c): c is PreflightCheck => Boolean(c));
}

export function stepStatus(id: StepId, checks: PreflightCheck[]): CheckStatus {
  const own = checksOfStep(id, checks);
  return own.length ? worstStatus(own) : 'unknown';
}

/** A step is done when nothing on it failed; a warning is good enough to move on. */
export function stepDone(id: StepId, checks: PreflightCheck[]): boolean {
  const s = stepStatus(id, checks);
  return s === 'ok' || s === 'warn';
}

export function nextStep(id: StepId, ids: StepId[] = STEP_IDS): StepId | null {
  const i = ids.indexOf(id);
  return i >= 0 && i + 1 < ids.length ? ids[i + 1] : null;
}

export function prevStep(id: StepId, ids: StepId[] = STEP_IDS): StepId | null {
  const i = ids.indexOf(id);
  return i > 0 ? ids[i - 1] : null;
}

// ---------- skipped steps ----------

export function isSkipped(settings: Pick<Settings, 'setupSkipped'>, id: StepId): boolean {
  return (settings.setupSkipped ?? []).includes(id);
}

/** Add or remove a step id, keeping the list unique, ordered and free of junk. */
export function applySkip(skipped: readonly string[], id: StepId, on: boolean): string[] {
  const set = new Set((skipped ?? []).filter((s): s is StepId => STEP_IDS.includes(s as StepId)));
  if (on) set.add(id);
  else set.delete(id);
  return STEP_IDS.filter((s) => set.has(s));
}

/**
 * A step the user skipped but which is green now is no longer "skipped" — the
 * warning would be a lie, and the user would never find the switch to clear it.
 */
export function reconcileSkips(skipped: readonly string[], checks: PreflightCheck[]): string[] {
  return STEP_IDS.filter((id) => (skipped ?? []).includes(id) && stepStatus(id, checks) !== 'ok');
}

/** The wizard opens by itself only on a truly fresh install. */
export function shouldOpenWizard(settings: Pick<Settings, 'setupCompletedAt'>): boolean {
  return !settings.setupCompletedAt;
}

/** Finishing: remember when, and which steps were left unfinished. */
export function finishPatch(skipped: readonly string[], now = Date.now()): SettingsPatch {
  return { setupCompletedAt: now, setupSkipped: applySkipAll(skipped) };
}

function applySkipAll(skipped: readonly string[]): string[] {
  return STEP_IDS.filter((s) => (skipped ?? []).includes(s));
}

/**
 * i18n keys for the "this is limited because you skipped it" notes, in step
 * order. The renderer renders them where the feature lives.
 */
export function limitationKeys(settings: Pick<Settings, 'setupSkipped'>): string[] {
  return STEP_IDS.filter((id) => isSkipped(settings, id)).map((id) => `setup.limited.${id}`);
}

// ---------- what a step starts by itself ----------

/**
 * The tasks a wizard page starts the moment it is shown, without a press: the
 * ones that only lay down what the installer carried (the Python environment,
 * the headless browser) or re-check a secret already stored. The vault is NOT
 * here: its folder is the owner's choice, so the page asks (the path, a picker,
 * «Создать здесь») before anything is written. The page still shows the command and streams the output — the rule
 * stays "the renderer names an id", it only presses for the user.
 *
 * Also not here, on purpose: the Claude login (opens a browser), registering
 * the schedule (writes the sessionid into the user's environment) and anything
 * that removes something. Those wait for the button.
 */
export const AUTO_TASKS: readonly TaskId[] = ['provision-runtime', 'install-chromium', 'session-check'];

export type RunTaskFix = Extract<PreflightFix, { kind: 'run-task' }>;

/**
 * The ONE `run-task` fix of a step that should start now (first in page
 * order): in AUTO_TASKS, with a command to run (an empty command means the
 * plan refused — a development run without the payload), not running, never
 * finished in this app run (`ok === null`), and not started by this wizard
 * session before — a task that failed is retried by the user, never in a loop.
 *
 * One at a time on purpose: the tvmcp page carries both the runtime and the
 * browser, and the browser install needs the runtime's Python. Each finish
 * re-runs the diagnosis, the new result re-runs the effect, the next starts.
 */
export function autoFixes(
  step: StepId,
  checks: PreflightCheck[],
  tasks: Partial<Record<TaskId, TaskState>> | undefined,
  started: ReadonlySet<TaskId>,
  commands?: Partial<Record<TaskId, string>>,
): RunTaskFix[] {
  // While anything runs, nothing else starts: the next one waits for the re-probe.
  if (Object.values(tasks ?? {}).some((st) => st?.running)) return [];
  for (const c of checksOfStep(step, checks)) {
    const fix = c.fix;
    if (!fix || fix.kind !== 'run-task' || !AUTO_TASKS.includes(fix.task) || started.has(fix.task)) continue;
    if (commands && fix.task in commands && !commands[fix.task]) continue;
    const st = tasks?.[fix.task];
    if (st?.running || (st && st.ok !== null)) continue;
    return [fix];
  }
  return [];
}

/** The task ids a step's checks carry as `run-task` fixes (for "started by itself" notes). */
export function stepTaskIds(step: StepId, checks: PreflightCheck[]): TaskId[] {
  return checksOfStep(step, checks)
    .map((c) => c.fix)
    .filter((f): f is RunTaskFix => Boolean(f && f.kind === 'run-task'))
    .map((f) => f.task);
}

/** Which native dialog a `pick-file` fix opens for each settings field. */
export const PICK_FOR_FIELD: Record<PathField, PickKind> = {
  tvmcpPath: 'tvmcp-dir',
  gtRagPath: 'gt-rag-dir',
  riskRulesPath: 'risk-rules',
  journalPath: 'journal',
  fieldNotesPath: 'field-notes',
  vaultPath: 'vault-dir',
};

/** The three scheduled runs, as the «Расписание» step lists them (Kyiv time, Mon–Fri). */
export const SCHEDULE_TIMES: { region: 'eu' | 'us' | 'review'; at: string }[] = [
  { region: 'eu', at: '09:30' },
  { region: 'us', at: '16:00' },
  { region: 'review', at: '20:00' },
];
