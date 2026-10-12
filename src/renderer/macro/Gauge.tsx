// The LONG / SHORT gauge, drawn the way crypto exchanges draw «Fear & Greed»:
// a half dial from strong short (left) to strong long (right), five zones from
// the skill's thresholds, a needle at the scale's balance (long % − short %).
// When the owner and the agent argued, the morning needle stays as a ghost and
// the solid one shows the needle after the discussion.
import { arcPath, clampNeedle, fmt, needleAngle, zoneOf, zoneSegments, type ZoneKey } from '@shared/macro';
import { t } from '../i18n';

const W = 320;
const H = 200;
const CX = W / 2;
const CY = 172;
const R0 = 104;
const R1 = 142;

interface Props {
  needle: number;
  /** Needle before the discussion; drawn as a ghost when it differs. */
  before?: number | null;
  zones: { neutral: number; strong: number };
  /** Small variant for the window strip: no ticks, no labels. */
  mini?: boolean;
  label?: string;
}

export function zoneLabel(key: ZoneKey): string {
  return t(`macro.zone.${key}`);
}

export function signed(n: number): string {
  const r = Math.round(n);
  return `${r > 0 ? '+' : r < 0 ? '−' : ''}${Math.abs(r)}%`;
}

function Needle({ value, ghost }: { value: number; ghost?: boolean }) {
  // Drawn pointing right (0°) and rotated: SVG rotation is clockwise, the
  // geometry helper's angle is counter-clockwise.
  const rot = -needleAngle(value);
  return (
    <g class={`gauge-needle${ghost ? ' ghost' : ''}`} style={{ transform: `rotate(${rot}deg)`, transformOrigin: `${CX}px ${CY}px` }}>
      <path d={`M${CX} ${CY - 5} L${CX + R1 - 14} ${CY - 1.2} L${CX + R1 - 14} ${CY + 1.2} L${CX} ${CY + 5} Z`} />
    </g>
  );
}

export function Gauge({ needle, before, zones, mini, label }: Props) {
  const value = clampNeedle(needle);
  const zone = zoneOf(value, zones);
  const segs = zoneSegments(zones);
  const showGhost = before != null && Math.round(before) !== Math.round(value);
  return (
    <figure class={`gauge${mini ? ' mini' : ''} zone-${zone}`} aria-label={`${label ?? t('macro.gauge')}: ${signed(value)}, ${zoneLabel(zone)}`}>
      <svg viewBox={`0 0 ${W} ${mini ? CY + 8 : H}`} role="img">
        {segs.map((s) => (
          <path key={s.key} class={`gauge-seg seg-${s.key}${s.key === zone ? ' on' : ''}`} d={arcPath(CX, CY, R0, R1, s.from, s.to)}>
            <title>{`${zoneLabel(s.key)}: ${signed(s.from)} … ${signed(s.to)}`}</title>
          </path>
        ))}
        {!mini ? (
          <>
            <text class="gauge-end" x={CX - R1 + 6} y={CY + 20} text-anchor="start">
              {t('macro.short')}
            </text>
            <text class="gauge-end" x={CX + R1 - 6} y={CY + 20} text-anchor="end">
              {t('macro.long')}
            </text>
          </>
        ) : null}
        {showGhost ? <Needle value={before!} ghost /> : null}
        <Needle value={value} />
        <circle class="gauge-hub" cx={CX} cy={CY} r={mini ? 10 : 9} />
      </svg>
      {!mini ? (
        <figcaption>
          <b class="gauge-value">{signed(value)}</b>
          <span class="gauge-zone">{zoneLabel(zone)}</span>
          {showGhost ? <span class="gauge-before">{t('macro.beforeTalk', { v: signed(before!) })}</span> : null}
        </figcaption>
      ) : null}
    </figure>
  );
}

/** Long / undecided / short as one stacked bar, numbers on the bar. */
export function SplitBar({ long, short, undecided }: { long: number; short: number; undecided: number }) {
  const total = Math.max(1, long + short + undecided);
  const pct = (n: number) => `${(n * 100) / total}%`;
  return (
    <div class="split">
      <div class="split-bar" role="img" aria-label={t('macro.splitAria', { l: Math.round(long), s: Math.round(short), u: Math.round(undecided) })}>
        <span class="split-long" style={{ width: pct(long) }} title={`${t('macro.forLong')} ${fmt(long)} %`} />
        <span class="split-und" style={{ width: pct(undecided) }} title={`${t('macro.undecided')} ${fmt(undecided)} %`} />
        <span class="split-short" style={{ width: pct(short) }} title={`${t('macro.forShort')} ${fmt(short)} %`} />
      </div>
      <div class="split-legend">
        <span>
          <i class="mdot long" />
          {t('macro.forLong')} <b>{Math.round(long)} %</b>
        </span>
        <span>
          <i class="mdot und" />
          {t('macro.undecided')} <b>{Math.round(undecided)} %</b>
        </span>
        <span>
          <i class="mdot short" />
          {t('macro.forShort')} <b>{Math.round(short)} %</b>
        </span>
      </div>
    </div>
  );
}

/**
 * A diverging bar for one group: short to the left of the centre line, long to
 * the right, both on the same scale (the group's share of the whole scale).
 */
export function GroupBar({ name, share, long, short, scale }: { name: string; share: number; long: number; short: number; scale: number }) {
  const w = (n: number) => `${Math.min(50, (n * 50) / Math.max(1, scale))}%`;
  const net = long - short;
  return (
    <div class="gbar" title={`${name}: ${t('macro.share')} ${fmt(share)} % · ${t('macro.forLong')} ${fmt(long)} · ${t('macro.forShort')} ${fmt(short)}`}>
      <span class="gbar-name">
        {name} <em>{Math.round(share)} %</em>
      </span>
      <span class="gbar-track">
        <span class="gbar-short" style={{ width: w(short) }} />
        <span class="gbar-mid" />
        <span class="gbar-long" style={{ width: w(long) }} />
      </span>
      <span class={`gbar-net ${net > 0 ? 'pos' : net < 0 ? 'neg' : ''}`}>{signed(net)}</span>
    </div>
  );
}
