// The wizard's own logic: which checks belong to which step, and what
// "skipped" means — including that it survives closing the wizard and a
// restart, because a forgotten skip is exactly the silent failure this
// feature exists to remove.
import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS } from '@shared/defaults';
import { CHECK_IDS, type PreflightCheck } from '@shared/preflight';
import { applySkip, checksOfStep, finishPatch, isSkipped, limitationKeys, nextStep, PICK_FOR_FIELD, prevStep, reconcileSkips, shouldOpenWizard, stepDone, stepStatus, STEP_IDS, WIZARD_STEPS } from './setup';

const check = (id: (typeof CHECK_IDS)[number], status: PreflightCheck['status']): PreflightCheck => ({ id, status, detailKey: `preflight.${id}.x` });

/** Every check green. */
const allOk = (): PreflightCheck[] => CHECK_IDS.map((id) => check(id, 'ok'));

describe('the wizard steps', () => {
  it('covers every preflight check exactly once', () => {
    const covered = WIZARD_STEPS.flatMap((s) => s.checks);
    expect([...covered].sort()).toEqual([...CHECK_IDS].sort());
    expect(new Set(covered).size).toBe(covered.length);
  });

  it('runs Claude -> the market server -> the knowledge base -> the official MCP -> TradingView -> files', () => {
    expect(STEP_IDS).toEqual(['claude', 'tvmcp', 'knowledge', 'official', 'tradingview', 'files']);
    expect(nextStep('claude')).toBe('tvmcp');
    expect(nextStep('files')).toBeNull();
    expect(prevStep('claude')).toBeNull();
    expect(prevStep('tradingview')).toBe('official');
  });

  // The tvmcp step lists the checks of BOTH runtime modes; a run produces only
  // one mode's worth, and the step shows exactly what it got.
  it('shows a step its own checks, in list order, skipping ones the run did not produce', () => {
    expect(checksOfStep('tvmcp', allOk()).map((c) => c.id)).toEqual(['uv', 'tvmcpClone', 'tvmcpRuntime', 'tvmcpBoot', 'chromium']);
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
    expect(Object.keys(PICK_FOR_FIELD).sort()).toEqual(['fieldNotesPath', 'gtRagPath', 'journalPath', 'riskRulesPath', 'tvmcpPath']);
    expect(PICK_FOR_FIELD.tvmcpPath).toBe('tvmcp-dir');
  });
});
