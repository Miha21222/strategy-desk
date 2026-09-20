// First-run wizard: Claude -> uv and tvmcp -> TradingView and the port ->
// files -> done. Opens by itself when `setupCompletedAt` is unset, and from
// «Мастер настройки» in the settings at any time.
//
// Every step shows its own slice of the preflight list, so the fixes are the
// same objects the diagnosis hands out: a button where the app may act, a
// copyable command where it may not. A step can be skipped, and the skip is
// remembered in the settings — the features it weakens say so instead of
// failing mysteriously later.
import type { PreflightFix, PreflightResult } from '@shared/preflight';
import { t } from '../i18n';
import { isSkipped, stepDone, STEP_IDS, WIZARD_STEPS, type StepId } from '../setup';
import { PreflightList } from './PreflightList';

export interface SetupWizardProps {
  step: StepId;
  result: PreflightResult | null;
  busy: boolean;
  /** Step ids currently marked skipped (renderer-local until the wizard closes). */
  skipped: string[];
  onStep: (id: StepId) => void;
  onSkip: (id: StepId, on: boolean) => void;
  onRecheck: () => void;
  onFix: (fix: PreflightFix) => void;
  onFinish: () => void;
  onClose: () => void;
}

export function SetupWizard(p: SetupWizardProps) {
  const index = STEP_IDS.indexOf(p.step);
  const step = WIZARD_STEPS[index] ?? WIZARD_STEPS[0];
  const checks = p.result?.checks ?? [];
  const done = p.result ? stepDone(step.id, checks) : false;
  const skipped = isSkipped({ setupSkipped: p.skipped }, step.id);
  const last = index === STEP_IDS.length - 1;
  const stillSkipped = STEP_IDS.filter((id) => p.skipped.includes(id));

  return (
    <div class="modal-backdrop">
      <div class="modal wizard" role="dialog" aria-label={t('setup.title')}>
        <header>
          <h2>{t('setup.title')}</h2>
          <button type="button" class="ghost" onClick={p.onClose} title={t('setup.close')}>
            ✕
          </button>
        </header>

        <ol class="wizard-steps">
          {WIZARD_STEPS.map((s, i) => {
            const state = p.result ? (isSkipped({ setupSkipped: p.skipped }, s.id) ? 'skipped' : stepDone(s.id, checks) ? 'ok' : 'todo') : 'todo';
            return (
              <li key={s.id} class={`${s.id === p.step ? 'on ' : ''}${state}`}>
                <button type="button" onClick={() => p.onStep(s.id)}>
                  <span class="num">{i + 1}</span>
                  {t(`setup.steps.${s.id}`)}
                </button>
              </li>
            );
          })}
        </ol>

        <div class="wizard-body">
          <div class="muted">{t('setup.step', { n: index + 1, total: STEP_IDS.length })}</div>
          <p>{t(`setup.stepIntro.${step.id}`)}</p>
          <PreflightList result={p.result} busy={p.busy} only={step.checks} onRecheck={p.onRecheck} onFix={p.onFix} />
          {skipped ? <p class="notice warn">{t(`setup.limited.${step.id}`)}</p> : p.result ? <p class="muted">{done ? t('setup.stepOk') : t('setup.stepProblems')}</p> : null}
        </div>

        <footer>
          <button type="button" onClick={() => p.onStep(STEP_IDS[Math.max(0, index - 1)])} disabled={index === 0}>
            {t('setup.back')}
          </button>
          <button type="button" onClick={() => p.onSkip(step.id, !skipped)}>
            {skipped ? t('setup.unskip') : t('setup.skip')}
          </button>
          <span class="spacer" />
          {last ? (
            <button type="button" class="primary" onClick={p.onFinish}>
              {t('setup.finish')}
            </button>
          ) : (
            <button type="button" class="primary" onClick={() => p.onStep(STEP_IDS[index + 1])}>
              {t('setup.next')}
            </button>
          )}
        </footer>

        {last ? (
          <div class="wizard-summary muted">
            {stillSkipped.length ? t('setup.doneSkipped', { list: stillSkipped.map((id) => t(`setup.steps.${id}`)).join(', ') }) : t('setup.doneOk')}
          </div>
        ) : null}
      </div>
    </div>
  );
}
