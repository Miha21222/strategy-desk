// briefreport: level checklist (tagged / untouched / not in play), stat tiles
// (volume vs typical + up to 4 states), news rows and a note.
import type { BriefReport } from '@shared/blocks';
import { t } from '../i18n';
import { StatTiles } from './charts';
import { fmtPrice } from './format';

const MAX_STATES = 4;

function hitMark(hit: boolean | null): { mark: string; cls: string; text: string } {
  if (hit === true) return { mark: '✓', cls: 'hit', text: t('cards.levelHit') };
  if (hit === false) return { mark: '✗', cls: 'miss', text: t('cards.levelUntouched') };
  return { mark: '–', cls: 'idle', text: t('cards.levelIdle') };
}

export function BriefReportView({ data }: { data: BriefReport }) {
  const tiles: { label: string; value: string }[] = [];
  if (data.volume) {
    const vs = data.volume.vsAvg != null ? ` (${data.volume.vsAvg >= 1 ? '+' : ''}${Math.round((data.volume.vsAvg - 1) * 100)}%)` : '';
    tiles.push({ label: t('cards.volume'), value: `${data.volume.text}${vs}` });
  }
  for (const s of data.states.slice(0, MAX_STATES)) tiles.push({ label: s.name, value: s.value });

  return (
    <div class="card briefreport">
      <div class="card-head">
        <span>
          {t('cards.brief')} · {data.symbol}
        </span>
        <span class="muted">
          {data.date}
          {data.session ? ` · ${data.session}` : ''}
          {data.regime ? ` · ${data.regime}` : ''}
        </span>
      </div>
      <div class="card-body">
        {data.levels.length ? (
          <div class="checklist">
            {data.levels.map((l, i) => {
              const h = hitMark(l.hit);
              return (
                <div class={`check-row ${h.cls}`} key={i} title={l.note ?? ''}>
                  <span class="check-mark">{h.mark}</span>
                  <span class="check-name">{l.name}</span>
                  <span class="check-price">{fmtPrice(l.price)}</span>
                  <span class="check-state muted">{h.text}</span>
                </div>
              );
            })}
          </div>
        ) : null}
        {tiles.length ? <StatTiles items={tiles} /> : null}
        {data.news.length ? (
          <div class="news">
            {data.news.map((n, i) => (
              <div class={`news-row ${n.impact}`} key={i}>
                <span class="news-dot">●</span>
                <span class="news-time">{n.time}</span>
                <span class="news-title">{n.title}</span>
              </div>
            ))}
          </div>
        ) : null}
        {data.notes ? <div class="card-note">{data.notes}</div> : null}
      </div>
    </div>
  );
}
