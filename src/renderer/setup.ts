// The setup wizard's own logic, kept out of the component so it can be tested
// without a DOM: which checks belong to which step, what "skipped" means and
// how it survives a restart.
//
// Skipping is a real answer, not a silent one: the step is remembered in
// `settings.setupSkipped`, and every feature it weakens says so (the day panel
// explains why it runs on the shipped example rules, the sidebar says why the
// lights are red) instead of failing mysteriously an hour later.
import type { PickKind, Settings, SettingsPatch } from '@shared/api';
import { worstStatus, type CheckId, type CheckStatus, type PathField, type PreflightCheck } from '@shared/preflight';

export type StepId = 'claude' | 'tvmcp' | 'tradingview' | 'files';

export interface WizardStep {
  id: StepId;
  /** Checks shown on this step, in list order. */
  checks: CheckId[];
}

/** Claude first (nothing works without it), then the toolchain, then the chart, then the files. */
export const WIZARD_STEPS: WizardStep[] = [
  { id: 'claude', checks: ['claude'] },
  { id: 'tvmcp', checks: ['uv', 'tvmcpClone', 'tvmcpBoot'] },
  { id: 'tradingview', checks: ['tvDesktop', 'cdpPort'] },
  { id: 'files', checks: ['workflows', 'riskRules', 'journal'] },
];

export const STEP_IDS: StepId[] = WIZARD_STEPS.map((s) => s.id);

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

export function nextStep(id: StepId): StepId | null {
  const i = STEP_IDS.indexOf(id);
  return i >= 0 && i + 1 < STEP_IDS.length ? STEP_IDS[i + 1] : null;
}

export function prevStep(id: StepId): StepId | null {
  const i = STEP_IDS.indexOf(id);
  return i > 0 ? STEP_IDS[i - 1] : null;
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

/** Which native dialog a `pick-file` fix opens for each settings field. */
export const PICK_FOR_FIELD: Record<PathField, PickKind> = {
  tvmcpPath: 'tvmcp-dir',
  riskRulesPath: 'risk-rules',
  journalPath: 'journal',
  fieldNotesPath: 'field-notes',
};
