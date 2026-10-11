// «Обзор дня» — the macro-review MVP tab.
//
// One trading day, three windows: 09:30 Europe, 16:00 USA, 20:00 day review.
// Everything on screen is read from the files the `macro-review` skill writes
// in the vault; the tab refreshes when they change (a run from here, from the
// Task Scheduler, or from a terminal). The buttons only ever send a turn to the
// day's macro chat — the skill does the work and writes the files.
//
// Written for a person, not for a developer (the same rule as note 20): the
// first screen answers «куда, можно ли, что делать, почему» in plain words.
// Numbers, levels, versions, file names and the full factor table live under
// «Подробнее», closed by default.
//
// Note 23 — the desk augments the trader, it does not replace him: before the
// gauge he says his own read of the window («Твой взгляд»), and at 20:00 he
// answers two questions about his day («Твой день»). Those two answers are
// the only things the desk writes into the vault; the evening then shows who
// was right — him, the scale, or neither.
import { useCallback, useEffect, useMemo, useState } from 'preact/hooks';
import type { ChatMeta, Settings } from '@shared/api';
import {
  defaultWindow,
  discussPrompt,
  fmt,
  HUMAN_SIDES,
  humanHit,
  kyivNow,
  MACRO_WINDOWS,
  runCommand,
  splitFactors,
  WINDOW_TIMES,
  windowState,
  zoneOf,
  type ByRule,
  type DayAnswers,
  type DiscussionItem,
  type HumanRead,
  type HumanSide,
  type MacroDay,
  type MacroIndex,
  type MacroShots,
  type MacroWindowId,
  type Region,
  type RegionSnapshot,
  type ReviewRegion,
  type ScaleFactor,
  type ScaleLight,
  type ScaleResult,
  type Side,
  type TradeTaken,
} from '@shared/macro';
import { Markdown } from '../chat/Markdown';
import { t } from '../i18n';
import type { UiMessage } from '../state';
import { Gauge, GroupBar, signed, SplitBar, zoneLabel } from './Gauge';
import { ShotsCard, ShotsInline, ShotsLightbox } from './Shots';

interface Props {
  settings: Settings;
  /** The day's macro chat, when one exists. */
  chatFor: (date: string) => ChatMeta | null;
  /** The chat open in the app and its live state; shown here when it is this day's macro chat. */
  currentKey: string | null;
  messages: UiMessage[];
  streaming: boolean;
  onSend: (date: string, text: string) => void;
  onOpenChat: (date: string) => void;
  onPickVault: () => void;
}

/** «пятница, 9 октября» */
function humanDate(d: string): string {
  return new Intl.DateTimeFormat('ru-RU', { weekday: 'long', day: 'numeric', month: 'long', timeZone: 'UTC' }).format(new Date(`${d}T12:00:00Z`));
}

export function MacroDesk(p: Props) {
  const [index, setIndex] = useState<MacroIndex | null>(null);
  const [date, setDate] = useState(() => kyivNow().date);
  const [day, setDay] = useState<MacroDay | null>(null);
  const [win, setWin] = useState<MacroWindowId | null>(null);
  const [flash, setFlash] = useState(false);
  const [now, setNow] = useState(() => kyivNow());

  const loadIndex = useCallback(async () => {
    try {
      setIndex(await window.desk.macro.index());
    } catch {
      setIndex(null);
    }
  }, []);

  const loadDay = useCallback(async (d: string) => {
    try {
      setDay(await window.desk.macro.day(d));
    } catch {
      setDay(null);
    }
  }, []);

  useEffect(() => {
    void loadIndex();
  }, [loadIndex, p.settings.vaultPath]);

  useEffect(() => {
    void loadDay(date);
  }, [date, loadDay, p.settings.vaultPath]);

  // The files moved: refresh the index, and the day if it is the one on screen.
  useEffect(
    () =>
      window.desk.macro.onChanged((dates) => {
        void loadIndex();
        if (dates.includes(date)) {
          void loadDay(date);
          setFlash(true);
          setTimeout(() => setFlash(false), 1600);
        }
      }),
    [date, loadDay, loadIndex],
  );

  // The window states depend on the clock.
  useEffect(() => {
    const id = setInterval(() => setNow(kyivNow()), 30_000);
    return () => clearInterval(id);
  }, []);

  const active: MacroWindowId = win ?? defaultWindow(day, date, now);
  const today = now.date;
  const days = index?.days ?? [];
  const prevDay = days.find((d) => d < date) ?? null;
  const nextDay = [...days].reverse().find((d) => d > date && d <= today) ?? (date < today ? today : null);
  const chat = p.chatFor(date);
  const mine = Boolean(chat && chat.key === p.currentKey);
  const messages = mine ? p.messages : null;
  const streaming = mine && p.streaming;
  const go = (d: string) => {
    setDate(d);
    setWin(null);
  };

  if (index && !index.ok) {
    return (
      <section class="macro">
        <div class="macro-empty card">
          <div class="card-head">{t('macro.setupTitle')}</div>
          <div class="card-body">
            <p>{t('macro.noVault')}</p>
            <button type="button" class="primary" onClick={p.onPickVault}>
              {t('macro.pickVault')}
            </button>
          </div>
        </div>
      </section>
    );
  }

  const state = windowState(active, day, date, now);
  // Note 23, «сначала человек»: on the day itself a window with a review but no
  // read from the owner shows nothing of its direction — not the tile's needle,
  // not the agent's text, not the plan — until he answers or asks to peek.
  const [peek, setPeek] = useState<MacroWindowId | null>(null);
  useEffect(() => setPeek(null), [date]);
  const unread = (id: MacroWindowId): boolean => {
    if (date !== today || !day) return false;
    // The evening tile's «N из 2 верно» would tell on the scale before the owner answered.
    if (id === 'review') return unread('eu') || unread('us');
    const r = day[id];
    return Boolean(r.scale || r.verdict) && !r.human && peek !== id;
  };
  return (
    <section class={`macro${flash ? ' flash' : ''}`}>
      <header class="macro-bar">
        <button type="button" class="ghost nav" disabled={!prevDay} onClick={() => prevDay && go(prevDay)} title={t('macro.prevDay')}>
          ‹
        </button>
        <h2 class="macro-day">{humanDate(date)}</h2>
        <button type="button" class="ghost nav" disabled={!nextDay} onClick={() => nextDay && go(nextDay)} title={t('macro.nextDay')}>
          ›
        </button>
        {date !== today ? (
          <button type="button" class="ghost" onClick={() => go(today)}>
            {t('macro.today')}
          </button>
        ) : null}
        <span class="spacer" />
        {chat ? (
          <button type="button" class="ghost" onClick={() => p.onOpenChat(date)}>
            {t('macro.openChat')}
          </button>
        ) : null}
      </header>

      {index?.demo ? <div class="demo-banner">{t('macro.demoBanner')}</div> : null}

      <nav class="macro-windows" aria-label={t('macro.windows')}>
        {MACRO_WINDOWS.map((id) => (
          <WindowTile key={id} id={id} day={day} active={active === id} state={windowState(id, day, date, now)} hidden={unread(id)} onClick={() => setWin(id)} />
        ))}
      </nav>

      <AgentStrip
        streaming={streaming}
        blocked={(p.streaming && !mine) || Boolean(index?.demo)}
        messages={unread(active) ? null : messages}
        state={state}
        window={active}
        onRun={() => p.onSend(date, runCommand(active))}
        onOpenChat={() => p.onOpenChat(date)}
      />

      {active === 'review' ? (
        <ReviewView day={day} date={date} past={date < today} onOpenNote={(rel) => void window.desk.macro.openNote(rel)} />
      ) : (
        <RegionView
          snap={day?.[active] ?? null}
          day={day}
          date={date}
          past={date < today}
          peek={peek === active}
          onPeek={() => setPeek(active)}
          busy={p.streaming || Boolean(index?.demo)}
          onDiscuss={(side, share, text) => p.onSend(date, discussPrompt(active, side, share, text))}
          onOpenNote={(rel) => void window.desk.macro.openNote(rel)}
        />
      )}
    </section>
  );
}

// ---------------------------------------------------------------- plain words

/** The headline under the gauge: what the scale says, in one sentence. */
function headline(res: ScaleResult, zones: { neutral: number; strong: number }, light: ScaleLight): string {
  // A red light wins: «ищем лонг» next to «сегодня не торгуем» would contradict itself.
  return t(`${light.color === 'red' ? 'macro.headRed' : 'macro.head'}.${zoneOf(res.needle, zones)}`);
}

function lightText(l: ScaleLight): string {
  if (l.color === 'red') return t('macro.lightRed');
  if (l.color === 'yellow') return t('macro.lightYellow', { r: l.risk_pct ?? 0.5 });
  return t('macro.lightGreen', { r: l.risk_pct ?? 1 });
}

// ---------------------------------------------------------------- window strip

function WindowTile({ id, day, active, state, hidden, onClick }: { id: MacroWindowId; day: MacroDay | null; active: boolean; state: string; hidden?: boolean; onClick: () => void }) {
  const w = WINDOW_TIMES[id];
  let body = <span class="wt-empty">{t(`macro.stateLong.${state}`)}</span>;
  if (hidden) {
    body = <span class={`wt-empty${id === 'review' ? '' : ' ask'}`}>{t(id === 'review' ? 'macro.tileLater' : 'macro.tileAsk')}</span>;
  } else if (id !== 'review') {
    const s = day?.[id]?.scale;
    if (s) {
      const n = s.after?.needle ?? s.result.needle;
      const zone = zoneOf(n, day!.zones);
      body = (
        <span class="wt-val">
          <Gauge needle={n} zones={day!.zones} mini />
          <span class={`wt-num zone-${zone}`}>
            <b>{zoneLabel(zone)}</b>
            <small>{t(`macro.lightShort.${s.light.color}`)}</small>
          </span>
        </span>
      );
    }
  } else if (day?.review.mech) {
    // The needle that counts is the one after the discussion, when there was one.
    const hits = (['eu', 'us'] as Region[])
      .map((r) => {
        const x = day.review.mech?.regions[r];
        return x?.after != null ? x.score?.after_hit : x?.score?.needle_hit;
      })
      .filter((h) => h === '1' || h === '0');
    const ok = hits.filter((h) => h === '1').length;
    body = (
      <span class="wt-val">
        <span class="wt-num">
          <b>{hits.length ? t('macro.hitsOf', { ok, n: hits.length }) : t('macro.noScoreYet')}</b>
          <small>{day.review.agent ? t('macro.reflectionReady') : t('macro.reflectionWait')}</small>
        </span>
      </span>
    );
  }
  return (
    <button type="button" class={`wtile${active ? ' on' : ''} st-${state}`} onClick={onClick} aria-pressed={active}>
      <span class="wt-head">
        {t(`macro.window.${id}`)} <span class="muted">· {w.at}</span>
        <span class={`wt-badge st-${state}`}>{t(`macro.state.${state}`)}</span>
      </span>
      {body}
    </button>
  );
}

// ---------------------------------------------------------------- agent strip

function AgentStrip(p: { streaming: boolean; blocked: boolean; messages: UiMessage[] | null; state: string; window: MacroWindowId; onRun: () => void; onOpenChat: () => void }) {
  const last = p.messages ? [...p.messages].reverse().find((m) => m.role === 'assistant' || m.role === 'notice') : null;
  const has = p.state === 'ready' || p.state === 'partial';
  const title = p.streaming ? t('macro.agentBusy') : p.state === 'partial' ? t(`macro.agentPartial.${p.window}`) : has ? t('macro.agentDone') : t(`macro.agentIdle.${p.window}`);
  // Rebuilding an existing review costs minutes and tokens, so it takes two
  // taps: the second button says so and goes away on its own.
  const [confirm, setConfirm] = useState(false);
  useEffect(() => {
    if (!confirm) return;
    const id = setTimeout(() => setConfirm(false), 6000);
    return () => clearTimeout(id);
  }, [confirm]);
  useEffect(() => setConfirm(false), [p.window, p.streaming]);
  const run = () => {
    if (has && !confirm) {
      setConfirm(true);
      return;
    }
    setConfirm(false);
    p.onRun();
  };
  return (
    <div class={`agent-strip card${p.streaming ? ' busy' : ''}`}>
      <div class="agent-head">
        <span class="agent-title">
          {p.streaming ? <i class="spinner" /> : null}
          {title}
        </span>
        <span class="spacer" />
        {confirm ? <span class="muted small">{t('macro.rerunCost')}</span> : null}
        <button type="button" class={p.state === 'due' || confirm ? 'primary' : ''} disabled={p.streaming || p.blocked} onClick={run}>
          {has ? (confirm ? t('macro.rerunConfirm') : t('macro.rerun')) : t('macro.run')}
        </button>
        {p.messages ? (
          <button type="button" class="ghost" onClick={p.onOpenChat}>
            {t('macro.toChat')}
          </button>
        ) : null}
      </div>
      {last ? (
        <div class={`agent-body${last.role === 'notice' ? ` notice ${last.level}` : ''}`}>
          {last.role === 'assistant' ? <Markdown text={last.text || '…'} /> : <p>{last.text}</p>}
        </div>
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------- 09:30 / 16:00

function RegionView({ snap, day, date, past, peek, onPeek, busy, onDiscuss, onOpenNote }: { snap: RegionSnapshot | null; day: MacroDay | null; date: string; past: boolean; peek: boolean; onPeek: () => void; busy: boolean; onDiscuss: (side: Side, share: number, text: string) => void; onOpenNote: (rel: string) => void }) {
  // Which screenshot the lightbox shows; null = closed.
  const [shotOpen, setShotOpen] = useState<number | null>(null);
  const closeShot = useCallback(() => setShotOpen(null), []);
  useEffect(() => setShotOpen(null), [snap?.region, snap?.shots?.date]);
  const region = snap?.region ?? 'eu';
  const human = snap?.human ?? null;
  const hasReview = Boolean(snap && (snap.scale || snap.verdict));
  // Ask for the owner's read on the day itself, even before the review ran; a
  // past day without one is left alone. While the review is there and he has
  // not answered, the question is the whole screen («сначала человек») —
  // «Показать без ответа» is the way around, and a read given after that is
  // recorded as such (before_scale: false).
  const askRead = !human && !past;
  const sendRead = (side: HumanSide) => void window.desk.macro.humanRead({ date, region, side, beforeScale: !(peek && hasReview) });

  if (!snap || !hasReview) {
    return (
      <div class="macro-main">
        {askRead ? <HumanReadCard region={region} human={null} scaleSide={null} onPick={sendRead} /> : null}
        <div class="macro-none muted">{t('macro.noData')}</div>
      </div>
    );
  }
  if (askRead && !peek) {
    return (
      <div class="macro-main">
        <HumanReadCard region={region} human={null} scaleSide={null} onPick={sendRead} onPeek={onPeek} />
      </div>
    );
  }
  const s = snap.scale;
  const v = snap.verdict;
  const zones = day?.zones ?? { neutral: 15, strong: 40 };
  const res = s?.after ?? s?.result ?? null;
  const split = s ? splitFactors(s.factors) : null;
  const opinion = v?.plan?.length ? v.plan : s?.plan?.summary ? [s.plan.summary] : [];

  return (
    <div class="macro-main">
      {askRead || human ? <HumanReadCard region={region} human={human} scaleSide={res ? zoneOf(res.needle, zones) : null} onPick={sendRead} /> : null}
      <div class="macro-grid">
        {s && res ? (
          <div class="card hero">
            <div class="card-head">{t('macro.whereTitle')}</div>
            <div class="card-body">
              <Gauge needle={res.needle} before={s.after ? s.result.needle : null} zones={zones} />
              <p class="hero-line">{headline(res, zones, s.light)}</p>
              <SplitBar long={res.long} short={res.short} undecided={res.undecided} />
              <div class={`mlight mlight-${s.light.color}`}>
                <b>{lightText(s.light)}</b>
                {[...(s.light.red ?? []), ...(s.light.yellow ?? [])].slice(0, 1).map((x) => (
                  <div class="mlight-why">{x}</div>
                ))}
              </div>
            </div>
          </div>
        ) : null}

        <div class="card todo">
          <div class="card-head">{t('macro.todoTitle')}</div>
          <div class="card-body">
            {opinion.length ? (
              <ul class="opinion">
                {opinion.map((x) => (
                  <li>{x}</li>
                ))}
              </ul>
            ) : (
              <p class="muted">{t('macro.noOpinion')}</p>
            )}
            {s?.plan?.cancel && !v?.plan?.length ? (
              <p class="cancel">
                <b>{t('macro.cancelTitle')}</b> {s.plan.cancel}
              </p>
            ) : null}
            <ShotsInline shots={snap.shots} onOpen={setShotOpen} />
            {v ? (
              <div class="news">
                <span class={`vbadge v-${String(v.verdict).toLowerCase()}`}>{t(`macro.newsVerdict.${v.verdict}`)}</span>
                {v.summary ? <p>{v.summary}</p> : null}
              </div>
            ) : null}
          </div>
        </div>

        {split && (split.long.length || split.short.length) ? (
          <div class="card why">
            <div class="card-head">{t('macro.whyTitle')}</div>
            <div class="card-body why-cols">
              <WhyColumn side="long" list={split.long} />
              <WhyColumn side="short" list={split.short} />
            </div>
          </div>
        ) : null}

        {s ? <DiscussionCard items={s.discussion ?? []} busy={busy} onDiscuss={onDiscuss} /> : null}
      </div>

      <details class="more">
        <summary>{t('macro.more')}</summary>
        <div class="macro-grid">
          <ShotsCard shots={snap.shots} onOpen={setShotOpen} />
          {s?.plan ? <PlanCard snap={snap} /> : null}
          {v ? <NewsCard v={v} snap={snap} onOpenNote={onOpenNote} /> : null}
          {s?.result.groups ? <GroupsCard groups={s.result.groups} /> : null}
          {split ? <FactorTable split={split} day={day} region={snap.region} /> : null}
          <ContextCard snap={snap} />
          {s ? (
            <p class="tech muted">
              {t('macro.techLine', { sym: s.sym, asof: s.asof, version: s.version ?? '—' })}
              {s.history && s.history.length > 1 ? ` · ${t('macro.rebuilt', { n: s.history.length })}` : ''}
            </p>
          ) : null}
        </div>
      </details>

      {shotOpen != null && snap.shots?.ok && snap.shots.shots.length ? (
        <ShotsLightbox shots={snap.shots} index={shotOpen} onIndex={setShotOpen} onClose={closeShot} />
      ) : null}
    </div>
  );
}

// ---------------------------------------------------------------- «Твой взгляд» (note 23)

/** The scale's zone as a human side, for «вы сходитесь / расходитесь». */
function zoneSide(zone: ReturnType<typeof zoneOf> | null): HumanSide | null {
  if (!zone) return null;
  if (zone === 'neutral') return 'NEUTRAL';
  return zone.endsWith('long') ? 'LONG' : 'SHORT';
}

/**
 * The owner's read of the window, asked before the gauge is shown. One tap;
 * afterwards one line: what he said, what the scale says, whether they agree.
 */
function HumanReadCard({ region, human, scaleSide, onPick, onPeek }: { region: Region; human: HumanRead | null; scaleSide: ReturnType<typeof zoneOf> | null; onPick: (side: HumanSide) => void; onPeek?: () => void }) {
  if (!human) {
    return (
      <div class="card human ask">
        <div class="card-head">{t('macro.yourReadTitle')}</div>
        <div class="card-body">
          <p>{t(`macro.yourReadAsk.${region}`)}</p>
          <div class="seg big" role="radiogroup" aria-label={t('macro.yourReadTitle')}>
            {HUMAN_SIDES.map((x) => (
              <button type="button" role="radio" aria-checked={false} class={x === 'LONG' ? 'long' : x === 'SHORT' ? 'short' : ''} onClick={() => onPick(x)}>
                {t(`macro.humanSide.${x}`)}
              </button>
            ))}
          </div>
          <p class="muted small">
            {t('macro.yourReadWhy')}
            {onPeek ? (
              <>
                {' '}
                <button type="button" class="link" onClick={onPeek}>
                  {t('macro.veilPeek')}
                </button>
              </>
            ) : null}
          </p>
        </div>
      </div>
    );
  }
  const ss = zoneSide(scaleSide);
  const agree = ss ? (ss === human.side ? 'same' : human.side === 'NO_TRADE' || ss === 'NEUTRAL' || human.side === 'NEUTRAL' ? 'near' : 'differ') : null;
  return (
    <div class={`card human said${agree ? ` ${agree}` : ''}`}>
      <div class="card-body human-line">
        <span>
          {t('macro.yourReadWas')} <b class={`hs-${human.side}`}>{t(`macro.humanSide.${human.side}`)}</b>
          {!human.before_scale ? <span class="muted"> · {t('macro.afterGauge')}</span> : null}
        </span>
        {ss ? (
          <span>
            {t('macro.scaleSays')} <b class={`hs-${ss}`}>{t(`macro.scaleSide.${ss}`)}</b>
            <span class="muted"> — {t(`macro.agree.${agree}`)}</span>
          </span>
        ) : (
          <span class="muted">{t('macro.scaleNotYet')}</span>
        )}
      </div>
    </div>
  );
}

/** Every factor that voted for one side, strongest first. */
function WhyColumn({ side, list }: { side: 'long' | 'short'; list: ScaleFactor[] }) {
  const sum = Math.round(list.reduce((n, f) => n + f.points, 0));
  return (
    <div class={`whycol ${side}`}>
      <div class="whycol-head">
        <span class="whycol-sum">{sum} %</span>
        <span class="whycol-label">{t(side === 'long' ? 'macro.forLongTitle' : 'macro.forShortTitle')}</span>
      </div>
      {list.length ? (
        <ul class="whycol-list">
          {list.map((f) => (
            <li title={f.why}>
              <span class="whycol-w">{fmt(f.points)} %</span>
              <span class="whycol-name">{f.name}</span>
            </li>
          ))}
        </ul>
      ) : (
        <p class="muted">{t('macro.noneSide')}</p>
      )}
    </div>
  );
}

const WEIGHTS = [
  { share: 5, key: 'weak' },
  { share: 10, key: 'important' },
  { share: 20, key: 'key' },
] as const;

const VIEW_ICON: Record<string, string> = { agree: '✅', partial: '➗', disagree: '❌' };

function DiscussionCard({ items, busy, onDiscuss }: { items: DiscussionItem[]; busy: boolean; onDiscuss: (side: Side, share: number, text: string) => void }) {
  const [side, setSide] = useState<Side>('LONG');
  const [share, setShare] = useState(10);
  const [text, setText] = useState('');
  const submit = (e: Event) => {
    e.preventDefault();
    if (!text.trim() || busy) return;
    onDiscuss(side, share, text);
    setText('');
  };
  return (
    <div class="card discussion">
      <div class="card-head">{t('macro.discussion')}</div>
      <div class="card-body">
        {items.map((d) => (
          <div class={`talk ${d.side === 'LONG' ? 'long' : 'short'}`}>
            <div>
              <b>{t(d.by === 'owner' ? 'macro.byOwner' : 'macro.byAgent')}</b>
              <span class="muted"> · {t(d.side === 'LONG' ? 'macro.forLong' : 'macro.forShort')}</span>
            </div>
            <div>{d.text}</div>
            {d.reply || d.agent_view ? (
              <div class="reply">
                {VIEW_ICON[d.agent_view ?? ''] ?? '💬'} <b>{t('macro.agentSays')}</b> {d.reply}
              </div>
            ) : null}
          </div>
        ))}
        <form class="talk-form" onSubmit={submit}>
          <p class="muted">{t('macro.discussHint')}</p>
          <div class="talk-row">
            <div class="seg" role="radiogroup" aria-label={t('macro.side')}>
              <button type="button" role="radio" aria-checked={side === 'LONG'} class={side === 'LONG' ? 'on long' : ''} onClick={() => setSide('LONG')}>
                {t('macro.forLongBtn')}
              </button>
              <button type="button" role="radio" aria-checked={side === 'SHORT'} class={side === 'SHORT' ? 'on short' : ''} onClick={() => setSide('SHORT')}>
                {t('macro.forShortBtn')}
              </button>
            </div>
            <div class="seg" role="radiogroup" aria-label={t('macro.weight')}>
              {WEIGHTS.map((w) => (
                <button type="button" role="radio" aria-checked={share === w.share} class={share === w.share ? 'on' : ''} onClick={() => setShare(w.share)} title={`${w.share} %`}>
                  {t(`macro.weightName.${w.key}`)}
                </button>
              ))}
            </div>
          </div>
          <textarea rows={2} value={text} placeholder={t('macro.argPlaceholder')} onInput={(e) => setText((e.target as HTMLTextAreaElement).value)} />
          <button type="submit" class="primary" disabled={busy || !text.trim()}>
            {t('macro.sendArg')}
          </button>
        </form>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- «Подробнее»

function PlanCard({ snap }: { snap: RegionSnapshot }) {
  const plan = snap.scale!.plan!;
  const adr = (x?: number | null) => (x != null ? t('macro.adrAway', { x: fmt(x, 2) }) : '');
  return (
    <div class="card plan">
      <div class="card-head">{t('macro.planTitle')}</div>
      <div class="card-body">
        <div class="levels">
          {plan.manipulation_levels?.length ? (
            <div>
              <div class="sub">{t('macro.manip')}</div>
              {plan.manipulation_levels.map((l) => (
                <div class="lvl">
                  <span>{l.name}</span>
                  <span class="muted">{adr(l.adr)}</span>
                  {l.price != null ? <code>{l.price}</code> : null}
                </div>
              ))}
            </div>
          ) : null}
          {plan.targets?.length ? (
            <div>
              <div class="sub">{t('macro.targets')}</div>
              {plan.targets.map((l) => (
                <div class="lvl">
                  <span>{l.name}</span>
                  <span class="muted">{adr(l.adr)}</span>
                  {l.price != null ? <code>{l.price}</code> : null}
                </div>
              ))}
            </div>
          ) : null}
        </div>
        {plan.setups?.map((x) => (
          <div class="setup">
            <b>{x.setup}</b> {x.why ? <span class="muted">— {x.why}</span> : null}
            <dl>
              {x.entry ? (
                <>
                  <dt>{t('macro.entry')}</dt>
                  <dd>{x.entry}</dd>
                </>
              ) : null}
              {x.stop ? (
                <>
                  <dt>{t('macro.stop')}</dt>
                  <dd>{x.stop}</dd>
                </>
              ) : null}
              {x.target ? (
                <>
                  <dt>{t('macro.target')}</dt>
                  <dd>{x.target}</dd>
                </>
              ) : null}
              {x.until ? (
                <>
                  <dt>{t('macro.until')}</dt>
                  <dd>{x.until}</dd>
                </>
              ) : null}
            </dl>
          </div>
        ))}
      </div>
    </div>
  );
}

function List({ title, items }: { title: string; items?: string[] }) {
  if (!items?.length) return null;
  return (
    <div>
      <div class="sub">{title}</div>
      <ul>
        {items.map((x) => (
          <li>{x}</li>
        ))}
      </ul>
    </div>
  );
}

function NewsCard({ v, snap, onOpenNote }: { v: NonNullable<RegionSnapshot['verdict']>; snap: RegionSnapshot; onOpenNote: (rel: string) => void }) {
  return (
    <div class="card mverdict">
      <div class="card-head">
        {t('macro.newsTitle')}
        <span class="spacer" />
        {snap.note ? (
          <button type="button" class="ghost small" onClick={() => onOpenNote(snap.note!)}>
            {t('macro.openNote')}
          </button>
        ) : null}
      </div>
      <div class="card-body">
        <List title={t('macro.reasons')} items={v.reasons} />
        <List title={t('macro.risks')} items={v.risks} />
        <List title={t('macro.watch')} items={v.watch} />
        <List title={t('macro.judgment')} items={v.scale_judgment?.map((j) => `${t(j.side === 'LONG' ? 'macro.forLong' : 'macro.forShort')}: ${j.text}`)} />
        {v.sources?.length ? (
          <div>
            <div class="sub">{t('macro.sources')}</div>
            <ul class="sources">
              {v.sources.map((x) => (
                <li>
                  {x.url ? (
                    <a href={x.url} onClick={(e) => (e.preventDefault(), void window.desk.open.external(x.url!))}>
                      {x.title}
                    </a>
                  ) : (
                    x.title
                  )}
                  {x.source ? <span class="muted"> — {x.source}</span> : null}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function GroupsCard({ groups }: { groups: NonNullable<ScaleResult['groups']> }) {
  const list = Object.entries(groups);
  const gmax = Math.max(1, ...list.map(([, g]) => g.share));
  return (
    <div class="card">
      <div class="card-head">{t('macro.groups')}</div>
      <div class="card-body">
        {list.map(([name, g]) => (
          <GroupBar name={name} share={g.share} long={g.long} short={g.short} scale={gmax} />
        ))}
        <p class="muted small">{t('macro.groupsNote')}</p>
      </div>
    </div>
  );
}

function FactorTable({ split, day, region }: { split: ReturnType<typeof splitFactors>; day: MacroDay | null; region: Region }) {
  const stats = new Map((day?.stats ?? []).filter((x) => x.region === region).map((x) => [x.factor, x]));
  const rows = [...split.long.map((f) => ({ f, side: 'long' })), ...split.short.map((f) => ({ f, side: 'short' })), ...split.silent.map((f) => ({ f, side: '' }))];
  return (
    <div class="card wide">
      <div class="card-head">{t('macro.allFactors')}</div>
      <div class="card-body">
        <table class="ftable">
          <thead>
            <tr>
              <th>{t('macro.colFactor')}</th>
              <th>{t('macro.colSide')}</th>
              <th>{t('macro.colWeight')}</th>
              <th>{t('macro.colGave')}</th>
              <th>{t('macro.colHit')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.map(({ f, side }) => {
              const st = stats.get(f.id);
              return (
                <tr title={f.why}>
                  <td>{f.name}</td>
                  <td class={side === 'long' ? 'pos' : side === 'short' ? 'neg' : 'muted'}>{side ? t(side === 'long' ? 'macro.forLong' : 'macro.forShort') : t('macro.silentShort')}</td>
                  <td>{Math.round(f.share)} %</td>
                  <td>{f.points ? `${fmt(f.points)} %` : '—'}</td>
                  <td class="muted">{st && st.total >= 3 ? `${Math.round((st.hits * 100) / st.total)} % (${st.total})` : '—'}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <p class="muted small">{t('macro.factorNote')}</p>
      </div>
    </div>
  );
}

function ContextCard({ snap }: { snap: RegionSnapshot }) {
  const c = snap.collect;
  const shape = snap.verdict?.day_shape;
  const events = c?.calendar?.events ?? [];
  const markets = (c?.markets ?? []).filter((m) => m.chg_pct != null);
  if (!c && !shape) return null;
  return (
    <div class="card wide context">
      <div class="card-head">{t('macro.context')}</div>
      <div class="card-body ctx">
        {shape ? (
          <div>
            <div class="sub">{t('macro.dayShape')}</div>
            {shape.summary ? <p>{shape.summary}</p> : null}
            {shape.windows?.map((w) => (
              <div class="lvl">
                <code>{w.window}</code>
                <span>{w.expect}</span>
              </div>
            ))}
          </div>
        ) : null}
        {events.length ? (
          <div>
            <div class="sub">{t('macro.calendar')}</div>
            {events.slice(0, 10).map((e) => (
              <div class="lvl">
                <code>{e.time_eet ?? ''}</code>
                <span>{e.title}</span>
                {e.impact === 'High' ? <span class="imp imp-high">{t('macro.important')}</span> : null}
              </div>
            ))}
          </div>
        ) : null}
        {markets.length ? (
          <div>
            <div class="sub">{t('macro.markets')}</div>
            {markets.slice(0, 14).map((m) => (
              <div class="lvl">
                <span>{m.label}</span>
                <span class={m.chg_pct! > 0 ? 'pos' : m.chg_pct! < 0 ? 'neg' : ''}>
                  {m.chg_pct! > 0 ? '+' : ''}
                  {fmt(m.chg_pct, 2)} %
                </span>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- 20:00

function ReviewView({ day, date, past, onOpenNote }: { day: MacroDay | null; date: string; past: boolean; onOpenNote: (rel: string) => void }) {
  const mech = day?.review.mech;
  const agent = day?.review.agent;
  const [shotOpen, setShotOpen] = useState<{ region: Region; index: number } | null>(null);
  useEffect(() => setShotOpen(null), [day?.date]);
  // The owner's two questions come first (note 23): on the day itself even
  // before the agent's review exists; on a past day only when he answered.
  const answers = day?.answers ?? null;
  const askDay = !past || Boolean(answers);
  const saveAnswers = (regions: DayAnswers['regions'], note: string) => void window.desk.macro.dayAnswers({ date, regions, note });
  if (!day || (!mech && !agent)) {
    return (
      <div class="macro-main">
        {askDay ? <DayAnswersCard answers={answers} onSave={saveAnswers} /> : null}
        <div class="macro-none muted">{t('macro.noReview')}</div>
      </div>
    );
  }
  const openShots = shotOpen ? day.review.shots?.[shotOpen.region] : null;
  return (
    <div class="macro-main">
      {askDay ? <DayAnswersCard answers={answers} onSave={saveAnswers} /> : null}
      <div class="macro-grid">
        {agent?.summary ? (
          <div class="card wide">
            <div class="card-head">
              {t('macro.reviewSummary')}
              <span class="spacer" />
              {day.review.note ? (
                <button type="button" class="ghost small" onClick={() => onOpenNote(day.review.note!)}>
                  {t('macro.openNote')}
                </button>
              ) : null}
            </div>
            <div class="card-body">
              <p class="hero-line left">{agent.summary}</p>
            </div>
          </div>
        ) : null}
        {(['eu', 'us'] as Region[]).map((r) => (
          <ReviewRegionCard region={r} rv={mech?.regions[r]} reflection={agent?.regions?.[r]} zones={day.zones} shots={day.review.shots?.[r] ?? null} human={day[r].human} answer={answers?.regions[r]} past={past} onOpenShot={(index) => setShotOpen({ region: r, index })} />
        ))}
        {agent?.lessons?.length ? (
          <div class="card wide">
            <div class="card-head">{t('macro.lessonsToday')}</div>
            <div class="card-body">
              <ul class="lessons">
                {agent.lessons.map((x, i) => (
                  <li class={i === 0 ? 'main' : 'rest'}>{x}</li>
                ))}
              </ul>
            </div>
          </div>
        ) : null}
      </div>

      {shotOpen && openShots?.ok && openShots.shots.length ? (
        <ShotsLightbox shots={openShots} index={shotOpen.index} onIndex={(index) => setShotOpen({ region: shotOpen.region, index })} onClose={() => setShotOpen(null)} />
      ) : null}

      <details class="more">
        <summary>{t('macro.moreReview')}</summary>
        <div class="macro-grid">
          {(['eu', 'us'] as Region[]).map((r) => (
            <FactorHits region={r} rv={mech?.regions[r]} names={day.names} />
          ))}
          <StatsCard day={day} />
          {agent?.weight_ideas?.length ? (
            <div class="card">
              <div class="card-head">{t('macro.weightIdeas')}</div>
              <div class="card-body">
                <ul>
                  {agent.weight_ideas.map((x) => (
                    <li>{x}</li>
                  ))}
                </ul>
              </div>
            </div>
          ) : null}
          {day.lessons.length ? (
            <div class="card">
              <div class="card-head">{t('macro.lessonsLog')}</div>
              <div class="card-body">
                <ul class="lessons small">
                  {day.lessons.map((x) => (
                    <li>{x}</li>
                  ))}
                </ul>
              </div>
            </div>
          ) : null}
        </div>
      </details>
    </div>
  );
}

const PLAN_TEXT: Record<string, string> = { да: 'planYes', нет: 'planNo', 'нет пробоя': 'planNoBreak', 'пробой без IB1': 'planHalf' };

function ReviewRegionCard({ region, rv, reflection, zones, shots, human, answer, past, onOpenShot }: { region: Region; rv?: ReviewRegion; reflection?: { what_worked?: string; what_failed?: string; discussion?: string }; zones: { neutral: number; strong: number }; shots: MacroShots | null; human: HumanRead | null; answer?: DayAnswers['regions'][Region]; past: boolean; onOpenShot: (i: number) => void }) {
  if (!rv || rv.missing) {
    return (
      <div class="card">
        <div class="card-head">{t(`macro.window.${region}`)}</div>
        <div class="card-body muted">{t('macro.noReviewRegion')}</div>
      </div>
    );
  }
  const f = rv.facts ?? {};
  const sc = rv.score ?? {};
  const dir = String(f.dir ?? 'FLAT');
  const needle = rv.after ?? rv.needle ?? 0;
  const afterSkip = rv.after != null && zoneOf(rv.after, zones) === 'neutral';
  const hit = (rv.after != null ? sc.after_hit : sc.needle_hit) as string | undefined;
  const plan = sc.plan_hit ? PLAN_TEXT[String(sc.plan_hit)] : null;
  return (
    <div class="card review-region">
      <div class="card-head">
        {t(`macro.window.${region}`)}
        {f.partial ? <span class="muted"> · {t('macro.partial')}</span> : null}
      </div>
      <div class="card-body">
        <div class="rv-top">
          <Gauge needle={needle} before={rv.after != null ? rv.needle : null} zones={zones} mini />
          <div class="rv-arrow">
            <div>
              {t('macro.factWas')} <b class={dir === 'LONG' ? 'pos' : dir === 'SHORT' ? 'neg' : ''}>{t(`macro.dir.${dir}`)}</b>
            </div>
            {rv.after != null && sc.needle_hit ? <div class="muted small">{t(sc.needle_hit === '1' ? 'macro.morningRight' : 'macro.morningWrong')}</div> : null}
          </div>
        </div>
        <WhoWasRight human={human} answer={answer} past={past} dir={dir} scaleZone={zoneLabel(zoneOf(needle, zones))} scaleHit={afterSkip ? 'skip' : hit} plan={plan} />
        <ShotsInline shots={shots} onOpen={onOpenShot} />
        {reflection ? (
          <div class="reflect">
            {reflection.what_worked ? (
              <p>
                <b>{t('macro.worked')}</b> {reflection.what_worked}
              </p>
            ) : null}
            {reflection.what_failed ? (
              <p>
                <b>{t('macro.failed')}</b> {reflection.what_failed}
              </p>
            ) : null}
            {reflection.discussion ? (
              <p>
                <b>{t('macro.whoRight')}</b> {reflection.discussion}
              </p>
            ) : null}
          </div>
        ) : (
          <p class="muted">{t('macro.noReflection')}</p>
        )}
      </div>
    </div>
  );
}

/**
 * «Кто был прав»: the owner's morning read, the scale, the plan — one small
 * table, each scored against the window. Scored here from the window's
 * direction, so it shows the moment `scale.py review` has written the facts,
 * before the agent's reflection. On a past day a missing read is not nagged.
 */
function WhoWasRight({ human, answer, past, dir, scaleZone, scaleHit, plan }: { human: HumanRead | null; answer?: DayAnswers['regions'][Region]; past: boolean; dir: string; scaleZone: string; scaleHit?: string; plan: string | null }) {
  const hh = humanHit(human?.side, dir);
  const mark = (h: boolean | null) => (h == null ? '' : h ? ' good' : ' bad');
  const trade = answer ? (answer.trade === 'none' ? t('macro.tradeNone') : `${t(`macro.trade.${answer.trade}`)}${answer.by_rule ? ` · ${t(`macro.byRule.${answer.by_rule}`)}` : ''}`) : '';
  return (
    <div class="who">
      {human || !past ? (
        <div class="who-row">
          <span class="who-k">{t('macro.whoYou')}</span>
          {human ? (
            <span class={`chip${mark(hh)}`}>
              {t(`macro.humanSide.${human.side}`)}
              {hh == null ? '' : hh ? ' ✓' : ' ✗'}
            </span>
          ) : (
            <span class="muted">{t('macro.whoNoRead')}</span>
          )}
          {trade ? <span class="muted small">{trade}</span> : null}
        </div>
      ) : trade ? (
        <div class="who-row">
          <span class="who-k">{t('macro.whoYou')}</span>
          <span class="muted small">{trade}</span>
        </div>
      ) : null}
      <div class="who-row">
        <span class="who-k">{t('macro.whoScale')}</span>
        <span class={`chip${scaleHit === '1' ? ' good' : scaleHit === '0' ? ' bad' : ''}`}>
          {scaleZone}
          {scaleHit === '1' ? ' ✓' : scaleHit === '0' ? ' ✗' : ''}
        </span>
        {scaleHit === 'skip' ? <span class="muted small">{t('macro.hitSkip')}</span> : scaleHit !== '1' && scaleHit !== '0' ? <span class="muted small">{t('macro.hitNone')}</span> : null}
      </div>
      {plan ? (
        <div class="who-row">
          <span class="who-k">{t('macro.whoPlan')}</span>
          <span class={`chip${plan === 'planYes' ? ' good' : plan === 'planNo' ? ' bad' : ''}`}>{t(`macro.${plan}`)}</span>
        </div>
      ) : null}
    </div>
  );
}

const TRADES: TradeTaken[] = ['none', 'long', 'short'];
const BY_RULES: ByRule[] = ['yes', 'no', 'unsure'];

/**
 * «Твой день» — the two evening questions (note 23): which trade, if any, in
 * each window and whether by the rules; and one line on what helped or got in
 * the way. Saved at once; the agent reads it for the review.
 */
function DayAnswersCard({ answers, onSave }: { answers: DayAnswers | null; onSave: (regions: DayAnswers['regions'], note: string) => void }) {
  const [editing, setEditing] = useState(!answers);
  const [regions, setRegions] = useState<DayAnswers['regions']>(() => answers?.regions ?? {});
  const [note, setNote] = useState(answers?.note ?? '');
  useEffect(() => {
    setEditing(!answers);
    setRegions(answers?.regions ?? {});
    setNote(answers?.note ?? '');
  }, [answers?.at, answers?.date]);
  const set = (r: Region, patch: Partial<DayAnswers['regions'][Region] & object>) => setRegions((cur) => ({ ...cur, [r]: { trade: 'none', ...(cur[r] ?? {}), ...patch } }));
  const complete = (['eu', 'us'] as Region[]).every((r) => regions[r]?.trade);
  const submit = (e: Event) => {
    e.preventDefault();
    if (!complete) return;
    onSave(regions, note);
    setEditing(false);
  };
  if (!editing && answers) {
    return (
      <div class="card human said">
        <div class="card-body human-line">
          {(['eu', 'us'] as Region[]).map((r) => {
            const a = answers.regions[r];
            return (
              <span>
                {t(`macro.window.${r}`)}: <b>{!a || a.trade === 'none' ? t('macro.tradeNone') : t(`macro.trade.${a.trade}`)}</b>
                {a?.by_rule && a.trade !== 'none' ? <span class="muted"> · {t(`macro.byRule.${a.by_rule}`)}</span> : null}
              </span>
            );
          })}
          {answers.note ? <span class="who-note">«{answers.note}»</span> : null}
          <button type="button" class="ghost small" onClick={() => setEditing(true)}>
            {t('macro.edit')}
          </button>
        </div>
      </div>
    );
  }
  return (
    <form class="card human ask" onSubmit={submit}>
      <div class="card-head">{t('macro.yourDayTitle')}</div>
      <div class="card-body">
        <p>{t('macro.yourDayQ1')}</p>
        {(['eu', 'us'] as Region[]).map((r) => {
          const a = regions[r];
          return (
            <div class="talk-row answers-row">
              <span class="who-k">{t(`macro.window.${r}`)}</span>
              <div class="seg" role="radiogroup" aria-label={t(`macro.window.${r}`)}>
                {TRADES.map((x) => (
                  <button type="button" role="radio" aria-checked={a?.trade === x} class={a?.trade === x ? `on ${x}` : ''} onClick={() => set(r, { trade: x })}>
                    {x === 'none' ? t('macro.tradeNone') : t(`macro.trade.${x}`)}
                  </button>
                ))}
              </div>
              {a && a.trade !== 'none' ? (
                <div class="seg" role="radiogroup" aria-label={t('macro.byRuleQ')}>
                  {BY_RULES.map((x) => (
                    <button type="button" role="radio" aria-checked={a.by_rule === x} class={a.by_rule === x ? 'on' : ''} onClick={() => set(r, { by_rule: x })}>
                      {t(`macro.byRule.${x}`)}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })}
        <p>{t('macro.yourDayQ2')}</p>
        <input type="text" maxLength={300} value={note} placeholder={t('macro.yourDayPlaceholder')} onInput={(e) => setNote((e.target as HTMLInputElement).value)} />
        <div class="talk-row">
          <button type="submit" class="primary" disabled={!complete}>
            {t('macro.save')}
          </button>
          {answers ? (
            <button type="button" class="ghost" onClick={() => setEditing(false)}>
              {t('macro.cancel')}
            </button>
          ) : null}
        </div>
        <p class="muted small">{t('macro.yourDayWhy')}</p>
      </div>
    </form>
  );
}

function FactorHits({ region, rv, names }: { region: Region; rv?: ReviewRegion; names: Record<string, string> }) {
  const factors = (rv?.factors ?? []).filter((x) => x.hit === '1' || x.hit === '0');
  if (!factors.length) return null;
  return (
    <div class="card">
      <div class="card-head">{t('macro.factorHits', { r: t(`macro.window.${region}`) })}</div>
      <div class="card-body rv-factors">
        {factors.map((x) => (
          <span class={`fhit ${x.hit === '1' ? 'good' : 'bad'}`}>
            {x.hit === '1' ? '✓' : '✗'} {names[x.factor] ?? (x.factor.startsWith('talk_') ? t('macro.talkArg') : x.factor)}
          </span>
        ))}
      </div>
    </div>
  );
}

function StatsCard({ day }: { day: MacroDay }) {
  const rows = useMemo(() => [...day.stats].filter((s) => s.total >= 1).sort((a, b) => b.hits / b.total - a.hits / a.total), [day.stats]);
  if (!rows.length) return null;
  return (
    <div class="card">
      <div class="card-head">{t('macro.stats')}</div>
      <div class="card-body">
        {rows.map((s) => {
          const r = s.hits / s.total;
          return (
            <div class="stat-row" title={t('macro.statTip', { h: s.hits, n: s.total })}>
              <span class="fname">
                {day.names[s.factor] ?? s.factor} <small class="muted">{t(`macro.window.${s.region}`)}</small>
              </span>
              <span class="stat-track">
                <span class={`stat-fill ${r >= 0.6 ? 'good' : r < 0.45 ? 'bad' : ''}`} style={{ width: `${r * 100}%` }} />
                <span class="stat-50" />
              </span>
              <span class="fpts">{t('macro.hitsOf', { ok: s.hits, n: s.total })}</span>
            </div>
          );
        })}
        <p class="muted small">{t('macro.statsNote')}</p>
      </div>
    </div>
  );
}
