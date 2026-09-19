// workflowstate: verdict badge, bias, levels, signals table, flags.
import type { WorkflowState } from '@shared/blocks';
import { t } from '../i18n';
import { fmtPrice } from './format';

export function WorkflowStateView({ data }: { data: WorkflowState }) {
  const conf = Math.round(data.bias.confidence * 100);
  return (
    <div class="card workflowstate">
      <div class="card-head">
        <span class={`verdict ${data.verdict}`}>{data.verdict}</span>
        <span>
          {data.workflow} · {data.date} · <span class="muted">{t(`cards.phase.${data.phase}`)}</span>
        </span>
      </div>
      <div class="card-body">
        <div class={`bias ${data.bias.direction}`}>
          <span class="bias-dir">{data.bias.direction === 'NONE' ? t('cards.biasNone') : data.bias.direction}</span>
          <span class="bias-conf muted">
            {t('cards.confidence')} {conf}%
          </span>
          <svg class="bias-bar" viewBox="0 0 100 6" width="100%" height={6} preserveAspectRatio="none" role="img">
            <rect class="bias-track" x={0} y={0} width={100} height={6} rx={3} />
            <rect class="bias-fill" x={0} y={0} width={conf} height={6} rx={3} />
          </svg>
        </div>
        {data.levels.length ? (
          <div class="wf-levels">
            {data.levels.map((l, i) => (
              <span class="wf-level" key={i} title={l.note ?? ''}>
                {l.name} <b>{fmtPrice(l.price)}</b>
              </span>
            ))}
          </div>
        ) : null}
        {data.signals.length ? (
          <table class="signals">
            <thead>
              <tr>
                <th>{t('cards.setup')}</th>
                <th>{t('cards.entry')}</th>
                <th>SL</th>
                <th>TP1</th>
                <th>RR</th>
                <th>{t('cards.validUntil')}</th>
              </tr>
            </thead>
            <tbody>
              {data.signals.map((s, i) => (
                <tr key={i}>
                  <td>{String(s.setup)}</td>
                  <td>{fmtPrice(s.entry)}</td>
                  <td>{fmtPrice(s.sl)}</td>
                  <td>{fmtPrice(s.tp1)}</td>
                  <td>{s.rr != null ? s.rr.toFixed(1) : '—'}</td>
                  <td>{s.validUntil ?? '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : null}
        {data.flags.length ? (
          <div class="flags">
            {data.flags.map((f, i) => (
              <span class="flag" key={i}>
                ⚑ {f}
              </span>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
