// Placeholder cards for blocks whose full rendering arrives with the analytics
// module (prodata, healthdata) and the compact error card for a block that
// failed JSON / schema validation. Raw text is collapsed, never lost.
import { useState } from 'preact/hooks';
import type { HealthData, Prodata } from '@shared/blocks';
import { t } from '../i18n';

function Collapsible({ label, text }: { label: string; text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div class="collapsible">
      <button type="button" class="small" onClick={() => setOpen(!open)}>
        {open ? '▾' : '▸'} {label}
      </button>
      {open ? (
        <pre>
          <code>{text}</code>
        </pre>
      ) : null}
    </div>
  );
}

export function ProdataCard({ data }: { data: Prodata }) {
  return (
    <div class="card placeholder prodata">
      <div class="card-head">
        <span>
          {t('cards.prodata')} · {data.strategy} <span class="muted">{data.version}</span>
        </span>
        <span class="muted">
          {data.market} · {t('cards.focus')}: {data.focus} · {t('cards.tradesTotal', { n: data.trades.length })}
        </span>
      </div>
      <div class="card-body">
        <div class="card-note">{t('cards.analyticsPending')}</div>
        <Collapsible label={t('cards.rawJson')} text={JSON.stringify(data, null, 2)} />
      </div>
    </div>
  );
}

export function HealthDataCard({ data }: { data: HealthData }) {
  return (
    <div class="card placeholder healthdata">
      <div class="card-head">
        <span>
          {t('cards.healthdata')} · {data.strategy}
        </span>
        <span class="muted">{t('cards.tradesTotal', { n: data.trades.length })}</span>
      </div>
      <div class="card-body">
        <div class="card-note">{t('cards.analyticsPending')}</div>
        <Collapsible label={t('cards.rawJson')} text={JSON.stringify(data, null, 2)} />
      </div>
    </div>
  );
}

export function ErrorCard({ kind, error, raw }: { kind: string; error: string; raw: string }) {
  return (
    <div class="card error">
      <div class="card-head">
        <span>{t('cards.invalid', { kind })}</span>
      </div>
      <div class="card-body">
        <div class="card-error">{error}</div>
        <Collapsible label={t('cards.rawText')} text={raw} />
      </div>
    </div>
  );
}

export function StreamingCard({ kind }: { kind: string }) {
  return (
    <div class="card streaming">
      <div class="card-head">
        <span class="pulse" /> {t('cards.streaming', { kind })}
      </div>
    </div>
  );
}
