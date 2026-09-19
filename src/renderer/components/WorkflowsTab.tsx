// «Воркфлоу» tab: every workflow with its quick prompts, report catalog,
// shelves and prompt block; one is active (settings.activeWorkflow).
import { useState } from 'preact/hooks';
import type { WorkflowDef } from '@shared/api';
import { t } from '../i18n';

export interface WorkflowsTabProps {
  workflows: WorkflowDef[];
  activeId: string;
  busy: boolean;
  onActivate: (id: string) => void;
  onRun: (prompt: string) => void;
}

function PromptBlock({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div class="collapsible">
      <button type="button" class="small" onClick={() => setOpen(!open)}>
        {open ? '▾' : '▸'} {t('workflows.promptBlock')}
      </button>
      {open ? <pre class="prompt-block">{text}</pre> : null}
    </div>
  );
}

export function WorkflowsTab(p: WorkflowsTabProps) {
  return (
    <section class="page workflows">
      <h2>{t('workflows.title')}</h2>
      <p class="muted">{t('workflows.intro')}</p>
      {!p.workflows.length ? <div class="empty">{t('workflows.empty')}</div> : null}
      {p.workflows.map((w) => {
        const active = w.id === p.activeId;
        return (
          <fieldset key={w.id} class={`workflow${active ? ' active' : ''}`}>
            <legend>
              {w.title} <code class="inline">{w.id}</code>
              {active ? <span class="tag base">{t('workflows.active')}</span> : null}
            </legend>
            <p class="muted">{w.description}</p>
            {!active ? (
              <div>
                <button type="button" class="primary" onClick={() => p.onActivate(w.id)}>
                  {t('workflows.activate')}
                </button>
              </div>
            ) : null}
            {w.quickPrompts.length ? (
              <div class="field">
                <span>{t('workflows.quick')}</span>
                <div class="chips">
                  {w.quickPrompts.map((q) => (
                    <button type="button" class="chip" key={q.id} disabled={p.busy} title={q.prompt} onClick={() => p.onRun(q.prompt)}>
                      {q.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
            {w.reports.length ? (
              <div class="field">
                <span>{t('workflows.reports')}</span>
                <div class="chips">
                  {w.reports.map((r) => (
                    <button type="button" class="chip action" key={r.id} disabled={p.busy} title={r.prompt} onClick={() => p.onRun(r.prompt)}>
                      {r.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}
            {w.shelves.length ? (
              <div class="field">
                <span>{t('workflows.shelves')}</span>
                <div class="muted">{w.shelves.map((s) => t(`shelf.${s}`)).join(' · ')}</div>
              </div>
            ) : null}
            {w.prompt ? <PromptBlock text={w.prompt} /> : null}
          </fieldset>
        );
      })}
    </section>
  );
}
