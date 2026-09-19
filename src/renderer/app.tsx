// Application shell: sidebar + tabbed main area. Owns all renderer state and
// the single chat:event subscription.
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import type { AgentInit, ChatEvent, ChatKind, ChatMeta, ChatMode, Settings, SettingsPatch, Status, TraderProfile } from '@shared/api';
import { DEFAULT_PROFILE, DEFAULT_SETTINGS } from '@shared/defaults';
import { ChatView } from './chat/ChatView';
import { HelpTab } from './components/HelpTab';
import { SettingsTab } from './components/SettingsTab';
import { Sidebar } from './components/Sidebar';
import { t } from './i18n';
import type { UiMessage } from './state';

type Tab = 'copilot' | 'settings' | 'help';
const STATUS_POLL_MS = 20_000;

function applyTheme(theme: Settings['theme']): void {
  const root = document.documentElement;
  if (theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme);
}

export function App() {
  const desk = window.desk;
  const [tab, setTab] = useState<Tab>('copilot');
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [profile, setProfile] = useState<TraderProfile>(DEFAULT_PROFILE);
  const [status, setStatus] = useState<Status | null>(null);
  const [chats, setChats] = useState<ChatMeta[]>([]);
  const [currentKey, setCurrentKey] = useState<string | null>(null);
  const [messages, setMessages] = useState<UiMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [thinking, setThinking] = useState(false);
  const [init, setInit] = useState<AgentInit | null>(null);
  const [mode, setMode] = useState<ChatMode>(() => {
    try {
      return localStorage.getItem('desk.mode') === 'saver' ? 'saver' : 'full';
    } catch {
      return 'full';
    }
  });
  const [launching, setLaunching] = useState(false);
  const [tvMessage, setTvMessage] = useState<string | null>(null);

  // Refs so the event handler sees fresh values without resubscribing.
  const currentKeyRef = useRef<string | null>(null);
  currentKeyRef.current = currentKey;

  const refreshChats = useCallback(async () => setChats(await desk.chats.list()), [desk]);
  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await desk.status.get());
    } catch {
      /* keep old status */
    }
  }, [desk]);

  useEffect(() => {
    void (async () => {
      const [s, p] = await Promise.all([desk.settings.get(), desk.profile.get()]);
      setSettings(s);
      setProfile(p);
      applyTheme(s.theme);
      await refreshChats();
      void refreshStatus();
    })();
    const timer = setInterval(() => void refreshStatus(), STATUS_POLL_MS);
    return () => clearInterval(timer);
  }, [desk, refreshChats, refreshStatus]);

  const current = chats.find((c) => c.key === currentKey) ?? null;

  const openChat = useCallback(
    async (key: string) => {
      const res = await desk.chats.open(key);
      if (!res) return;
      setCurrentKey(key);
      setMessages(res.messages);
      setStreaming(false);
      setThinking(false);
      setInit(null);
      setTab('copilot');
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
        return;
      }
      if (ev.kind === 'done') {
        setStreaming(false);
        setThinking(false);
        // Freeze the live assistant message so the next turn starts a new one.
        setMessages((ms) => {
          const last = ms[ms.length - 1];
          const frozen = last && last.role === 'assistant' && last.at === -1 ? [...ms.slice(0, -1), { ...last, at: Date.now() }] : ms;
          return ev.stopped ? [...frozen, { role: 'notice', text: t('chat.stopped'), at: Date.now(), level: 'info' }] : frozen;
        });
        void refreshChats();
        return;
      }
      if (ev.kind === 'error') {
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
  }, [desk, refreshChats]);

  const send = useCallback(
    (text: string, key = currentKeyRef.current) => {
      if (!key) return;
      setMessages((ms) => [...ms, { role: 'user', text, at: Date.now() }]);
      setStreaming(true);
      setThinking(false);
      desk.chat.send({ chatKey: key, text, mode });
    },
    [desk, mode],
  );

  const newChat = useCallback(async (): Promise<string> => {
    const chat = await desk.chats.create('chat');
    await refreshChats();
    setCurrentKey(chat.key);
    setMessages([]);
    setInit(null);
    setStreaming(false);
    setTab('copilot');
    return chat.key;
  }, [desk, refreshChats]);

  const quick = useCallback(
    async (prompt: string) => {
      if (streaming) return;
      const key = currentKeyRef.current ?? (await newChat());
      currentKeyRef.current = key;
      send(prompt, key);
    },
    [newChat, send, streaming],
  );

  const stop = useCallback(() => {
    if (currentKeyRef.current) desk.chat.stop(currentKeyRef.current);
  }, [desk]);

  const changeMode = (m: ChatMode) => {
    setMode(m);
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
      if ('tvmcpPath' in patch || 'toolsets' in patch || 'cdpPort' in patch || 'authMode' in patch) void refreshStatus();
    },
    [desk, refreshStatus],
  );

  const setSecret = useCallback(
    async (name: 'apiKey' | 'oauthToken', value: string | null) => {
      setSettings(value === null ? await desk.settings.clearSecret(name) : await desk.settings.setSecret(name, value));
      void refreshStatus();
    },
    [desk, refreshStatus],
  );

  const launchTv = useCallback(async () => {
    setLaunching(true);
    setTvMessage(null);
    try {
      const r = await desk.tv.launch();
      setTvMessage(r.ok ? t('tv.ok') : t(`tv.${r.error ?? 'spawn-failed'}`, { detail: r.detail ?? '' }));
    } finally {
      setLaunching(false);
      void refreshStatus();
    }
  }, [desk, refreshStatus]);

  const toggleSection = (id: string) => void patchSettings({ sidebar: { ...settings.sidebar, [id]: !settings.sidebar?.[id] } });

  const chatOp = (op: Promise<ChatMeta[]>) => op.then(setChats);
  const onDelete = async (key: string) => {
    setChats(await desk.chats.delete(key));
    if (key === currentKeyRef.current) {
      setCurrentKey(null);
      setMessages([]);
      setStreaming(false);
    }
  };

  return (
    <div class="app">
      <Sidebar
        status={status}
        settings={settings}
        chats={chats}
        currentKey={currentKey}
        launching={launching}
        tvMessage={tvMessage}
        onToggleSection={toggleSection}
        onLaunchTv={launchTv}
        onRefreshStatus={refreshStatus}
        onQuick={quick}
        onNewChat={() => void newChat()}
        onOpen={(k) => void openChat(k)}
        onRename={(k, title) => void chatOp(desk.chats.rename(k, title))}
        onPin={(k, pinned) => void chatOp(desk.chats.pin(k, pinned))}
        onDelete={(k) => void onDelete(k)}
        onSetKind={(k, kind: ChatKind) => void chatOp(desk.chats.setKind(k, kind))}
      />
      <main class="main">
        <nav class="tabs">
          {(['copilot', 'settings', 'help'] as Tab[]).map((id) => (
            <button type="button" key={id} class={tab === id ? 'on' : ''} onClick={() => setTab(id)}>
              {t(`tabs.${id}`)}
            </button>
          ))}
        </nav>
        {tab === 'copilot' ? (
          <ChatView chat={current} messages={messages} streaming={streaming} thinking={thinking} init={init} mode={mode} onModeChange={changeMode} onSend={(text) => send(text)} onStop={stop} />
        ) : tab === 'settings' ? (
          <SettingsTab settings={settings} profile={profile} status={status} onSettings={(patch) => void patchSettings(patch)} onSecret={(n, v) => void setSecret(n, v)} onProfile={(p) => void desk.profile.set(p).then(setProfile)} />
        ) : (
          <HelpTab />
        )}
      </main>
    </div>
  );
}
