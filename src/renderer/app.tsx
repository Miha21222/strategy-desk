// Application shell: sidebar + tabbed main area + report drawer. Owns all
// renderer state, the single chat:event subscription and the card actions.
import { useCallback, useEffect, useMemo, useRef, useState } from 'preact/hooks';
import type { Attachment, AgentInit, ChatEvent, ChatKind, ChatMeta, ChatMode, SecretName, Settings, SettingsPatch, Status, StoredReport, TraderProfile, WorkflowDef } from '@shared/api';
import type { StressReport } from '@shared/blocks';
import { DEFAULT_PROFILE, DEFAULT_SETTINGS } from '@shared/defaults';
import type { PreflightFix, PreflightResult } from '@shared/preflight';
import { applyTaskEvent, emptyTask, TASK_IDS, type TaskId, type TaskState } from '@shared/tasks';
import { nextZoom } from '@shared/zoom';
import { CardActionsContext, type CardActions } from './blocks';
import { blocksOfKind } from './chat/blocks';
import { ChatView } from './chat/ChatView';
import { DayPanel } from './components/DayPanel';
import { MacroDesk } from './macro/MacroDesk';
import { MacroBoundary } from './macro/Boundary';
import { macroChatTitle } from '@shared/macro';
import { HelpTab } from './components/HelpTab';
import { ReportDrawer } from './components/ReportDrawer';
import { SettingsTab } from './components/SettingsTab';
import { SetupWizard } from './components/SetupWizard';
import { Sidebar } from './components/Sidebar';
import { Toasts } from './components/Toasts';
import { WorkflowsTab } from './components/WorkflowsTab';
import { t } from './i18n';
import { healthCheckPrompt, REPORT_CATALOG } from './prompts';
import { applySkip, finishPatch, PICK_FOR_FIELD, reconcileSkips, shouldOpenWizard, stepIdsFor, type StepId } from './setup';
import type { UiMessage } from './state';
import { addToast, TOAST_MS, type Toast, type ToastLevel } from './toasts';

type Tab = 'macro' | 'copilot' | 'day' | 'workflows' | 'settings' | 'help';
/**
 * The macro-review MVP shows four tabs; the day panel and the workflow tab are
 * still here, behind `settings.legacyUi`, until they are rebuilt or removed.
 */
const MVP_TABS: Tab[] = ['macro', 'copilot', 'settings', 'help'];
const LEGACY_TABS: Tab[] = ['macro', 'copilot', 'day', 'workflows', 'settings', 'help'];
const STATUS_POLL_MS = 20_000;

function applyTheme(theme: Settings['theme']): void {
  const root = document.documentElement;
  if (theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme);
}

export function App() {
  const desk = window.desk;
  const [tab, setTab] = useState<Tab>('macro');
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [profile, setProfile] = useState<TraderProfile>(DEFAULT_PROFILE);
  const [status, setStatus] = useState<Status | null>(null);
  const [chats, setChats] = useState<ChatMeta[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowDef[]>([]);
  const [currentKey, setCurrentKey] = useState<string | null>(null);
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [thinking, setThinking] = useState(false);
  // True between "sent" and the session's init frame: that gap is the CLI
  // booting and tvmcp connecting, which is worth naming instead of showing
  // "думаю" for three seconds.
  const [connecting, setConnecting] = useState(false);
  const [init, setInit] = useState<AgentInit | null>(null);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [drawerReportId, setDrawerReportId] = useState<string | null>(null);
  const [mode, setMode] = useState<ChatMode>(() => {
    try {
      return localStorage.getItem('desk.mode') === 'saver' ? 'saver' : 'full';
    } catch {
      return 'full';
    }
  });
  const [launching, setLaunching] = useState(false);
  const [toasts, setToasts] = useState<Toast[]>([]);
  // ----- setup wizard / diagnosis -----
  const [preflight, setPreflight] = useState<PreflightResult | null>(null);
  const [preflightBusy, setPreflightBusy] = useState(false);
  const [wizardStep, setWizardStep] = useState<StepId | null>(null);
  const [skipped, setSkipped] = useState<string[]>([]);
  // The long operations the diagnosis can start, and the command each of them
  // runs. Held here rather than inside the list so the output survives moving
  // between the wizard and the settings while a 15-minute sync is in flight.
  const [tasks, setTasks] = useState<Partial<Record<TaskId, TaskState>>>({});
  const [taskCommands, setTaskCommands] = useState<Partial<Record<TaskId, string>>>({});

  // Refs so the event handler sees fresh values without resubscribing.
  const currentKeyRef = useRef<string | null>(null);
  currentKeyRef.current = currentKey;
  const profileRef = useRef(profile);
  profileRef.current = profile;
  const patchSettingsRef = useRef<(patch: SettingsPatch) => Promise<void>>(async () => {});

  const dismissToast = useCallback((id: number) => setToasts((list) => list.filter((x) => x.id !== id)), []);

  /** Show a system notice in the corner; it clears itself after TOAST_MS. */
  const pushToast = useCallback(
    (text: string, level: ToastLevel = 'info') => {
      const id = Date.now() + Math.random();
      setToasts((list) => addToast(list, { id, text, level }));
      setTimeout(() => dismissToast(id), TOAST_MS[level]);
    },
    [dismissToast],
  );

  const refreshChats = useCallback(async () => setChats(await desk.chats.list()), [desk]);

  /** Ask the main process to boot the session this chat's next turn will use. */
  const prewarm = useCallback((chatKey: string | null) => desk.chat.prewarm({ chatKey, mode }), [desk, mode]);
  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await desk.status.get());
    } catch {
      /* keep old status */
    }
  }, [desk]);

  /** Re-probe everything; a skipped step that is green again stops being skipped. */
  const runPreflight = useCallback(async (): Promise<PreflightResult | null> => {
    setPreflightBusy(true);
    try {
      const r = await desk.preflight.run();
      setPreflight(r);
      setSkipped((prev) => reconcileSkips(prev, r.checks));
      // The command a task would run depends on the settings (the vault path,
      // the provisioned python): refresh the previews with the diagnosis.
      void Promise.all(TASK_IDS.map(async (id) => [id, await desk.tasks.command(id).catch(() => '')] as const))
        .then((commands) => setTaskCommands(Object.fromEntries(commands)))
        .catch(() => undefined);
      return r;
    } catch {
      return null;
    } finally {
      setPreflightBusy(false);
    }
  }, [desk]);

  // One subscription for the whole app: a task started in the wizard keeps
  // streaming into the same state when the user closes it and opens the
  // settings, and the reducer is the one the main process uses.
  useEffect(() => {
    const off = desk.tasks.onEvent((ev) => {
      setTasks((prev) => ({ ...prev, [ev.id]: applyTaskEvent(prev[ev.id] ?? emptyTask(ev.id), ev) }));
    });
    void (async () => {
      try {
        const [states, commands] = await Promise.all([
          desk.tasks.list(),
          Promise.all(TASK_IDS.map(async (id) => [id, await desk.tasks.command(id).catch(() => '')] as const)),
        ]);
        setTasks(Object.fromEntries(states.map((s) => [s.id, s])));
        setTaskCommands(Object.fromEntries(commands));
      } catch {
        /* the list stays empty; every button still works */
      }
    })();
    return off;
  }, [desk]);

  /** Start one of the four operations, then re-probe: the fix only counts if it took. */
  const runTask = useCallback(
    async (id: TaskId) => {
      const state = await desk.tasks.run(id);
      setTasks((prev) => ({ ...prev, [id]: state }));
      await runPreflight();
      if (!state.ok) pushToast(t('preflight.task.failed', { error: state.error || '—' }), 'error');
    },
    [desk, pushToast, runPreflight],
  );

  useEffect(() => {
    void (async () => {
      const [s, p, w] = await Promise.all([desk.settings.get(), desk.profile.get(), desk.workflows.list().catch(() => [] as WorkflowDef[])]);
      setSettings(s);
      setProfile(p);
      setWorkflows(w);
      setSkipped(s.setupSkipped ?? []);
      applyTheme(s.theme);
      void desk.ui.zoom(s.uiScale ?? 1);
      await refreshChats();
      void refreshStatus();
      desk.chat.prewarm({ chatKey: null, mode });
      // A fresh install opens the wizard by itself; afterwards it lives in the settings.
      if (shouldOpenWizard(s)) {
        setWizardStep(stepIdsFor(s)[0]);
        void runPreflight();
      }
    })();
    const timer = setInterval(() => void refreshStatus(), STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, [desk, refreshChats, refreshStatus, runPreflight]);

  const current = chats.find((c) => c.key === currentKey) ?? null;
  const activeWorkflow = workflows.find((w) => w.id === settings.activeWorkflow) ?? workflows[0] ?? null;
  const catalog = useMemo(() => (activeWorkflow?.reports.length ? activeWorkflow.reports : REPORT_CATALOG), [activeWorkflow]);

  const openChat = useCallback(
    async (key: string, focus = true) => {
      const res = await desk.chats.open(key);
      if (!res) return;
      setCurrentKey(key);
      setMessages(res.messages);
      setStreaming(false);
      setThinking(false);
      setInit(null);
      setDrawerOpen(false);
      setDrawerReportId(null);
      if (focus) setTab('copilot');
      setConnecting(false);
      prewarm(key);
    },
    [desk, prewarm],
  );

  /** Capture every valid stressreport of a finished assistant message into chats.json; open the drawer on the newest. */
  const captureReports = useCallback(
    async (chatKey: string, text: string) => {
      const found = blocksOfKind(text, 'stressreport');
      if (!found.length) return;
      let list: ChatMeta[] | null = null;
      for (const b of found) {
        const d = b.data;
        list = await desk.chats.addReport(chatKey, { kind: 'stressreport', version: d.version, grade: d.grade, strategy: d.strategy, data: d });
      }
      if (!list) return;
      setChats(list);
      if (chatKey === currentKeyRef.current) {
        const chat = list.find((c) => c.key === chatKey);
        const newest = chat?.reports?.[chat.reports.length - 1];
        if (newest) {
          setDrawerReportId(newest.id);
          setDrawerOpen(true);
        }
      }
    },
    [desk],
  );

  // ----- chat events -----
  useEffect(() => {
    const unsubscribe = desk.chat.onEvent((ev: ChatEvent) => {
      if (ev.chatKey !== currentKeyRef.current) {
        if (ev.kind === 'done' || ev.kind === 'error') void refreshChats();
        return;
      }
      if (ev.kind === 'init') {
        setInit(ev.init);
        setConnecting(false);
        return;
      }
      if (ev.kind === 'done') {
        setStreaming(false);
        setThinking(false);
        setConnecting(false);
        // The session id moved on with this turn; prepare the next one now.
        prewarm(ev.chatKey);
        // Freeze the live assistant message so the next turn starts a new one.
        setMessages((ms) => {
          const last = ms[ms.length - 1];
          const live = last && last.role === 'assistant' && last.at === -1 ? last : null;
          if (live) void captureReports(ev.chatKey, live.text);
          const frozen = live ? [...ms.slice(0, -1), { ...live, at: Date.now() }] : ms;
          return ev.stopped ? [...frozen, { role: 'notice', text: t('chat.stopped'), at: Date.now(), level: 'info' }] : frozen;
        });
        void refreshChats();
        return;
      }
      if (ev.kind === 'error') {
        setConnecting(false);
        const text = ev.message === 'busy' ? t('chat.busy') : `${t('chat.error')}: ${ev.message}`;
        setMessages((ms) => [...ms, { role: 'notice', text, at: Date.now(), level: 'error' }]);
        return;
      }
      if (ev.kind === 'notice') {
        const m = ev.message.match(/^mcp:([^:]+):(.+)$/);
        const text = m ? t('notice.mcp', { name: m[1], status: m[2] }) : t(`notice.${ev.message}`);
        setMessages((ms) => [...ms, { role: 'notice', text, at: Date.now(), level: 'info' }]);
        return;
      }
      // text / thinking / tool / tool-result mutate the trailing assistant message.
      setConnecting(false);
      setThinking(ev.kind === 'thinking');
      // A live (still streaming) assistant message is marked with at === -1.
      setMessages((ms) => {
        const last = ms[ms.length - 1];
        const isLive = Boolean(last && last.role === 'assistant' && last.at === -1);
        const base = isLive && last.role === 'assistant' ? { ...last, tools: [...last.tools] } : { role: 'assistant' as const, text: '', at: -1, tools: [], thinking: false };
        const rest = isLive ? ms.slice(0, -1) : ms;
        if (ev.kind === 'text') base.text += ev.delta;
        else if (ev.kind === 'thinking') base.thinking = true;
        else if (ev.kind === 'tool') base.tools.push({ id: ev.id, name: ev.name, input: ev.input, ok: null, result: '' });
        else if (ev.kind === 'tool-result') {
          const idx = base.tools.findIndex((x) => x.id === ev.id);
          if (idx >= 0) base.tools[idx] = { ...base.tools[idx], ok: ev.ok, result: ev.text };
        }
        return [...rest, base];
      });
    });
    return unsubscribe;
  }, [captureReports, desk, refreshChats]);

  const newChat = useCallback(
    async (focus = true): Promise<string> => {
      // The MVP's free chat is a question to the vault agent (note 23); the Strategy Lab copilot stays behind legacyUi.
      const chat = await desk.chats.create(settings.legacyUi ? 'chat' : 'ask', settings.legacyUi ? settings.activeWorkflow || null : null);
      await refreshChats();
      setCurrentKey(chat.key);
      setMessages([]);
      setInit(null);
      setStreaming(false);
      setDrawerOpen(false);
      if (focus) setTab('copilot');
      return chat.key;
    },
    [desk, refreshChats, settings.activeWorkflow],
  );

  /**
   * Send one turn. With no chat open the chat is created first: the composer is
   * right there under the cursor, so typing into it must work without a trip to
   * the "new chat" button in the sidebar.
   */
  const send = useCallback(
    async (text: string, key = currentKeyRef.current, attachments: Attachment[] = []) => {
      const chatKey = key ?? (await newChat());
      currentKeyRef.current = chatKey;
      setMessages((ms) => [...ms, { role: 'user', text, at: Date.now(), ...(attachments.length ? { files: attachments.map((a) => a.name) } : {}) }]);
      setStreaming(true);
      setThinking(false);
      setConnecting(true);
      desk.chat.send({ chatKey, text, mode, ...(attachments.length ? { attachments } : {}) });
    },
    [desk, mode, newChat],
  );

  /**
   * Send one library / panel prompt, creating a chat first when none is open.
   * `focus` is false for a turn the day panel fired by itself: yanking the user
   * out of the panel mid-session because an event arrived would be rude.
   */
  const quick = useCallback(
    async (prompt: string, focus = true) => {
      if (streaming) return;
      const key = currentKeyRef.current ?? (await newChat(focus));
      currentKeyRef.current = key;
      if (focus) setTab('copilot');
      await send(prompt, key);
    },
    [newChat, send, streaming],
  );

  // ----- macro review: one chat per day runs the skill in the vault -----
  const macroChatFor = useCallback((date: string) => chats.find((c) => c.kind === 'macro' && c.title === macroChatTitle(date)) ?? null, [chats]);

  const ensureMacroChat = useCallback(
    async (date: string): Promise<string> => {
      const found = macroChatFor(date);
      if (found) return found.key;
      const chat = await desk.chats.create('macro', null);
      setChats(await desk.chats.rename(chat.key, macroChatTitle(date)));
      return chat.key;
    },
    [desk, macroChatFor],
  );

  /** Send one turn to the day's macro chat without leaving the macro tab. */
  const macroSend = useCallback(
    async (date: string, text: string) => {
      if (streaming) return;
      const key = await ensureMacroChat(date);
      if (key !== currentKeyRef.current) await openChat(key, false);
      currentKeyRef.current = key;
      await send(text, key);
    },
    [ensureMacroChat, openChat, send, streaming],
  );

  const macroOpenChat = useCallback(
    async (date: string) => {
      const key = await ensureMacroChat(date);
      if (key !== currentKeyRef.current) await openChat(key, true);
      else setTab('copilot');
    },
    [ensureMacroChat, openChat],
  );

  const pickVault = useCallback(async () => {
    const picked = await desk.dialog.pick('vault-dir');
    if (picked) await patchSettingsRef.current({ vaultPath: picked });
  }, [desk]);

  const healthCheck = useCallback(
    async (key: string) => {
      const chat = chats.find((c) => c.key === key);
      if (!chat || streaming) return;
      if (key !== currentKeyRef.current) await openChat(key);
      currentKeyRef.current = key;
      await send(healthCheckPrompt(chat.title || t('chat.untitled')), key);
    },
    [chats, openChat, send, streaming],
  );

  const stop = useCallback(() => {
    if (currentKeyRef.current) desk.chat.stop(currentKeyRef.current);
  }, [desk]);

  const changeMode = (m: ChatMode) => {
    setMode(m);
    // The mode is part of the system prompt, so the waiting session no longer fits.
    desk.chat.prewarm({ chatKey: currentKeyRef.current, mode: m });
    try {
      localStorage.setItem('desk.mode', m);
    } catch {
      /* ignore */
    }
  };

  const patchSettings = useCallback(
    async (patch: SettingsPatch) => {
      const next = await desk.settings.set(patch);
      setSettings(next);
      if (patch.theme) applyTheme(next.theme);
      if ('uiScale' in patch) void desk.ui.zoom(next.uiScale);
      prewarm(currentKeyRef.current);
      if ('tvmcpPath' in patch || 'toolsets' in patch || 'cdpPort' in patch || 'authMode' in patch) void refreshStatus();
    },
    [desk, refreshStatus],
  );

  patchSettingsRef.current = patchSettings;

  // Ctrl +/-/0 walks the same zoom steps the settings offer, so the keyboard
  // and the dropdown can never disagree about the stored value.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      const scale = settings.uiScale ?? 1;
      let next: number | null = null;
      if (e.key === '+' || e.key === '=') next = nextZoom(scale, 1);
      else if (e.key === '-' || e.key === '_') next = nextZoom(scale, -1);
      else if (e.key === '0') next = 1;
      if (next === null || next === scale) return;
      e.preventDefault();
      void patchSettings({ uiScale: next });
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [patchSettings, settings.uiScale]);

  const setSecret = useCallback(
    async (name: SecretName, value: string | null) => {
      setSettings(value === null ? await desk.settings.clearSecret(name) : await desk.settings.setSecret(name, value));
      void refreshStatus();
      // The cookie has its own preflight row («set, unverified» / «ok»).
      if (name === 'tvSession') void runPreflight();
    },
    [desk, refreshStatus, runPreflight],
  );

  const launchTv = useCallback(async () => {
    setLaunching(true);
    try {
      const r = await desk.tv.launch();
      if (r.ok) pushToast(t('tv.ok'), 'info');
      else pushToast(t(`tv.${r.error ?? 'spawn-failed'}`, { detail: r.detail ?? '' }), 'error');
    } finally {
      setLaunching(false);
      void refreshStatus();
    }
  }, [desk, pushToast, refreshStatus]);

  /**
   * Apply one preflight fix. What the app may do for the user: launch
   * TradingView, switch the port, open a file picker, flip one of its own
   * settings, and run one of the four operations it owns end to end. `copy` and
   * `explain` are handled inside the list — installing a toolchain into the
   * user's system, cloning into their folders and closing their TradingView
   * stay theirs to do.
   */
  const applyFix = useCallback(
    async (fix: PreflightFix) => {
      if (fix.kind === 'launch-tv') await launchTv();
      else if (fix.kind === 'set-port') await patchSettings({ cdpPort: fix.port });
      else if (fix.kind === 'set-flag') await patchSettings({ [fix.field]: fix.value });
      else if (fix.kind === 'run-task') {
        // runTask re-probes by itself, and it must not be raced by a second one.
        await runTask(fix.task);
        return;
      } else if (fix.kind === 'pick-file') {
        const picked = await desk.dialog.pick(PICK_FOR_FIELD[fix.field]);
        if (!picked) return;
        await patchSettings({ [fix.field]: picked });
      } else return;
      await runPreflight();
    },
    [desk, launchTv, patchSettings, runPreflight, runTask],
  );

  /**
   * Delete the provisioned Python environment. Destructive and not undoable in
   * place (the next provisioning downloads it again), so it asks first — and it
   * is the only reason this confirm exists anywhere in the app.
   */
  const clearRuntime = useCallback(async () => {
    if (!window.confirm(t('settings.clearRuntimeConfirm'))) return;
    const r = await desk.runtime.clear();
    pushToast(t('settings.clearRuntimeDone', { path: r.removed }), 'info');
    await runPreflight();
  }, [desk, pushToast, runPreflight]);

  const openWizard = useCallback(() => {
    setWizardStep(stepIdsFor(settings)[0]);
    void runPreflight();
  }, [runPreflight, settings]);

  const skipStep = useCallback((id: StepId, on: boolean) => setSkipped((prev) => applySkip(prev, id, on)), []);

  /** Finishing remembers both the date and the steps that stayed unfinished. */
  const finishWizard = useCallback(async () => {
    await patchSettings(finishPatch(skipped));
    setWizardStep(null);
    setTab('copilot');
  }, [patchSettings, skipped]);

  /** Closing without finishing still persists the skips, so nothing is silently lost. */
  const closeWizard = useCallback(async () => {
    setWizardStep(null);
    await patchSettings({ setupSkipped: skipped });
  }, [patchSettings, skipped]);

  const toggleSection = (id: string) => void patchSettings({ sidebar: { ...settings.sidebar, [id]: !settings.sidebar?.[id] } });

  const chatOp = (op: Promise<ChatMeta[]>) => op.then(setChats);
  const onDelete = async (key: string) => {
    setChats(await desk.chats.delete(key));
    if (key === currentKeyRef.current) {
      setCurrentKey(null);
      setMessages([]);
      setStreaming(false);
      setDrawerOpen(false);
    }
  };

  // ----- card actions (nextsteps chips, report drawer) -----
  const cardActions = useMemo<CardActions>(
    () => ({
      busy: streaming,
      send: (prompt) => void quick(prompt),
      saveChat: async (kind: ChatKind, name?: string) => {
        const key = currentKeyRef.current;
        if (!key) return;
        let list = await desk.chats.setKind(key, kind);
        if (name && name.trim()) list = await desk.chats.rename(key, name.trim());
        setChats(list);
      },
      appendProfileStyle: async (text: string) => {
        const p = profileRef.current;
        const style = p.style.trim() ? `${p.style.trim()}\n${text.trim()}` : text.trim();
        setProfile(await desk.profile.set({ ...p, style }));
      },
      openReport: (report: StressReport) => {
        const chat = chats.find((c) => c.key === currentKeyRef.current);
        const match = (chat?.reports ?? []).find((r) => r.strategy === report.strategy && r.version === report.version && r.grade === report.grade && r.data.headline === report.headline);
        setDrawerReportId(match?.id ?? null);
        setDrawerOpen(true);
      },
    }),
    [chats, desk, quick, streaming],
  );

  const reports: StoredReport[] = current?.reports ?? [];

  return (
    <div class="app">
      <Sidebar
        status={status}
        settings={settings}
        chats={chats}
        currentKey={currentKey}
        launching={launching}
        quickPrompts={settings.legacyUi ? (activeWorkflow?.quickPrompts ?? []) : []}
        onToggleSection={toggleSection}
        onLaunchTv={launchTv}
        onRefreshStatus={refreshStatus}
        onOpenWizard={openWizard}
        onQuick={quick}
        onHealth={(k) => void healthCheck(k)}
        onNewChat={() => void newChat()}
        onOpen={(k) => void openChat(k)}
        onRename={(k, title) => void chatOp(desk.chats.rename(k, title))}
        onPin={(k, pinned) => void chatOp(desk.chats.pin(k, pinned))}
        onDelete={(k) => void onDelete(k)}
        onSetKind={(k, kind: ChatKind) => void chatOp(desk.chats.setKind(k, kind))}
      />
      <main class="main">
        <nav class="tabs">
          {(settings.legacyUi ? LEGACY_TABS : MVP_TABS).map((id) => (
            <button type="button" key={id} class={tab === id ? 'on' : ''} onClick={() => setTab(id)}>
              {t(`tabs.${id}`)}
            </button>
          ))}
        </nav>
        <CardActionsContext.Provider value={cardActions}>
          {tab === 'macro' ? (
            <MacroBoundary>
            <MacroDesk
              settings={settings}
              chatFor={macroChatFor}
              currentKey={currentKey}
              messages={messages}
              streaming={streaming}
              onSend={(date, text) => void macroSend(date, text)}
              onOpenChat={(date) => void macroOpenChat(date)}
              onPickVault={() => void pickVault()}
            />
            </MacroBoundary>
          ) : tab === 'copilot' ? (
            <ChatView chat={current} simple={!settings.legacyUi} messages={messages} streaming={streaming} thinking={thinking} connecting={connecting} init={init} mode={mode} catalog={catalog} onModeChange={changeMode} onSend={(text, files) => void send(text, undefined, files)} onQuick={(p) => void quick(p)} onStop={stop} />
          ) : tab === 'day' ? (
            <DayPanel workflow={activeWorkflow} settings={settings} busy={streaming} onAsk={(prompt, focus) => void quick(prompt, focus)} />
          ) : tab === 'workflows' ? (
            <WorkflowsTab workflows={workflows} activeId={activeWorkflow?.id ?? ''} busy={streaming} onActivate={(id) => void patchSettings({ activeWorkflow: id })} onRun={(p) => void quick(p)} />
          ) : tab === 'settings' ? (
            <SettingsTab
              settings={settings}
              profile={profile}
              status={status}
              workflows={workflows}
              preflight={preflight}
              preflightBusy={preflightBusy}
              tasks={tasks}
              taskCommands={taskCommands}
              onTaskStop={(id) => desk.tasks.stop(id)}
              onPreflight={() => void runPreflight()}
              onFix={(fix) => void applyFix(fix)}
              onOpenWizard={openWizard}
              onClearRuntime={() => void clearRuntime()}
              onSettings={(patch) => void patchSettings(patch)}
              onSecret={(n, v) => void setSecret(n, v)}
              onPickVault={() => void pickVault().then(() => runPreflight())}
              onProfile={(p) => void desk.profile.set(p).then(setProfile)}
              onToast={pushToast}
            />
          ) : (
            <HelpTab onOpenWizard={openWizard} simple={!settings.legacyUi} />
          )}
          {tab === 'copilot' && current ? (
            <ReportDrawer open={drawerOpen} reports={reports} selectedId={drawerReportId} onSelect={setDrawerReportId} onDelete={(id) => void chatOp(desk.chats.deleteReport(current.key, id))} onClose={() => setDrawerOpen(false)} />
          ) : null}
        </CardActionsContext.Provider>
      </main>
      <Toasts items={toasts} onDismiss={dismissToast} />
      {wizardStep ? (
        <SetupWizard
          step={wizardStep}
          result={preflight}
          busy={preflightBusy}
          skipped={skipped}
          tasks={tasks}
          taskCommands={taskCommands}
          onTaskStop={(id) => desk.tasks.stop(id)}
          settings={settings}
          onSecret={(n, v) => void setSecret(n, v)}
          onPickVault={() => void pickVault().then(() => runPreflight())}
          onTask={(id) => void runTask(id)}
          onStep={setWizardStep}
          onSkip={skipStep}
          onRecheck={() => void runPreflight()}
          onFix={(fix) => void applyFix(fix)}
          onFinish={() => void finishWizard()}
          onClose={() => void closeWizard()}
        />
      ) : null}
    </div>
  );
}
