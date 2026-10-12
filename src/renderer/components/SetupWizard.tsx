// First-run wizard: Claude -> the market server -> the official MCP ->
// TradingView and the port -> files -> done. Opens by itself when
// `setupCompletedAt` is unset, and from «Мастер настройки» in the settings at
// any time.
//
// Every step shows its own slice of the preflight list, so the fixes are the
// same objects the diagnosis hands out: a button where the app may act (up to
// and including provisioning its own Python environment, which the step shows
// the command for and streams), a copyable command where it may not. A step can
// be skipped, and the skip is remembered in the settings — the features it
// weakens say so instead of failing mysteriously later.
import { useEffect, useRef, useState } from 'preact/hooks';
import type { SecretName, Settings } from '@shared/api';
import type { PreflightCheck, PreflightFix, PreflightResult } from '@shared/preflight';
import type { TaskId, TaskState } from '@shared/tasks';
import { t } from '../i18n';
import { autoFixes, isSkipped, SCHEDULE_TIMES, stepDone, stepIdsFor, stepTaskIds, visibleStepIds, WIZARD_STEPS, type StepId } from '../setup';
import { PreflightList } from './PreflightList';

export interface SetupWizardProps {
  step: StepId;
  result: PreflightResult | null;
  busy: boolean;
  /** The steps shown depend on `legacyUi`; the vault, session and schedule pages read the rest. */
  settings: Settings;
  /** The session page stores the cookie (null = clear); it never reads it back. */
  onSecret: (name: SecretName, value: string | null) => void;
  /** The vault page's «Выбрать папку»: a native picker, then settings.vaultPath. */
  onPickVault: () => void;
  /** Start a task by id outside of a check's fix (removing the schedule). */
  onTask: (id: TaskId) => void;
  /** Step ids currently marked skipped (renderer-local until the wizard closes). */
  skipped: string[];
  onStep: (id: StepId) => void;
  onSkip: (id: StepId, on: boolean) => void;
  onRecheck: () => void;
  onFix: (fix: PreflightFix) => void;
  /** Live state and the command of each long operation a step may start. */
  tasks?: Partial<Record<TaskId, TaskState>>;
  taskCommands?: Partial<Record<TaskId, string>>;
  onTaskStop?: (id: TaskId) => void;
  /** One line into a running task: the login code from the browser. */
  onTaskInput?: (id: TaskId, text: string) => void;
  onFinish: () => void;
  onClose: () => void;
}

/**
 * The vault page: where the folder is, and a picker. Creating it is the
 * `vault` check's own fix (a task), so the button appears exactly when the
 * folder is missing or empty and a template shipped.
 */
function VaultExtra({ settings, onPickVault }: { settings: Settings; onPickVault: () => void }) {
  return (
    <div class="wizard-extra">
      <p class="muted">{t('setup.vault.where')}</p>
      <div class="inline">
        <code class="path">{settings.vaultPath || '—'}</code>
        <button type="button" onClick={onPickVault}>
          {t('setup.vault.pick')}
        </button>
      </div>
      <p class="muted">{t('setup.vault.hint')}</p>
    </div>
  );
}

/**
 * The session page: what the cookie is, where to copy it from, and the field
 * that stores it. The value goes to safeStorage through the same channel as
 * the API key and is never shown again; the «Проверить» button is the check's
 * fix, so it appears once something is stored.
 */
function SessionExtra({ settings, check, onSecret, onTask, tasks }: { settings: Settings; check?: PreflightCheck; onSecret: (name: SecretName, value: string | null) => void; onTask: (id: TaskId) => void; tasks?: Partial<Record<TaskId, TaskState>> }) {
  const [value, setValue] = useState('');
  const running = Boolean(tasks?.['session-check']?.running);
  return (
    <div class="wizard-extra">
      <p>{t('setup.session.what')}</p>
      <ol class="howto">
        <li>{t('setup.session.how1')}</li>
        <li>{t('setup.session.how2')}</li>
        <li>{t('setup.session.how3')}</li>
        <li>{t('setup.session.how4')}</li>
      </ol>
      <label class="field">
        <span>
          {t('setup.session.field')} — <em>{settings.hasTvSession ? t('settings.secretSet') : t('settings.secretNotSet')}</em>
        </span>
        <div class="inline">
          <input type="password" value={value} autocomplete="off" placeholder={t('setup.session.placeholder')} onInput={(e) => setValue((e.target as HTMLInputElement).value)} />
          <button
            type="button"
            class="primary"
            disabled={!value.trim()}
            onClick={() => {
              onSecret('tvSession', value.trim());
              setValue('');
            }}
          >
            {t('settings.saveSecret')}
          </button>
          <button type="button" disabled={!settings.hasTvSession} onClick={() => onSecret('tvSession', null)}>
            {t('settings.clearSecret')}
          </button>
        </div>
        <small>{t('setup.session.stored')}</small>
      </label>
      {settings.hasTvSession && check?.status === 'ok' ? (
        <button type="button" disabled={running} onClick={() => onTask('session-check')}>
          {running ? t('preflight.task.running') : t('setup.session.recheck')}
        </button>
      ) : null}
    </div>
  );
}

/**
 * The schedule page: the three runs and the clock they assume. The register
 * button is the `scheduler` check's fix; removing is offered here, once
 * something is registered.
 */
function ScheduleExtra({ checks, onTask, tasks }: { checks: PreflightCheck[]; onTask: (id: TaskId) => void; tasks?: Partial<Record<TaskId, TaskState>> }) {
  const scheduler = checks.find((c) => c.id === 'scheduler');
  const tz = checks.find((c) => c.id === 'timezone');
  const registered = scheduler?.status === 'ok' || scheduler?.detailKey === 'preflight.scheduler.partial' || scheduler?.detailKey === 'preflight.scheduler.disabled';
  const running = Boolean(tasks?.['scheduler-unregister']?.running);
  return (
    <div class="wizard-extra">
      <p>{t('setup.schedule.what')}</p>
      <ul class="schedule-times">
        {SCHEDULE_TIMES.map((s) => (
          <li key={s.region}>
            <b>{s.at}</b> — {t(`setup.schedule.${s.region}`)}
          </li>
        ))}
      </ul>
      <p class={`muted${tz?.status === 'warn' ? ' warn-text' : ''}`}>{t('setup.schedule.clock', { tz: String(tz?.vars?.tz ?? '—') })}</p>
      {registered ? (
        <button type="button" disabled={running} onClick={() => onTask('scheduler-unregister')}>
          {running ? t('preflight.task.running') : t('setup.schedule.unregister')}
        </button>
      ) : null}
    </div>
  );
}

export function SetupWizard(p: SetupWizardProps) {
  const checks = p.result?.checks ?? [];
  // The engine the user did not choose has no checks, so its step is not shown
  // at all — and the numbering counts what is on screen. The MVP shows six
  // pages; the legacy desk every one.
  const ids = visibleStepIds(checks, stepIdsFor(p.settings));
  const index = Math.max(0, ids.indexOf(p.step));
  const step = WIZARD_STEPS.find((s) => s.id === ids[index]) ?? WIZARD_STEPS[0];
  const done = p.result ? stepDone(step.id, checks) : false;
  const skipped = isSkipped({ setupSkipped: p.skipped }, step.id);
  const last = index === ids.length - 1;
  const stillSkipped = ids.filter((id) => p.skipped.includes(id));

  // What the installer could not do, the page does when it opens: lay the
  // vault template down, build the Python environment, fetch the browser,
  // re-check a stored cookie (`AUTO_TASKS`). Once per task per wizard session;
  // a failure is retried by the user, with the button and the log right there.
  const started = useRef(new Set<TaskId>());
  const [autoRan, setAutoRan] = useState<TaskId[]>([]);
  useEffect(() => {
    if (!p.result || p.busy) return;
    const fixes = autoFixes(step.id, checks, p.tasks, started.current, p.taskCommands);
    if (!fixes.length) return;
    for (const fix of fixes) started.current.add(fix.task);
    setAutoRan((prev) => [...prev, ...fixes.map((f) => f.task)]);
    for (const fix of fixes) p.onFix(fix);
  }, [step.id, p.result, p.busy, p.tasks, p.taskCommands]);
  const stepTasks = stepTaskIds(step.id, checks);
  const autoHere = autoRan.filter((id) => stepTasks.includes(id));

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
          {WIZARD_STEPS.filter((s) => ids.includes(s.id)).map((s, i) => {
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
          <div class="muted">{t('setup.step', { n: index + 1, total: ids.length })}</div>
          <p>{t(`setup.stepIntro.${step.id}`)}</p>
          {autoHere.length ? <p class="muted">{t('setup.auto', { list: autoHere.map((id) => p.taskCommands?.[id] || id).join('; ') })}</p> : null}
          {step.id === 'vault' ? <VaultExtra settings={p.settings} onPickVault={p.onPickVault} /> : null}
          {step.id === 'session' ? <SessionExtra settings={p.settings} check={checks.find((c) => c.id === 'tvSession')} onSecret={p.onSecret} onTask={p.onTask} tasks={p.tasks} /> : null}
          {step.id === 'schedule' ? <ScheduleExtra checks={checks} onTask={p.onTask} tasks={p.tasks} /> : null}
          <PreflightList
            result={p.result}
            busy={p.busy}
            only={step.checks}
            tasks={p.tasks}
            taskCommands={p.taskCommands}
            onRecheck={p.onRecheck}
            onFix={p.onFix}
            onTaskStop={p.onTaskStop}
            onTaskInput={p.onTaskInput}
          />
          {skipped ? <p class="notice warn">{t(`setup.limited.${step.id}`)}</p> : p.result ? <p class="muted">{done ? t('setup.stepOk') : t('setup.stepProblems')}</p> : null}
        </div>

        <footer>
          <button type="button" onClick={() => p.onStep(ids[Math.max(0, index - 1)])} disabled={index === 0}>
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
            <button type="button" class="primary" onClick={() => p.onStep(ids[index + 1])}>
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
