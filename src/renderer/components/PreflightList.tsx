// The preflight check list, rendered the same way in the wizard and in the
// «Диагностика» section of the settings.
//
// A fix is a button where the app may act: launching TradingView, switching the
// port, picking a file, flipping one of its own settings — and the four
// `run-task` operations it owns end to end (provisioning its bundled Python
// runtime, fetching the headless browser, registering and authenticating the
// official TradingView MCP). Those show the exact command before the press and
// stream their output while they run, because a fifteen-minute install behind a
// silent button is indistinguishable from a hang.
//
// Everything that would install software into the user's system, clone into
// their folders or close their TradingView stays copyable text.
import { useState } from 'preact/hooks';
import type { PreflightCheck, PreflightFix, PreflightResult } from '@shared/preflight';
import type { TaskId, TaskState } from '@shared/tasks';
import { t } from '../i18n';

export interface PreflightListProps {
  result: PreflightResult | null;
  busy: boolean;
  /** Only these check ids, in list order; empty = all of them. */
  only?: string[];
  /** Live state of the long operations, keyed by task id. */
  tasks?: Partial<Record<TaskId, TaskState>>;
  /** What each task would run, shown before the press. */
  taskCommands?: Partial<Record<TaskId, string>>;
  onRecheck: () => void;
  onFix: (fix: PreflightFix) => void;
  onTaskStop?: (id: TaskId) => void;
}

const MARK: Record<string, string> = { ok: '✓', warn: '!', fail: '×', unknown: '?' };

function Copyable({ command, label }: { command: string; label: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div class="fix-copy">
      <code>{command}</code>
      <button
        type="button"
        onClick={() => {
          void navigator.clipboard?.writeText(command);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
      >
        {copied ? t('preflight.copied') : label}
      </button>
    </div>
  );
}

/**
 * A long operation: the command it will run, a button, and its output.
 *
 * The output is shown while it runs and stays after it finishes — a failed
 * `uv sync` says which package could not be resolved, and that line is the
 * whole diagnosis. The tail is what matters, so the box scrolls to the bottom.
 */
function TaskFix({
  fix,
  state,
  command,
  onFix,
  onStop,
}: {
  fix: Extract<PreflightFix, { kind: 'run-task' }>;
  state?: TaskState;
  command?: string;
  onFix: (f: PreflightFix) => void;
  onStop?: (id: TaskId) => void;
}) {
  const [showLog, setShowLog] = useState(false);
  const running = Boolean(state?.running);
  const lines = state?.lines ?? [];
  const note = fix.noteKey ? t(fix.noteKey) : '';
  const tail = lines.slice(-8);

  return (
    <div class="fix task-fix">
      {note ? <p class="fix-note">{note}</p> : null}
      {command ? <code class="task-command">{command}</code> : null}
      <div class="task-buttons">
        <button type="button" class="primary" disabled={running} onClick={() => onFix(fix)}>
          {running ? t('preflight.task.running') : state?.ok === false ? t('preflight.task.retry') : t(fix.labelKey)}
        </button>
        {running && onStop ? (
          <button type="button" onClick={() => onStop(fix.task)}>
            {t('preflight.task.stop')}
          </button>
        ) : null}
        {lines.length ? (
          <button type="button" class="ghost" onClick={() => setShowLog(!showLog)}>
            {showLog ? t('preflight.task.hideLog') : t('preflight.task.showLog', { n: lines.length })}
          </button>
        ) : null}
      </div>
      {state?.ok === false && state.error ? <p class="fix-note error">{t('preflight.task.failed', { error: state.error })}</p> : null}
      {state?.ok === true ? <p class="fix-note">{t('preflight.task.done')}</p> : null}
      {lines.length ? <pre class="task-log">{(showLog ? lines : tail).join('\n')}</pre> : null}
    </div>
  );
}

function Fix({
  fix,
  tasks,
  taskCommands,
  onFix,
  onTaskStop,
}: {
  fix: PreflightFix;
  tasks?: Partial<Record<TaskId, TaskState>>;
  taskCommands?: Partial<Record<TaskId, string>>;
  onFix: (f: PreflightFix) => void;
  onTaskStop?: (id: TaskId) => void;
}) {
  const [open, setOpen] = useState(false);
  const note = 'noteKey' in fix && fix.noteKey ? t(fix.noteKey) : '';

  if (fix.kind === 'run-task') {
    return <TaskFix fix={fix} state={tasks?.[fix.task]} command={taskCommands?.[fix.task]} onFix={onFix} onStop={onTaskStop} />;
  }

  if (fix.kind === 'copy') {
    return (
      <div class="fix">
        {note ? <p class="fix-note">{note}</p> : null}
        <Copyable command={fix.command} label={t(fix.labelKey)} />
        {fix.altCommand ? (
          <details>
            <summary>{t('preflight.otherOs')}</summary>
            <Copyable command={fix.altCommand} label={t('preflight.copy')} />
          </details>
        ) : null}
      </div>
    );
  }

  if (fix.kind === 'explain') {
    // Nothing is done for the user here on purpose: the instruction is the fix.
    return (
      <div class="fix">
        <button type="button" onClick={() => setOpen(!open)}>
          {open ? t('preflight.hideNote') : t(fix.labelKey)}
        </button>
        {open ? <p class="fix-note">{note}</p> : null}
      </div>
    );
  }

  const label = fix.kind === 'set-port' ? t(fix.labelKey, { port: fix.port }) : t(fix.labelKey);
  return (
    <div class="fix">
      {fix.kind === 'set-flag' && note ? <p class="fix-note">{note}</p> : null}
      <button type="button" class="primary" onClick={() => onFix(fix)}>
        {label}
      </button>
    </div>
  );
}

function Row({
  check,
  tasks,
  taskCommands,
  onFix,
  onTaskStop,
}: {
  check: PreflightCheck;
  tasks?: Partial<Record<TaskId, TaskState>>;
  taskCommands?: Partial<Record<TaskId, string>>;
  onFix: (f: PreflightFix) => void;
  onTaskStop?: (id: TaskId) => void;
}) {
  return (
    <li class={`pf-row ${check.status}`}>
      <span class={`pf-mark ${check.status}`} aria-hidden="true">
        {MARK[check.status] ?? '?'}
      </span>
      <div class="pf-body">
        <div class="pf-title">{t(`preflight.check.${check.id}`)}</div>
        <div class="pf-detail">{t(check.detailKey, check.vars)}</div>
        {check.fix ? <Fix fix={check.fix} tasks={tasks} taskCommands={taskCommands} onFix={onFix} onTaskStop={onTaskStop} /> : null}
        {check.docs ? (
          <a href={check.docs} target="_blank" rel="noreferrer">
            {t('preflight.docs')}
          </a>
        ) : null}
      </div>
    </li>
  );
}

export function PreflightList(p: PreflightListProps) {
  const all = p.result?.checks ?? [];
  const checks = p.only?.length ? p.only.map((id) => all.find((c) => c.id === id)).filter((c): c is PreflightCheck => Boolean(c)) : all;
  const bad = checks.filter((c) => c.status === 'fail' || c.status === 'warn').length;

  return (
    <div class="preflight">
      <div class="pf-head">
        <button type="button" onClick={p.onRecheck} disabled={p.busy}>
          {p.busy ? t('preflight.checking') : t('preflight.recheck')}
        </button>
        {p.result ? (
          <span class="muted">
            {t('preflight.checkedAt', { time: new Date(p.result.at).toLocaleTimeString() })} — {bad ? t('preflight.hasProblems', { n: bad }) : t('preflight.allOk')}
          </span>
        ) : (
          <span class="muted">{p.busy ? t('preflight.checking') : t('preflight.never')}</span>
        )}
      </div>
      <ul class="pf-list">
        {checks.map((c) => (
          <Row key={c.id} check={c} tasks={p.tasks} taskCommands={p.taskCommands} onFix={p.onFix} onTaskStop={p.onTaskStop} />
        ))}
      </ul>
    </div>
  );
}
