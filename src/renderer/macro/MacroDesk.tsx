// «Макро-ревью» — the macro-review MVP tab.
//
// One trading day, three windows: 09:30 Europe, 16:00 USA, 20:00 day review.
// Everything on screen is read from the files the `macro-review` skill writes
// in the vault; the tab refreshes when they change (a run from here, from the
// Task Scheduler, or from a terminal). The buttons only ever send a turn to the
// day's macro chat — the skill does the work and writes the files.
import { useCallback, useEffect, useMemo, useState } from 'preact/hooks';
import type { ChatMeta, Settings } from '@shared/api';
import {
  defaultWindow,
  discussPrompt,
  kyivNow,
  MACRO_WINDOWS,
  runCommand,
  splitFactors,
  WINDOW_TIMES,
  windowState,
  zoneOf,
  type DiscussionItem,
  type MacroDay,
  type MacroIndex,
  type MacroWindowId,
  type Region,
  type RegionSnapshot,
  type ReviewRegion,
  type ScaleFactor,
  type Side,
} from '@shared/macro';
import { Markdown } from '../chat/Markdown';
import { t } from '../i18n';
import type { UiMessage } from '../state';
import { Gauge, GroupBar, signed, SplitBar, zoneLabel } from './Gauge';

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

const LIGHT_ICON: Record<string, string> = { green: '🟢', yellow: '🟡', red: '🔴' };
const fmtDate = (d: string) => `${d.slice(8, 10)}.${d.slice(5, 7)}.${d.slice(0, 4)}`;
const shiftDate = (d: string, days: number) => new Date(Date.parse(`${d}T12:00:00Z`) + days * 86_400_000).toISOString().slice(0, 10);

export function MacroDesk(p: Props) {
  const [index, setIndex] = useState<MacroIndex | null>(null);
  const [date, setDate] = useState(() => kyivNow().date);
  const [day, setDay] = useState<MacroDay | null>(null);
  const [win, setWin] = useState<MacroWindowId | null>(null);
  const [loading, setLoading] = useState(false);
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
    setLoading(true);
    try {
      setDay(await window.desk.macro.day(d));
    } catch {
      setDay(null);
    } finally {
      setLoading(false);
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
  const days = index?.days ?? [];
  const prevDay = days.find((d) => d < date) ?? null;
  const nextDay = [...days].reverse().find((d) => d > date) ?? null;
  const chat = p.chatFor(date);
  const today = now.date;
  const mine = Boolean(chat && chat.key === p.currentKey);
  const messages = mine ? p.messages : null;
  const streaming = mine && p.streaming;

  const run = (id: MacroWindowId) => p.onSend(date, runCommand(id));

  if (index && !index.ok) {
    return (
      <section class="macro">
        <div class="macro-empty card">
          <div class="card-head">{t('macro.title')}</div>
          <div class="card-body">
            <p>{t(index.error === 'no-vault' ? 'macro.noVault' : 'macro.noStateDir', { path: index.vault })}</p>
            <button type="button" class="primary" onClick={p.onPickVault}>
              {t('macro.pickVault')}
            </button>
          </div>
        </div>
      </section>
    );
  }

  return (
    <section class={`macro${flash ? ' flash' : ''}`}>
      <header class="macro-bar">
        <div class="macro-date">
          <button type="button" class="ghost" disabled={!prevDay} onClick={() => prevDay && (setDate(prevDay), setWin(null))} title={t('macro.prevDay')}>
            ‹
          </button>
          <input type="date" value={date} onInput={(e) => (setDate((e.target as HTMLInputElement).value || today), setWin(null))} />
          <button type="button" class="ghost" disabled={!nextDay && date >= today} onClick={() => (setDate(nextDay ?? today), setWin(null))} title={t('macro.nextDay')}>
            ›
          </button>
          {date !== today ? (
            <button type="button" class="ghost" onClick={() => (setDate(today), setWin(null))}>
              {t('macro.today')}
            </button>
          ) : null}
        </div>
        <span class="macro-live" title={index?.vault ?? ''}>
          <i class="live-dot" />
          {loading ? t('macro.loading') : t('macro.watching')}
        </span>
        <span class="spacer" />
        {chat ? (
          <button type="button" class="ghost" onClick={() => p.onOpenChat(date)}>
            {t('macro.openChat')}
          </button>
        ) : null}
      </header>

      <nav class="macro-windows" aria-label={t('macro.windows')}>
        {MACRO_WINDOWS.map((id) => (
          <WindowTile key={id} id={id} day={day} active={active === id} state={windowState(id, day, date, now)} onClick={() => setWin(id)} />
        ))}
      </nav>

      <AgentStrip
        streaming={streaming}
        blocked={p.streaming && !mine}
        messages={messages}
        state={windowState(active, day, date, now)}
        window={active}
        isToday={date === today}
        onRun={() => run(active)}
        onOpenChat={() => p.onOpenChat(date)}
      />

      {active === 'review' ? (
        <ReviewView day={day} onOpenNote={(rel) => void window.desk.macro.openNote(rel)} />
      ) : (
        <RegionView
          snap={day?.[active] ?? null}
          day={day}
          busy={p.streaming}
          onDiscuss={(side, share, text) => p.onSend(date, discussPrompt(active, side, share, text))}
          onOpenNote={(rel) => void window.desk.macro.openNote(rel)}
        />
      )}
    </section>
  );
}

// ---------------------------------------------------------------- window strip

function WindowTile({ id, day, active, state, onClick }: { id: MacroWindowId; day: MacroDay | null; active: boolean; state: string; onClick: () => void }) {
  const w = WINDOW_TIMES[id];
  let body = <span class="wt-empty">{t(`macro.state.${state}`)}</span>;
  if (id !== 'review') {
    const s = day?.[id]?.scale;
    if (s) {
      const n = s.after?.needle ?? s.result.needle;
      const zone = zoneOf(n, day!.zones);
      body = (
        <span class="wt-val">
          <Gauge needle={n} zones={day!.zones} mini />
          <span class={`wt-num zone-${zone}`}>
            <b>{signed(n)}</b>
            <small>{zoneLabel(zone)}</small>
          </span>
          <span class="wt-light" title={s.light.label}>
            {LIGHT_ICON[s.light.color] ?? '⚪'}
          </span>
        </span>
      );
    }
  } else if (day?.review.mech) {
    const hits = (['eu', 'us'] as Region[]).map((r) => day.review.mech?.regions[r]?.score?.needle_hit).filter((h) => h === '1' || h === '0');
    body = (
      <span class="wt-val">
        <span class="wt-num">
          <b>{hits.length ? `${hits.filter((h) => h === '1').length}/${hits.length}` : '—'}</b>
          <small>{t('macro.needleHits')}</small>
        </span>
        <span class="wt-light">{day.review.agent ? '✍️' : '⏳'}</span>
      </span>
    );
  }
  return (
    <button type="button" class={`wtile${active ? ' on' : ''} st-${state}`} onClick={onClick} aria-pressed={active}>
      <span class="wt-head">
        <b>{w.at}</b> {t(`macro.window.${id}`)}
        <span class={`wt-badge st-${state}`}>{t(`macro.state.${state}`)}</span>
      </span>
      {body}
    </button>
  );
}

// ---------------------------------------------------------------- agent strip

function AgentStrip(p: { streaming: boolean; blocked: boolean; messages: UiMessage[] | null; state: string; window: MacroWindowId; isToday: boolean; onRun: () => void; onOpenChat: () => void }) {
  const last = p.messages ? [...p.messages].reverse().find((m) => m.role === 'assistant' || m.role === 'notice') : null;
  const tools = last && last.role === 'assistant' ? last.tools : [];
  const tool = p.streaming && tools.length ? tools[tools.length - 1] : null;
  const runLabel = t(`macro.run.${p.window}`);
  return (
    <div class={`agent-strip card${p.streaming ? ' busy' : ''}`}>
      <div class="agent-head">
        <span class="agent-title">
          {p.streaming ? <i class="spinner" /> : null}
          {p.streaming ? (tool ? t('macro.agentTool', { name: tool.name.replace(/^mcp__/, '') }) : t('macro.agentBusy')) : t('macro.agent')}
        </span>
        <span class="spacer" />
        <button type="button" class={p.state === 'due' ? 'primary' : ''} disabled={p.streaming || p.blocked} onClick={p.onRun} title={runCommand(p.window)}>
          {p.state === 'ready' || p.state === 'partial' ? t('macro.rerun') : runLabel}
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
      ) : (
        <p class="agent-hint muted">{p.isToday && p.state === 'due' ? t('macro.hintDue') : t('macro.hint')}</p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- 09:30 / 16:00

function RegionView({ snap, day, busy, onDiscuss, onOpenNote }: { snap: RegionSnapshot | null; day: MacroDay | null; busy: boolean; onDiscuss: (side: Side, share: number, text: string) => void; onOpenNote: (rel: string) => void }) {
  if (!snap || (!snap.scale && !snap.verdict)) {
    return <div class="macro-none muted">{t('macro.noData')}</div>;
  }
  const s = snap.scale;
  const v = snap.verdict;
  const zones = day?.zones ?? { neutral: 15, strong: 40 };
  const res = s?.after ?? s?.result ?? null;
  const groups = Object.entries(s?.result.groups ?? {});
  const gmax = Math.max(1, ...groups.map(([, g]) => g.share));
  const split = s ? splitFactors(s.factors) : null;
  const stats = new Map((day?.stats ?? []).filter((x) => x.region === snap.region).map((x) => [x.factor, x]));

  return (
    <div class="macro-grid">
      {s && res ? (
        <div class="card hero">
          <div class="card-head">
            {t('macro.scaleTitle', { sym: s.sym, region: t(`macro.window.${snap.region}`) })}
            <span class="spacer" />
            <span class="muted">
              {s.version} · {s.asof.slice(11)}
            </span>
          </div>
          <div class="card-body">
            <Gauge needle={res.needle} before={s.after ? s.result.needle : null} zones={zones} />
            <SplitBar long={res.long} short={res.short} undecided={res.undecided} />
            <div class={`mlight mlight-${s.light.color}`}>
              <b>{s.light.label}</b>
              {s.light.risk_pct != null ? <span class="muted"> · {t('macro.risk', { r: s.light.risk_pct })}</span> : null}
              {[...(s.light.red ?? []), ...(s.light.yellow ?? [])].map((x) => (
                <div class="mlight-why">{x}</div>
              ))}
            </div>
            {zoneOf(res.needle, zones) === 'neutral' ? <p class="note-neutral">{t('macro.neutralSkip')}</p> : null}
            {s.history && s.history.length > 1 ? (
              <div class="history">
                <span class="muted">{t('macro.history')}</span>
                {s.history.slice(-6).map((h) => (
                  <span class="chip" title={h.asof}>
                    {h.asof.slice(11)} {signed(h.needle)}
                    {h.after != null ? ` → ${signed(h.after)}` : ''}
                  </span>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      ) : null}

      {v ? <VerdictCard v={v} snap={snap} onOpenNote={onOpenNote} /> : <div class="card"><div class="card-body muted">{t('macro.noVerdict')}</div></div>}

      {s?.plan ? <PlanCard snap={snap} /> : null}

      {groups.length ? (
        <div class="card">
          <div class="card-head">{t('macro.groups')}</div>
          <div class="card-body">
            {groups.map(([name, g]) => (
              <GroupBar name={name} share={g.share} long={g.long} short={g.short} scale={gmax} />
            ))}
            <p class="card-note">{t('macro.groupsNote')}</p>
          </div>
        </div>
      ) : null}

      {split ? (
        <div class="card wide">
          <div class="card-head">{t('macro.factors')}</div>
          <div class="card-body factors">
            <FactorColumn title={t('macro.forLong')} side="long" list={split.long} stats={stats} />
            <FactorColumn title={t('macro.forShort')} side="short" list={split.short} stats={stats} />
            {split.silent.length ? (
              <details class="silent">
                <summary>{t('macro.silent', { n: split.silent.length })}</summary>
                {split.silent.map((f) => (
                  <div class="frow silent-row" title={f.why}>
                    <span class="fname">{f.name}</span>
                    <span class="muted">{f.why}</span>
                  </div>
                ))}
              </details>
            ) : null}
          </div>
        </div>
      ) : null}

      {s ? <DiscussionCard items={s.discussion ?? []} busy={busy} onDiscuss={onDiscuss} /> : null}

      <ContextCard snap={snap} />
    </div>
  );
}

function FactorColumn({ title, side, list, stats }: { title: string; side: 'long' | 'short'; list: ScaleFactor[]; stats: Map<string, { hits: number; total: number }> }) {
  const max = Math.max(1, ...list.map((f) => f.share));
  const sum = list.reduce((n, f) => n + f.points, 0);
  return (
    <div class={`fcol ${side}`}>
      <div class="fcol-head">
        {title} <b>{Math.round(sum)} %</b>
      </div>
      {list.length ? (
        list.map((f) => {
          const st = stats.get(f.id);
          return (
            <div class="frow" title={f.why}>
              <span class="fname">
                {f.name}
                <small class="muted"> · {f.group}</small>
              </span>
              <span class="fbar">
                <span class="fbar-share" style={{ width: `${(f.share * 100) / max}%` }}>
                  <span class="fbar-pts" style={{ width: `${Math.max(0, Math.min(1, f.strength)) * 100}%` }} />
                </span>
              </span>
              <span class="fpts">
                {f.points.toFixed(1)}
                <small class="muted">/{f.share.toFixed(0)}</small>
              </span>
              {st && st.total >= 3 ? <span class={`fstat ${st.hits / st.total >= 0.6 ? 'good' : st.hits / st.total < 0.45 ? 'bad' : ''}`} title={t('macro.statTip', { h: st.hits, n: st.total })}>{Math.round((st.hits * 100) / st.total)}%</span> : <span class="fstat" />}
              {f.why ? <span class="fwhy">{f.why}</span> : null}
            </div>
          );
        })
      ) : (
        <div class="muted">{t('macro.noneSide')}</div>
      )}
    </div>
  );
}

function VerdictCard({ v, snap, onOpenNote }: { v: NonNullable<RegionSnapshot['verdict']>; snap: RegionSnapshot; onOpenNote: (rel: string) => void }) {
  return (
    <div class="card mverdict">
      <div class="card-head">
        {t('macro.verdictTitle')}
        <span class={`vbadge v-${String(v.verdict).toLowerCase()}`}>{t(`macro.verdict.${v.verdict}`)}</span>
        {v.confidence ? <span class="muted">{t('macro.confidence', { c: t(`macro.conf.${v.confidence}`) })}</span> : null}
        <span class="spacer" />
        {snap.note ? (
          <button type="button" class="ghost small" onClick={() => onOpenNote(snap.note!)}>
            {t('macro.openNote')}
          </button>
        ) : null}
      </div>
      <div class="card-body">
        {v.summary ? <p class="vsum">{v.summary}</p> : null}
        {v.plan?.length ? (
          <div class="opinion">
            <div class="sub">{t('macro.opinion')}</div>
            <ul>
              {v.plan.map((x) => (
                <li>{x}</li>
              ))}
            </ul>
          </div>
        ) : null}
        {v.reasons?.length ? (
          <details open={!v.plan?.length}>
            <summary>{t('macro.reasons', { n: v.reasons.length })}</summary>
            <ul>
              {v.reasons.map((x) => (
                <li>{x}</li>
              ))}
            </ul>
          </details>
        ) : null}
        {v.risks?.length ? (
          <details>
            <summary>{t('macro.risks', { n: v.risks.length })}</summary>
            <ul>
              {v.risks.map((x) => (
                <li>{x}</li>
              ))}
            </ul>
          </details>
        ) : null}
        {v.watch?.length ? (
          <details>
            <summary>{t('macro.watch', { n: v.watch.length })}</summary>
            <ul>
              {v.watch.map((x) => (
                <li>{x}</li>
              ))}
            </ul>
          </details>
        ) : null}
        {v.scale_judgment?.length ? (
          <div class="judg">
            <div class="sub">{t('macro.judgment')}</div>
            {v.scale_judgment.map((j) => (
              <div class={`talk ${j.side === 'LONG' ? 'long' : 'short'}`}>
                <b>{t(j.side === 'LONG' ? 'macro.long' : 'macro.short')}</b> {j.text}
              </div>
            ))}
          </div>
        ) : null}
        {v.sources?.length ? (
          <details>
            <summary>{t('macro.sources', { n: v.sources.length })}</summary>
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
          </details>
        ) : null}
      </div>
    </div>
  );
}

function PlanCard({ snap }: { snap: RegionSnapshot }) {
  const plan = snap.scale!.plan!;
  const adr = (x?: number | null) => (x != null ? `${x.toFixed(2)} ADR` : '');
  return (
    <div class="card plan">
      <div class="card-head">
        {t('macro.planTitle')}
        {plan.side ? <span class={`vbadge v-${plan.side.toLowerCase()}`}>{t(`macro.verdict.${plan.side}`)}</span> : null}
        <span class="spacer" />
        {plan.session?.entry ? <span class="muted">{t('macro.entryWindow', { w: plan.session.entry })}</span> : null}
      </div>
      <div class="card-body">
        {plan.summary ? <p class="vsum">{plan.summary}</p> : null}
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
        {plan.cancel ? <p class="cancel">⛔ {plan.cancel}</p> : null}
      </div>
    </div>
  );
}

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
        {items.length ? (
          items.map((d) => (
            <div class={`talk ${d.side === 'LONG' ? 'long' : 'short'}`}>
              <div>
                <b>{t(d.by === 'owner' ? 'macro.byOwner' : 'macro.byAgent')}</b> · {t(d.side === 'LONG' ? 'macro.long' : 'macro.short')} {Math.round(d.share)} %{d.at ? <span class="muted"> · {d.at.slice(11)}</span> : null}
              </div>
              <div>{d.text}</div>
              {d.reply || d.agent_view ? (
                <div class="reply">
                  {VIEW_ICON[d.agent_view ?? ''] ?? '💬'} {d.agent_view ? t(`macro.view.${d.agent_view}`) : ''} {d.reply}
                </div>
              ) : null}
            </div>
          ))
        ) : (
          <p class="muted">{t('macro.noDiscussion')}</p>
        )}
        <form class="talk-form" onSubmit={submit}>
          <div class="seg" role="radiogroup" aria-label={t('macro.side')}>
            <button type="button" role="radio" aria-checked={side === 'LONG'} class={side === 'LONG' ? 'on long' : ''} onClick={() => setSide('LONG')}>
              {t('macro.long')}
            </button>
            <button type="button" role="radio" aria-checked={side === 'SHORT'} class={side === 'SHORT' ? 'on short' : ''} onClick={() => setSide('SHORT')}>
              {t('macro.short')}
            </button>
          </div>
          <label class="share">
            {t('macro.shareLabel', { n: share })}
            <input type="range" min={1} max={50} step={1} value={share} onInput={(e) => setShare(Number((e.target as HTMLInputElement).value))} />
            <small class="muted">{t('macro.shareHint')}</small>
          </label>
          <textarea rows={2} value={text} placeholder={t('macro.argPlaceholder')} onInput={(e) => setText((e.target as HTMLTextAreaElement).value)} />
          <button type="submit" class="primary" disabled={busy || !text.trim()}>
            {t('macro.sendArg')}
          </button>
        </form>
      </div>
    </div>
  );
}

function ContextCard({ snap }: { snap: RegionSnapshot }) {
  const c = snap.collect;
  const shape = snap.verdict?.day_shape;
  const events = c?.calendar?.events ?? [];
  const markets = (c?.markets ?? []).filter((m) => m.chg_pct != null);
  const prev = (c?.prev_day_check ?? []) as { sym?: string; forecast?: string | null; actual?: string; first_break?: string }[];
  if (!c && !shape) return null;
  return (
    <div class="card wide context">
      <div class="card-head">{t('macro.context')}</div>
      <div class="card-body ctx">
        {shape ? (
          <div>
            <div class="sub">{t('macro.dayShape')}</div>
            <p>
              <b>{shape.forecast ?? '—'}</b>
              {shape.prob_two_sided != null ? <span class="muted"> · {t('macro.twoSided', { p: Math.round(shape.prob_two_sided * 100) })}</span> : null}
            </p>
            {shape.summary ? <p>{shape.summary}</p> : null}
            {shape.windows?.map((w) => (
              <div class="lvl">
                <code>{w.window}</code>
                <span>{w.expect}</span>
              </div>
            ))}
          </div>
        ) : null}
        {c?.calendar ? (
          <div>
            <div class="sub">
              {t('macro.calendar')} <span class={`cal cal-${(c.calendar.status ?? '').toLowerCase()}`}>{c.calendar.status}</span>
            </div>
            {(c.calendar.reasons ?? []).map((r) => (
              <div class="mlight-why">{r}</div>
            ))}
            {events.slice(0, 10).map((e) => (
              <div class="lvl">
                <code>{e.time_eet ?? ''}</code>
                <span>{e.title}</span>
                <span class={`imp imp-${(e.impact ?? '').toLowerCase()}`}>{e.impact}</span>
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
                  {m.chg_pct!.toFixed(2)} %
                </span>
              </div>
            ))}
            {c?.headlines ? <p class="muted">{t('macro.headlines', { n: c.headlines.relevant ?? 0, tone: c.headlines.tone_sum ?? 0 })}</p> : null}
          </div>
        ) : null}
        {prev.length ? (
          <div>
            <div class="sub">{t('macro.prevCheck')}</div>
            {prev.map((x) => (
              <div class="lvl">
                <span>{x.sym}</span>
                <span class="muted">{t('macro.prevLine', { f: x.forecast ?? '—', a: x.actual ?? '—' })}</span>
              </div>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- 20:00

const PLAN_HIT: Record<string, string> = { да: 'good', нет: 'bad' };

function ReviewView({ day, onOpenNote }: { day: MacroDay | null; onOpenNote: (rel: string) => void }) {
  const mech = day?.review.mech;
  const agent = day?.review.agent;
  if (!day || (!mech && !agent)) return <div class="macro-none muted">{t('macro.noReview')}</div>;
  const zones = day.zones;
  return (
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
            <p class="vsum">{agent.summary}</p>
          </div>
        </div>
      ) : null}
      {(['eu', 'us'] as Region[]).map((r) => (
        <ReviewRegionCard region={r} rv={mech?.regions[r]} reflection={agent?.regions?.[r]} names={day.names} zones={zones} />
      ))}
      {agent?.lessons?.length || agent?.weight_ideas?.length ? (
        <div class="card">
          <div class="card-head">{t('macro.lessonsToday')}</div>
          <div class="card-body">
            <ul class="lessons">
              {(agent.lessons ?? []).map((x) => (
                <li>{x}</li>
              ))}
            </ul>
            {agent.weight_ideas?.length ? (
              <>
                <div class="sub">{t('macro.weightIdeas')}</div>
                <ul>
                  {agent.weight_ideas.map((x) => (
                    <li>{x}</li>
                  ))}
                </ul>
              </>
            ) : null}
          </div>
        </div>
      ) : null}
      <StatsCard day={day} />
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
  );
}

function hitLabel(h?: string | null): { cls: string; text: string } {
  if (h === '1') return { cls: 'good', text: t('macro.hit') };
  if (h === '0') return { cls: 'bad', text: t('macro.miss') };
  return { cls: '', text: t('macro.noScore') };
}

function ReviewRegionCard({ region, rv, reflection, names, zones }: { region: Region; rv?: ReviewRegion; reflection?: { what_worked?: string; what_failed?: string; discussion?: string }; names: Record<string, string>; zones: { neutral: number; strong: number } }) {
  if (!rv || rv.missing) {
    return (
      <div class="card">
        <div class="card-head">{t(`macro.window.${region}`)}</div>
        <div class="card-body muted">{rv?.missing ?? t('macro.noReviewRegion')}</div>
      </div>
    );
  }
  const f = rv.facts ?? {};
  const sc = rv.score ?? {};
  const dir = String(f.dir ?? '');
  const needle = rv.after ?? rv.needle ?? 0;
  const nh = hitLabel(sc.needle_hit as string);
  const ah = rv.after != null ? hitLabel(sc.after_hit as string) : null;
  const factors = (rv.factors ?? []).filter((x) => x.hit === '1' || x.hit === '0');
  return (
    <div class="card review-region">
      <div class="card-head">
        {t(`macro.window.${region}`)} · {String(f.sym ?? '')}
        {f.partial ? <span class="muted"> · {t('macro.partial')}</span> : null}
      </div>
      <div class="card-body">
        <div class="rv-top">
          <Gauge needle={needle} before={rv.after != null ? rv.needle : null} zones={zones} mini />
          <div class="rv-arrow">
            <div>
              {t('macro.morning')} <b>{signed(rv.needle ?? 0)}</b>
              {rv.after != null ? (
                <>
                  {' '}
                  → <b>{signed(rv.after)}</b>
                </>
              ) : null}
            </div>
            <div>
              {t('macro.fact')} <b class={dir === 'LONG' ? 'pos' : dir === 'SHORT' ? 'neg' : ''}>{t(`macro.dir.${dir || 'FLAT'}`)}</b>
              {typeof f.chg_adr === 'number' ? <span class="muted"> ({f.chg_adr > 0 ? '+' : ''}{f.chg_adr.toFixed(2)} ADR)</span> : null}
            </div>
          </div>
        </div>
        <div class="rv-chips">
          <span class={`chip ${nh.cls}`}>
            {t('macro.needle')}: {nh.text}
          </span>
          {ah ? (
            <span class={`chip ${ah.cls}`}>
              {t('macro.afterTalk')}: {ah.text}
            </span>
          ) : null}
          {sc.plan_hit ? <span class={`chip ${PLAN_HIT[String(sc.plan_hit)] ?? ''}`}>{t('macro.planA', { v: String(sc.plan_hit) })}</span> : null}
          {sc.manip_swept != null ? <span class={`chip ${sc.manip_swept ? 'good' : ''}`}>{t(sc.manip_swept ? 'macro.manipYes' : 'macro.manipNo')}</span> : null}
          {f.first_break ? <span class="chip">{t('macro.firstBreak', { d: t(`macro.break.${f.first_break}`) })}</span> : null}
        </div>
        {factors.length ? (
          <div class="rv-factors">
            {factors.map((x) => (
              <span class={`fhit ${x.hit === '1' ? 'good' : 'bad'}`} title={`${x.side} · ${t('macro.share')} ${x.share}`}>
                {x.hit === '1' ? '✓' : '✗'} {names[x.factor] ?? x.factor.replace(/^talk_/, t('macro.talkPrefix'))}
              </span>
            ))}
          </div>
        ) : null}
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
                {day.names[s.factor] ?? s.factor} <small class="muted">{s.region.toUpperCase()}</small>
              </span>
              <span class="stat-track">
                <span class={`stat-fill ${r >= 0.6 ? 'good' : r < 0.45 ? 'bad' : ''}`} style={{ width: `${r * 100}%` }} />
                <span class="stat-50" />
              </span>
              <span class="fpts">
                {Math.round(r * 100)}% <small class="muted">n={s.total}</small>
              </span>
            </div>
          );
        })}
        <p class="card-note">{t('macro.statsNote')}</p>
      </div>
    </div>
  );
}
