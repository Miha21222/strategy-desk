// «День» tab: the app watches a trading day by itself.
//
// Everything on this screen except the chat turns is deterministic: the levels,
// the calendar, the risk verdict, the journal summary and every sentinel poll
// go through `desk.tv.call` -> the tvmcp CLI -> JSON. No model, no tokens, ~2.5 s
// of local CPU per call. A chat turn is sent only when an event whose type the
// workflow listed in `panel.askModelOn` arrives, or when the user asks for one.
//
// The poll loop is chained: the next tick is scheduled only after the previous
// call resolved, so one run never has two polls in flight.
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { Settings, WorkflowDef } from '@shared/api';
import {
  adrFromScan,
  buildSentinelSpec,
  emptyPanelDay,
  eventPromptVars,
  eventsOwedTurn,
  levelsArgs,
  levelsFromScan,
  mergeEvents,
  nextPollDelayMs,
  renderPrompt,
  shouldContinuePolling,
  todayISO,
  type PanelDayState,
  type PanelDef,
  type SentinelEvent,
  type SentinelState,
  type TvCallResult,
} from '@shared/panel';
import { CalendarBody, Card, EventFeed, emptySlot, JournalBody, LevelsBody, RiskBody, RunSummary, type Slot } from '../panel/cards';
import { t } from '../i18n';
import { isSkipped } from '../setup';

export interface DayPanelProps {
  workflow: WorkflowDef | null;
  settings: Settings;
  /** The chat is streaming: do not queue another turn on top of it. */
  busy: boolean;
  /**
   * Send exactly one chat turn. `focus` is false for a turn the panel fired by
   * itself, so an arriving event does not yank the user out of the panel.
   */
  onAsk: (prompt: string, focus: boolean) => void;
}

/** A poll that failed this many times in a row stops the loop and says so. */
const MAX_POLL_FAILURES = 5;
const RETRY_MS = 30_000;

const desk = () => window.desk;

function runIdFor(workflowId: string, date: string, replay: boolean): string {
  const base = `desk-${workflowId}-${date}`;
  return replay ? `${base}-replay-${Date.now().toString(36)}` : base;
}

export function DayPanel(p: DayPanelProps) {
  const panel: PanelDef | null = p.workflow?.panel ?? null;
  const workflowId = p.workflow?.id ?? '';

  const [date, setDate] = useState(() => todayISO());
  const [replayFrom, setReplayFrom] = useState('');
  const [replayTo, setReplayTo] = useState('');
  const [levels, setLevels] = useState<Slot>(emptySlot());
  const [calendar, setCalendar] = useState<Slot>(emptySlot());
  const [risk, setRisk] = useState<Slot>(emptySlot());
  const [journal, setJournal] = useState<Slot>(emptySlot());
  const [day, setDay] = useState<PanelDayState>(() => emptyPanelDay(workflowId, todayISO()));
  const [runState, setRunState] = useState<SentinelState | null>(null);
  const [runBusy, setRunBusy] = useState(false);
  const [pollNote, setPollNote] = useState('');
  // Mirrors the timer so the indicator re-renders; the timer itself is a ref
  // because the chained loop must not restart on every render.
  const [polling, setPolling] = useState(false);

  // Refs so the chained poll loop always sees fresh values without resubscribing.
  const dayRef = useRef(day);
  dayRef.current = day;
  const panelRef = useRef(panel);
  panelRef.current = panel;
  const askRef = useRef(p.onAsk);
  askRef.current = p.onAsk;
  const busyRef = useRef(p.busy);
  busyRef.current = p.busy;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef(false);
  const failures = useRef(0);
  const preopenDone = useRef('');

  const clearTimer = () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    setPolling(false);
  };

  // ----- one deterministic call -----

  const call = useCallback(async (name: string, args: Record<string, unknown>): Promise<TvCallResult> => {
    try {
      return await desk().tv.call(name, args);
    } catch (e) {
      return { ok: false, error: String((e as Error)?.message ?? e), warnings: [], ms: 0 };
    }
  }, []);

  const fill = useCallback(
    async (set: (s: Slot) => void, name: string, args: Record<string, unknown>) => {
      set({ status: 'loading', data: null, error: '', warnings: [], at: Date.now() });
      const r = await call(name, args);
      set(
        r.ok
          ? { status: 'ok', data: r.data, error: '', warnings: [...r.warnings, ...toolWarnings(r.data)], at: Date.now() }
          : { status: 'error', data: null, error: r.error, warnings: r.warnings, at: Date.now() },
      );
      return r;
    },
    [call],
  );

  // ----- the pre-session block -----

  const loadPreopen = useCallback(async (): Promise<Preopen> => {
    const out: Preopen = { levels: null, calendar: null, risk: null };
    const pn = panelRef.current;
    if (!pn) return out;
    const cfg = p.settings;
    const jobs: Promise<unknown>[] = [];
    // Three parallel spawns: each pays the same ~2.5 s interpreter floor, so
    // doing them in sequence would cost three times as much for nothing.
    const scanArgs = levelsArgs(pn, date);
    if (scanArgs) jobs.push(fill(setLevels, 'tv_scan_levels', scanArgs).then((r) => (out.levels = r)));
    if (pn.calendar) jobs.push(fill(setCalendar, 'tv_calendar_check', { ...pn.calendar, date }).then((r) => (out.calendar = r)));
    if (pn.risk) {
      // A workflow that names its own rules or journal (a second prop account,
      // say) keeps them; the settings only fill what the yaml left open.
      const riskArgs: Record<string, unknown> = { ...pn.risk, today: date };
      if (cfg.journalPath && riskArgs.journal === undefined) riskArgs.journal = cfg.journalPath;
      if (cfg.riskRulesPath && riskArgs.rules_path === undefined) riskArgs.rules_path = cfg.riskRulesPath;
      jobs.push(fill(setRisk, 'tv_risk_guard', riskArgs).then((r) => (out.risk = r)));
    }
    const journalPath = (pn.journal as { path_or_name?: unknown } | undefined)?.path_or_name ?? cfg.journalPath;
    if (pn.journal && journalPath) {
      jobs.push(fill(setJournal, 'tv_journal_load', { ...pn.journal, path_or_name: journalPath }));
    } else if (pn.journal) {
      setJournal({ status: 'error', data: null, error: t('day.noJournalPath'), warnings: [], at: Date.now() });
    }
    await Promise.all(jobs);
    return out;
  }, [date, fill, p.settings]);

  // ----- persistence -----

  const persist = useCallback(async (next: PanelDayState) => {
    setDay(next);
    dayRef.current = next;
    try {
      await desk().panel.set(next);
    } catch {
      /* a panel file that cannot be written must not take the watch down */
    }
  }, []);

  // ----- the chained poll loop -----

  const schedule = useCallback((ms: number) => {
    clearTimer();
    timer.current = setTimeout(() => void tickRef.current(), ms);
    setPolling(true);
  }, []);

  const tick = useCallback(async () => {
    const pn = panelRef.current;
    const state = dayRef.current;
    if (!pn || !state.runId || inFlight.current) return;
    inFlight.current = true;
    try {
      const r = await call('tv_sentinel_poll', { run_id: state.runId, since_seq: state.lastSeq, max_events: 100 });
      if (!r.ok) {
        failures.current += 1;
        setPollNote(t('day.pollFailed', { error: r.error, n: failures.current }));
        if (failures.current < MAX_POLL_FAILURES) schedule(RETRY_MS);
        return;
      }
      failures.current = 0;
      setPollNote(r.warnings[0] ?? '');
      const data = r.data as { events?: SentinelEvent[]; last_seq?: number; state?: SentinelState; next_poll_after_s?: number };
      const fresh = Array.isArray(data.events) ? data.events : [];
      setRunState(data.state ?? null);

      const events = mergeEvents(state.events, fresh);
      // A replay never spends a model turn: it is how the owner checks a day
      // after the fact, and they ask for a verdict explicitly if they want one.
      const worth = state.replay ? [] : eventsOwedTurn(pn, events, state.asked);
      // An event is marked asked only when its turn actually goes out. While the
      // chat is busy (the plan turn, a long answer) it stays owed and rides the
      // next poll's turn, instead of being dropped at 10:30 when it mattered.
      const send = worth.length > 0 && !busyRef.current && Boolean(pn.prompts.event);
      const next: PanelDayState = {
        ...state,
        lastSeq: typeof data.last_seq === 'number' ? data.last_seq : state.lastSeq,
        events,
        asked: send ? [...state.asked, ...worth.map((e) => e.seq)] : state.asked,
        updatedAt: Date.now(),
      };
      await persist(next);

      // One turn per poll, for the newest owed event; the prompt also carries
      // every owed event, so nothing is lost.
      if (send && pn.prompts.event) {
        const newest = worth[worth.length - 1];
        askRef.current(
          renderPrompt(pn.prompts.event, {
            ...eventPromptVars(newest, data.state ?? null, {
              date,
              workflow: workflowId,
              events_json: JSON.stringify(worth),
            }),
          }),
          false,
        );
      }

      const delay = nextPollDelayMs(data, pn.pollFloorSeconds);
      if (delay !== null) schedule(delay);
      else clearTimer();
    } finally {
      inFlight.current = false;
    }
  }, [call, date, persist, schedule, workflowId]);

  const tickRef = useRef(tick);
  tickRef.current = tick;

  // ----- attach / start / stop -----

  const attach = useCallback(
    async (runId: string) => {
      const r = await call('tv_sentinel_status', { run_id: runId });
      if (!r.ok) {
        setPollNote(r.error);
        return null;
      }
      const st = (r.data as { state?: SentinelState }).state ?? null;
      setRunState(st);
      if (shouldContinuePolling(st)) schedule(0);
      return st;
    },
    [call, schedule],
  );

  const start = useCallback(
    async (replay: { from: string; to: string } | null, freshScan: unknown = null) => {
      const pn = panelRef.current;
      if (!pn || pn.kind !== 'session-watch') return;
      setRunBusy(true);
      setPollNote('');
      try {
        // The watch wants the levels it is going to watch, so fetch them first
        // when the pre-session block has not run yet.
        let scan = freshScan ?? (levels.status === 'ok' ? levels.data : null);
        const scanArgs = levelsArgs(pn, date);
        if (!scan && scanArgs) {
          const r = await fill(setLevels, 'tv_scan_levels', scanArgs);
          scan = r.ok ? r.data : null;
        }
        const spec = buildSentinelSpec(pn, {
          day: date,
          levels: levelsFromScan(scan, pn.levelNames),
          adr: adrFromScan(scan),
        });
        const runId = runIdFor(workflowId, date, Boolean(replay));
        const args: Record<string, unknown> = { spec, run_id: runId };
        if (replay) args.replay = { from: replay.from, to: replay.to };
        const r = await call('tv_sentinel_start', args);
        if (!r.ok) {
          // A start never clobbers a live run: an "already exists" refusal is
          // exactly the signal to reattach to the run that is already going.
          if (/already exists/i.test(r.error)) {
            const next = { ...dayRef.current, runId, replay, updatedAt: Date.now() };
            await persist(next);
            await attach(runId);
            return;
          }
          setPollNote(r.error);
          return;
        }
        const st = (r.data as { state?: SentinelState }).state ?? null;
        setRunState(st);
        await persist({ ...emptyPanelDay(workflowId, date), runId, replay });
        schedule(0);
      } finally {
        setRunBusy(false);
      }
    },
    [attach, call, date, fill, levels, persist, schedule, workflowId],
  );

  // ----- the whole day in one press -----

  const [fullStep, setFullStep] = useState<'' | 'preopen' | 'plan' | 'watch' | 'done'>('');

  /**
   * «Запустить день целиком»: the pre-open block, the plan turn fed with its
   * numbers, then the sentinel. Every part stays available on its own button;
   * this only chains them. A second press on a day that already has a run just
   * reattaches — `start` treats "already exists" as the reattach signal.
   */
  const runFullDay = useCallback(async () => {
    const pn = panelRef.current;
    if (!pn || pn.kind !== 'session-watch') return;
    setFullStep('preopen');
    try {
      const pre = await loadPreopen();
      const data = (r: TvCallResult | null) => JSON.stringify(r && r.ok ? r.data : r ? { error: r.error } : null);
      if (pn.prompts.plan && busyRef.current) {
        // Say it: otherwise «день запущен» reads as if the plan went out.
        setPollNote(t('day.planSkippedBusy'));
      } else if (pn.prompts.plan) {
        setFullStep('plan');
        askRef.current(
          renderPrompt(pn.prompts.plan, {
            date,
            workflow: workflowId,
            levels_json: data(pre.levels),
            calendar_json: data(pre.calendar),
            risk_json: data(pre.risk),
          }),
          true,
        );
      }
      setFullStep('watch');
      if (!dayRef.current.runId) await start(null, pre.levels?.ok ? pre.levels.data : null);
      else await attach(dayRef.current.runId);
      setFullStep('done');
    } catch (e) {
      setPollNote(String((e as Error)?.message ?? e));
      setFullStep('');
    }
  }, [attach, date, loadPreopen, start, workflowId]);

  const stop = useCallback(async () => {
    const runId = dayRef.current.runId;
    if (!runId) return;
    clearTimer();
    setRunBusy(true);
    try {
      const r = await call('tv_sentinel_stop', { run_id: runId });
      if (r.ok) setRunState((r.data as { state?: SentinelState }).state ?? null);
      else setPollNote(r.error);
    } finally {
      setRunBusy(false);
    }
  }, [call]);

  const forget = useCallback(async () => {
    clearTimer();
    setRunState(null);
    setPollNote('');
    failures.current = 0;
    await desk().panel.clear(workflowId, date).catch(() => undefined);
    await persist(emptyPanelDay(workflowId, date));
  }, [date, persist, workflowId]);

  // ----- lifecycle: load the stored day, reattach, run the pre-session block -----

  useEffect(() => {
    clearTimer();
    failures.current = 0;
    setRunState(null);
    setPollNote('');
    setFullStep('');
    if (!panel || !workflowId) {
      setDay(emptyPanelDay(workflowId, date));
      return;
    }
    let live = true;
    void (async () => {
      const stored = await desk().panel.get(workflowId, date).catch(() => null);
      if (!live) return;
      const next = stored ?? emptyPanelDay(workflowId, date);
      setDay(next);
      dayRef.current = next;
      if (next.runId) await attach(next.runId);
      // The pre-session block is deterministic and costs no tokens, so it runs
      // by itself the first time this workflow+date is opened in this session.
      const key = `${workflowId}|${date}`;
      if (preopenDone.current !== key) {
        preopenDone.current = key;
        setLevels(emptySlot());
        setCalendar(emptySlot());
        setRisk(emptySlot());
        setJournal(emptySlot());
        await loadPreopen();
      }
    })();
    return () => {
      live = false;
      clearTimer();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workflowId, date, panel]);

  useEffect(() => () => clearTimer(), []);

  // ----- render -----

  if (!p.workflow) return <section class="page">{<div class="empty">{t('day.noWorkflow')}</div>}</section>;
  if (!panel) {
    return (
      <section class="page">
        <h2>{t('day.title')}</h2>
        <div class="empty">{t('day.noPanel', { title: p.workflow.title })}</div>
      </section>
    );
  }

  const watched = new Set(panel.levelNames);
  const asked = new Set(day.asked);
  const canAskVerdict = Boolean(panel.prompts.verdict) && !p.busy;
  const canReview = Boolean(panel.prompts.review) && !p.busy && day.events.length > 0;

  const askVerdict = () => {
    if (!panel.prompts.verdict) return;
    p.onAsk(
      renderPrompt(panel.prompts.verdict, {
        date,
        workflow: workflowId,
        state_json: JSON.stringify(runState ?? {}),
        risk_json: JSON.stringify(risk.data ?? null),
        journal_json: JSON.stringify((journal.data as { stats?: unknown })?.stats ?? null),
        calendar_json: JSON.stringify(calendar.data ?? null),
        events_json: JSON.stringify(day.events.slice(-40)),
      }),
      true,
    );
  };

  const askReview = () => {
    if (!panel.prompts.review) return;
    p.onAsk(
      renderPrompt(panel.prompts.review, {
        date,
        workflow: workflowId,
        state_json: JSON.stringify(runState ?? {}),
        events_json: JSON.stringify(day.events.slice(-80)),
      }),
      true,
    );
  };

  return (
    <section class="page day-panel">
      <h2>
        {t('day.title')} <code class="inline">{p.workflow.title}</code>
      </h2>
      <p class="muted">{t('day.intro')}</p>

      <div class="panel-toolbar">
        <label class="inline">
          <span>{t('day.date')}</span>
          <input type="date" value={date} onInput={(e) => setDate((e.target as HTMLInputElement).value || todayISO())} />
        </label>
        <button type="button" onClick={() => setDate(todayISO())}>
          {t('day.today')}
        </button>
        <button type="button" onClick={() => void loadPreopen()}>
          {t('day.recheck')}
        </button>
        <span class="spacer" />
        {panel.kind === 'session-watch' ? (
          <button
            type="button"
            class="primary"
            disabled={runBusy || (fullStep !== '' && fullStep !== 'done')}
            title={t('day.fullRunHint')}
            onClick={() => void runFullDay()}
          >
            {t('day.fullRun')}
          </button>
        ) : null}
        {fullStep ? <span class="muted small">{t(`day.fullStep.${fullStep}`)}</span> : null}
        {canAskVerdict ? (
          <button type="button" onClick={askVerdict}>
            {t('day.askVerdict')}
          </button>
        ) : null}
        {canReview ? (
          <button type="button" onClick={askReview}>
            {t('day.askReview')}
          </button>
        ) : null}
      </div>

      <div class="panel-grid">
        {panel.levels ? (
          <Card title={t('day.levels')} slot={levels} onRefresh={() => void fill(setLevels, 'tv_scan_levels', levelsArgs(panel, date) ?? {})}>
            <LevelsBody data={levels.data} watched={watched} />
          </Card>
        ) : null}
        {panel.calendar ? (
          <Card title={t('day.calendar')} slot={calendar} onRefresh={() => void fill(setCalendar, 'tv_calendar_check', { ...panel.calendar, date })}>
            <CalendarBody data={calendar.data} />
          </Card>
        ) : null}
        {panel.risk ? (
          <Card title={t('day.riskTitle')} slot={risk} onRefresh={() => void loadPreopen()}>
            <RiskBody data={risk.data} />
          </Card>
        ) : null}
        {panel.journal ? (
          <Card title={t('day.journal')} slot={journal} onRefresh={() => void loadPreopen()}>
            <JournalBody data={journal.data} />
          </Card>
        ) : null}
      </div>

      {!p.settings.riskRulesPath && panel.risk ? <div class="panel-hint">{t('day.noRulesHint')}</div> : null}
      {/* A skipped wizard step says so here, instead of the day quietly running on the shipped example. */}
      {isSkipped(p.settings, 'files') ? <div class="panel-hint">{t('setup.limited.files')}</div> : null}
      {isSkipped(p.settings, 'tvmcp') ? <div class="panel-hint">{t('setup.limited.tvmcp')}</div> : null}

      {panel.kind === 'session-watch' ? (
        <fieldset class="watch">
          <legend>{t('day.watch')}</legend>
          <RunSummary state={runState} />
          <div class="btn-row">
            <button type="button" class="primary" disabled={runBusy || Boolean(day.runId)} onClick={() => void start(null)}>
              {t('day.startWatch')}
            </button>
            <button type="button" disabled={runBusy || !day.runId} onClick={() => void stop()}>
              {t('day.stopWatch')}
            </button>
            <button type="button" disabled={runBusy || !day.runId} onClick={() => void forget()}>
              {t('day.forget')}
            </button>
            <span class="muted small">{polling ? t('day.polling') : day.runId ? t('day.idle') : t('day.notStarted')}</span>
          </div>
          <div class="btn-row replay">
            <label class="inline">
              <span>{t('day.replay')}</span>
              <input type="datetime-local" value={replayFrom} onInput={(e) => setReplayFrom((e.target as HTMLInputElement).value)} />
              <input type="datetime-local" value={replayTo} onInput={(e) => setReplayTo((e.target as HTMLInputElement).value)} />
            </label>
            <button
              type="button"
              disabled={runBusy || !replayFrom || !replayTo || Boolean(day.runId)}
              onClick={() => void start({ from: `${replayFrom}:00Z`, to: `${replayTo}:00Z` })}
            >
              {t('day.runReplay')}
            </button>
            <span class="muted small">{t('day.replayHint')}</span>
          </div>
          {pollNote ? <div class="panel-hint">{pollNote}</div> : null}
          <EventFeed events={day.events} asked={asked} />
        </fieldset>
      ) : null}
    </section>
  );
}

/** Tool-level `warnings` are data the user must see, not noise to swallow. */
function toolWarnings(data: unknown): string[] {
  const w = (data as { warnings?: unknown })?.warnings;
  return Array.isArray(w) ? w.filter((x): x is string => typeof x === 'string').slice(0, 6) : [];
}

/** What the pre-open block fetched, for a caller that needs it right away. */
interface Preopen {
  levels: TvCallResult | null;
  calendar: TvCallResult | null;
  risk: TvCallResult | null;
}
