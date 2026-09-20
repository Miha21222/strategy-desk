// The preflight check list, rendered the same way in the wizard and in the
// «Диагностика» section of the settings.
//
// A fix is a button only where the app may act on its own (launch TradingView,
// switch the port, pick a file). Everything that would install software, reach
// the network or close the user's TradingView is copyable text plus one
// sentence saying what will happen.
import { useState } from 'preact/hooks';
import type { PreflightCheck, PreflightFix, PreflightResult } from '@shared/preflight';
import { t } from '../i18n';

export interface PreflightListProps {
  result: PreflightResult | null;
  busy: boolean;
  /** Only these check ids, in list order; empty = all of them. */
  only?: string[];
  onRecheck: () => void;
  onFix: (fix: PreflightFix) => void;
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

function Fix({ fix, onFix }: { fix: PreflightFix; onFix: (f: PreflightFix) => void }) {
  const [open, setOpen] = useState(false);
  const note = 'noteKey' in fix && fix.noteKey ? t(fix.noteKey) : '';

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
      <button type="button" class="primary" onClick={() => onFix(fix)}>
        {label}
      </button>
    </div>
  );
}

function Row({ check, onFix }: { check: PreflightCheck; onFix: (f: PreflightFix) => void }) {
  return (
    <li class={`pf-row ${check.status}`}>
      <span class={`pf-mark ${check.status}`} aria-hidden="true">
        {MARK[check.status] ?? '?'}
      </span>
      <div class="pf-body">
        <div class="pf-title">{t(`preflight.check.${check.id}`)}</div>
        <div class="pf-detail">{t(check.detailKey, check.vars)}</div>
        {check.fix ? <Fix fix={check.fix} onFix={onFix} /> : null}
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
          <Row key={c.id} check={c} onFix={p.onFix} />
        ))}
      </ul>
    </div>
  );
}
