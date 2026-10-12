// The wizard's own logic: which checks belong to which step, and what
// "skipped" means — including that it survives closing the wizard and a
// restart, because a forgotten skip is exactly the silent failure this
// feature exists to remove.
import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS } from '@shared/defaults';
import { CHECK_IDS, type PreflightCheck } from '@shared/preflight';
import {
  applySkip,
  checksOfStep,
  finishPatch,
  isSkipped,
  limitationKeys,
  nextStep,
  PICK_FOR_FIELD,
  prevStep,
  reconcileSkips,
  shouldOpenWizard,
  stepDone,
  stepIdsFor,
  stepStatus,
  STEP_IDS,
  visibleStepIds,
  WIZARD_STEPS,
} from './setup';

const check = (id: (typeof CHECK_IDS)[number], status: PreflightCheck['status']): PreflightCheck => ({ id, status, detailKey: `preflight.${id}.x` });

/** Every check green. */
const allOk = (): PreflightCheck[] => CHECK_IDS.map((id) => check(id, 'ok'));

describe('the wizard steps', () => {
  it('covers every preflight check exactly once', () => {
    const covered = WIZARD_STEPS.flatMap((s) => s.checks);
    expect([...covered].sort()).toEqual([...CHECK_IDS].sort());
    expect(new Set(covered).size).toBe(covered.length);
  });

  it('runs the vault first, then the engine, the market server, the knowledge base, the official MCP, TradingView, the session, the schedule and the files', () => {
    expect(STEP_IDS).toEqual(['vault', 'claude', 'codex', 'tvmcp', 'knowledge', 'official', 'tradingview', 'session', 'schedule', 'files']);
    expect(nextStep('files')).toBeNull();
    expect(prevStep('vault')).toBeNull();
    expect(prevStep('tradingview')).toBe('official');
  });

  // The macro-review MVP: six pages, the legacy desk's pages hidden — their
  // checks still run (Settings → «Диагностика» shows them), only the wizard
  // pages go.
  it('shows six pages for the MVP and every page for the legacy desk', () => {
    expect(stepIdsFor({ legacyUi: false })).toEqual(['vault', 'claude', 'tvmcp', 'tradingview', 'session', 'schedule']);
    expect(stepIdsFor({ legacyUi: true })).toEqual(STEP_IDS);
    const mvpRun = [check('vault', 'ok'), check('claude', 'ok'), check('tvmcpRuntime', 'ok'), check('toolsPython', 'ok'), check('knowledgeBase', 'warn'), check('tvDesktop', 'ok'), check('tvSession', 'warn'), check('scheduler', 'warn'), check('timezone', 'ok'), check('workflows', 'ok')];
    expect(visibleStepIds(mvpRun, stepIdsFor({ legacyUi: false }))).toEqual(['vault', 'claude', 'tvmcp', 'tradingview', 'session', 'schedule']);
    expect(nextStep('tradingview', visibleStepIds(mvpRun, stepIdsFor({ legacyUi: false })))).toBe('session');
    expect(checksOfStep('tvmcp', mvpRun).map((c) => c.id)).toEqual(['tvmcpRuntime', 'toolsPython']);
  });

  // Only the engine that answers the chat is diagnosed, so exactly one of the
  // two engine steps has checks — and the other must not appear as an empty
  // page with a "skip" button for nothing.
  it('hides the step of the engine that was not diagnosed', () => {
    const claudeRun = [check('claude', 'ok'), check('tvmcpBoot', 'ok'), check('officialMcp', 'warn')];
    expect(visibleStepIds(claudeRun)).toEqual(['claude', 'tvmcp', 'official']);
    expect(nextStep('claude', visibleStepIds(claudeRun))).toBe('tvmcp');

    const codexRun = [check('codex', 'ok'), check('codexAuth', 'fail'), check('tvmcpBoot', 'ok'), check('codexOfficialMcp', 'warn')];
    expect(visibleStepIds(codexRun)).toEqual(['codex', 'tvmcp', 'official']);
    expect(prevStep('tvmcp', visibleStepIds(codexRun))).toBe('codex');
  });

  it('shows every step before the first diagnosis, rather than none', () => {
    expect(visibleStepIds([])).toEqual(STEP_IDS);
  });

  // The tvmcp step lists the checks of BOTH runtime modes; a run produces only
  // one mode's worth, and the step shows exactly what it got.
  it('shows a step its own checks, in list order, skipping ones the run did not produce', () => {
    expect(checksOfStep('tvmcp', allOk()).map((c) => c.id)).toEqual(['uv', 'tvmcpClone', 'tvmcpRuntime', 'tvmcpBoot', 'chromium', 'toolsPython']);
    expect(checksOfStep('tvmcp', [check('tvmcpBoot', 'ok')]).map((c) => c.id)).toEqual(['tvmcpBoot']);
    const packaged = [check('tvmcpRuntime', 'ok'), check('tvmcpBoot', 'ok'), check('chromium', 'warn')];
    expect(checksOfStep('tvmcp', packaged).map((c) => c.id)).toEqual(['tvmcpRuntime', 'tvmcpBoot', 'chromium']);
  });

  it('a step is done on ok or warn, and not on fail or unknown', () => {
    const warn = allOk().map((c) => (c.id === 'riskRules' ? check('riskRules', 'warn') : c));
    expect(stepStatus('files', warn)).toBe('warn');
    expect(stepDone('files', warn)).toBe(true);

    const failed = allOk().map((c) => (c.id === 'uv' ? check('uv', 'fail') : c));
    expect(stepDone('tvmcp', failed)).toBe(false);

    const unknown = allOk().map((c) => (c.id === 'tvmcpBoot' ? check('tvmcpBoot', 'unknown') : c));
    expect(stepDone('tvmcp', unknown)).toBe(false);
  });
});

describe('a skipped step is remembered', () => {
  it('adds and removes the step, keeping the list unique and in step order', () => {
    let skipped = applySkip([], 'tradingview', true);
    expect(skipped).toEqual(['tradingview']);
    skipped = applySkip(skipped, 'tradingview', true);
    expect(skipped).toEqual(['tradingview']);
    skipped = applySkip(skipped, 'claude', true);
    expect(skipped).toEqual(['claude', 'tradingview']);
    expect(applySkip(skipped, 'claude', false)).toEqual(['tradingview']);
  });

  it('drops junk that ended up in the stored list', () => {
    expect(applySkip(['nonsense', 'files', ''], 'files', true)).toEqual(['files']);
  });

  it('survives finishing: the patch carries both the date and the skipped steps', () => {
    const patch = finishPatch(['files', 'nonsense'], 1_700_000_000_000);
    expect(patch).toEqual({ setupCompletedAt: 1_700_000_000_000, setupSkipped: ['files'] });
    // ...and the settings that come back read the same way the wizard wrote them.
    const stored = { ...DEFAULT_SETTINGS, ...patch };
    expect(isSkipped(stored, 'files')).toBe(true);
    expect(isSkipped(stored, 'claude')).toBe(false);
    expect(shouldOpenWizard(stored)).toBe(false);
  });

  it('opens by itself only before the wizard has ever been finished', () => {
    expect(shouldOpenWizard(DEFAULT_SETTINGS)).toBe(true);
    expect(shouldOpenWizard({ setupCompletedAt: 1 })).toBe(false);
  });

  it('stops calling a step skipped once it is green again', () => {
    const skipped = ['tradingview', 'files'];
    const checks = allOk().map((c) => (c.id === 'riskRules' ? check('riskRules', 'warn') : c));
    // TradingView is fully ok now, the files step still only warns.
    expect(reconcileSkips(skipped, checks)).toEqual(['files']);
    expect(reconcileSkips(skipped, allOk())).toEqual([]);
  });

  it('turns the skipped steps into the notes the affected features show', () => {
    expect(limitationKeys({ setupSkipped: [] })).toEqual([]);
    expect(limitationKeys({ setupSkipped: ['files', 'claude'] })).toEqual(['setup.limited.claude', 'setup.limited.files']);
  });
});

describe('the pick-file fixes', () => {
  it('maps every settings path field to a dialog kind', () => {
    expect(Object.keys(PICK_FOR_FIELD).sort()).toEqual(['fieldNotesPath', 'gtRagPath', 'journalPath', 'riskRulesPath', 'tvmcpPath', 'vaultPath']);
    expect(PICK_FOR_FIELD.vaultPath).toBe('vault-dir');
    expect(PICK_FOR_FIELD.tvmcpPath).toBe('tvmcp-dir');
  });
});
