// Chart screenshots of a review window: the levels the skill drew on
// TradingView (`state/shots_<D>_<region>.json` + PNGs under indices-kb/shots/).
//
// The main view gets one picture (M15) with its legend; «Подробнее» gets all
// of them; either opens the lightbox. No file at all = nothing on screen; a
// failed run = one quiet line saying why, in plain words.
import { useEffect, useState } from 'preact/hooks';
import { legendColor, mainShot, shotsErrorText, shotTabLabel, type MacroShot, type MacroShots, type ShotLegend } from '@shared/macro';
import { t } from '../i18n';

// ---------------------------------------------------------------- loading

/**
 * One request per picture per run: keyed by the json's `asof`, so a re-run
 * (new asof, same paths) loads the new pictures while switching days and
 * windows reuses what is already here.
 */
const cache = new Map<string, Promise<string | null>>();
const CACHE_MAX = 40;

function loadImage(path: string, asof: string): Promise<string | null> {
  const key = `${asof}\n${path}`;
  let p = cache.get(key);
  if (!p) {
    p = window.desk.macro.image(path).catch(() => null);
    cache.set(key, p);
    // Data URLs are large; keep the newest few.
    while (cache.size > CACHE_MAX) cache.delete(cache.keys().next().value as string);
  }
  return p;
}

/** The picture as a data URL; `undefined` while loading, `null` when it is not there. */
export function useShotImage(path: string, asof: string): string | null | undefined {
  const [src, setSrc] = useState<string | null | undefined>(undefined);
  useEffect(() => {
    let live = true;
    setSrc(undefined);
    void loadImage(path, asof).then((x) => live && setSrc(x));
    return () => {
      live = false;
    };
  }, [path, asof]);
  return src;
}

// ---------------------------------------------------------------- pieces

export function ShotLegendList({ legend }: { legend: ShotLegend[] }) {
  if (!legend.length) return null;
  return (
    <ul class="shot-legend">
      {legend.map((l) => (
        <li key={l.key || l.label} class={`lg-${l.kind}${(l.style ?? (l.kind === 'open' ? 'dashed' : 'solid')) === 'dashed' ? ' lg-dashed' : ''}`}>
          <i class="lg-line" style={{ '--lg': legendColor(l) } as Record<string, string>} />
          {l.label}
        </li>
      ))}
    </ul>
  );
}

/** The picture itself, its warning chip and its states. Clicking opens the lightbox. */
function ShotImage({ shot, asof, onOpen }: { shot: MacroShot; asof: string; onOpen?: () => void }) {
  const src = useShotImage(shot.path, asof);
  const label = shotTabLabel(shot);
  if (src === null) return <div class="shot-frame shot-missing muted">{t('macro.shotsMissing')}</div>;
  return (
    <button type="button" class="shot-frame" onClick={onOpen} title={t('macro.shotsOpen')} aria-label={`${label}. ${t('macro.shotsOpen')}`} disabled={!src}>
      {src ? <img src={src} alt={label} draggable={false} /> : <span class="shot-loading muted">{t('macro.shotsLoading')}</span>}
      {shot.verify_ok === false ? <span class="shot-warn">⚠ {t('macro.shotsMoved')}</span> : null}
    </button>
  );
}

// ---------------------------------------------------------------- «Что делать»

/** One picture under the plan, or one quiet line when the run made none. */
export function ShotsInline({ shots, onOpen }: { shots: MacroShots | null; onOpen: (i: number) => void }) {
  if (!shots) return null;
  if (!shots.ok) return <p class="shots-none muted">{t('macro.shotsNone', { why: shotsWhy(shots.error) })}</p>;
  const main = mainShot(shots.shots);
  if (!main) return null;
  return (
    <figure class="shot-inline">
      <ShotImage shot={main} asof={shots.asof} onOpen={() => onOpen(shots.shots.indexOf(main))} />
      <ShotLegendList legend={main.legend} />
    </figure>
  );
}

function shotsWhy(error: string | null): string {
  const e = shotsErrorText(error);
  if (e.known === 'dry-run') return t('macro.shotsDryRun');
  if (e.known === 'none') return t('macro.shotsNoReason');
  return e.text;
}

// ---------------------------------------------------------------- «Подробнее»

export function ShotsCard({ shots, onOpen }: { shots: MacroShots | null; onOpen: (i: number) => void }) {
  if (!shots?.ok || !shots.shots.length) return null;
  return (
    <div class="card wide shots-card">
      <div class="card-head">{t('macro.shotsTitle')}</div>
      <div class="card-body shots-grid">
        {shots.shots.map((s, i) => (
          <figure class="shot-cell" key={s.path}>
            <figcaption>{shotTabLabel(s)}</figcaption>
            <ShotImage shot={s} asof={shots.asof} onOpen={() => onOpen(i)} />
            <ShotLegendList legend={s.legend} />
          </figure>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- lightbox

export function ShotsLightbox({ shots, index, onIndex, onClose }: { shots: MacroShots; index: number; onIndex: (i: number) => void; onClose: () => void }) {
  const list = shots.shots;
  const i = Math.max(0, Math.min(index, list.length - 1));
  const shot = list[i];
  const src = useShotImage(shot?.path ?? '', shots.asof);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      } else if (e.key === 'ArrowRight' && list.length > 1) onIndex((i + 1) % list.length);
      else if (e.key === 'ArrowLeft' && list.length > 1) onIndex((i - 1 + list.length) % list.length);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [i, list.length, onClose, onIndex]);

  if (!shot) return null;
  return (
    <div class="shot-lightbox" role="dialog" aria-modal="true" aria-label={shotTabLabel(shot)} onClick={onClose}>
      <div class="lb-panel" onClick={(e) => e.stopPropagation()}>
        <div class="lb-head">
          {list.length > 1 ? (
            <div class="seg lb-tabs" role="tablist">
              {list.map((s, k) => (
                <button type="button" role="tab" key={s.path} aria-selected={k === i} class={k === i ? 'on' : ''} onClick={() => onIndex(k)}>
                  {shotTabLabel(s)}
                </button>
              ))}
            </div>
          ) : (
            <b>{shotTabLabel(shot)}</b>
          )}
          <span class="spacer" />
          {shot.verify_ok === false ? <span class="shot-warn inline">⚠ {t('macro.shotsMoved')}</span> : null}
          <button type="button" class="ghost lb-close" onClick={onClose} title={t('macro.shotsClose')} aria-label={t('macro.shotsClose')}>
            ✕
          </button>
        </div>
        <div class="lb-img" onClick={onClose}>
          {src ? <img src={src} alt={shotTabLabel(shot)} /> : <span class="muted">{src === null ? t('macro.shotsMissing') : t('macro.shotsLoading')}</span>}
        </div>
        <ShotLegendList legend={shot.legend} />
      </div>
    </div>
  );
}
