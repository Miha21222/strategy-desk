// Chat panel: message list with streaming assistant text, collapsed tool
// rows, thinking indicator, composer (with the `/` report catalog), Stop,
// token/cost meter, mode toggle and the home chips on an empty chat.
import { useEffect, useRef, useState } from 'preact/hooks';
import type { AgentInit, ChatMeta, ChatMode, ToolCallItem, WorkflowPrompt } from '@shared/api';
import { Markdown } from './Markdown';
import { t } from '../i18n';
import { filterCatalog, HOME_CHIPS, READ_CHART_VARIANTS, webStrategyPrompt } from '../prompts';
import type { UiMessage } from '../state';

export interface ChatViewProps {
  chat: ChatMeta | null;
  messages: UiMessage[];
  streaming: boolean;
  thinking: boolean;
  /** Sent, but the session has not reported in yet: CLI boot + MCP connect. */
  connecting: boolean;
  init: AgentInit | null;
  mode: ChatMode;
  catalog: WorkflowPrompt[];
  onModeChange: (m: ChatMode) => void;
  onSend: (text: string) => void;
  /** Send a library prompt; creates a chat first when none is open. */
  onQuick: (prompt: string) => void;
  onStop: () => void;
}

function ToolRow({ tool }: { tool: ToolCallItem }) {
  const [open, setOpen] = useState(false);
  const state = tool.ok === null ? 'running' : tool.ok ? 'ok' : 'err';
  const label = tool.ok === null ? t('chat.toolRunning') : tool.ok ? t('chat.toolOk') : t('chat.toolErr');
  return (
    <div class={`tool ${state}`}>
      <button type="button" class="tool-head" onClick={() => setOpen(!open)}>
        <span class="tool-dot" />
        <span class="tool-name">{tool.name}</span>
        <span class="tool-state">{label}</span>
        <span class="tool-caret">{open ? '▾' : '▸'}</span>
      </button>
      {open ? (
        <div class="tool-body">
          <div class="tool-label">input</div>
          <pre>{tool.input}</pre>
          {tool.result ? (
            <>
              <div class="tool-label">result</div>
              <pre>{tool.result}</pre>
            </>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function Message({ m }: { m: UiMessage }) {
  if (m.role === 'user') {
    return (
      <div class="msg user">
        <div class="bubble">{m.text}</div>
      </div>
    );
  }
  if (m.role === 'notice') {
    return <div class={`msg notice ${m.level}`}>{m.text}</div>;
  }
  return (
    <div class="msg assistant">
      {m.tools.length ? <div class="tools">{m.tools.map((tool) => <ToolRow key={tool.id} tool={tool} />)}</div> : null}
      {m.text ? <Markdown text={m.text} /> : null}
    </div>
  );
}

/** Home chips + the read-chart chooser, shown on an empty chat. */
function Home({ busy, onQuick }: { busy: boolean; onQuick: (prompt: string) => void }) {
  const [chooser, setChooser] = useState(false);
  return (
    <div class="home">
      <div class="home-title">{t('home.title')}</div>
      <div class="muted small-hint">{t('home.subtitle')}</div>
      <div class="chips home-chips">
        {HOME_CHIPS.map((c) => (
          <button
            type="button"
            class={`chip${c.chooser ? ' action' : ''}`}
            key={c.id}
            disabled={busy}
            title={c.prompt ?? ''}
            onClick={() => {
              if (c.chooser) setChooser(true);
              else if (c.random) onQuick(webStrategyPrompt());
              else if (c.prompt) onQuick(c.prompt);
            }}
          >
            {t(c.labelKey)}
          </button>
        ))}
      </div>
      {chooser ? (
        <div class="chooser">
          <div class="chooser-title">{t('home.chooserTitle')}</div>
          {READ_CHART_VARIANTS.map((v) => (
            <button
              type="button"
              class="chooser-item"
              key={v.id}
              title={v.prompt}
              onClick={() => {
                setChooser(false);
                onQuick(v.prompt);
              }}
            >
              {t(v.labelKey)}
            </button>
          ))}
          <button type="button" class="small" onClick={() => setChooser(false)}>
            {t('home.cancel')}
          </button>
        </div>
      ) : null}
      <div class="muted small-hint">{t('home.catalogHint')}</div>
    </div>
  );
}

function fmtTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export function ChatView(p: ChatViewProps) {
  const [draft, setDraft] = useState('');
  const listRef = useRef<HTMLDivElement>(null);
  const taRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = listRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [p.messages, p.streaming]);

  useEffect(() => {
    setDraft('');
    taRef.current?.focus();
  }, [p.chat?.key]);

  // `/query` in the composer opens the report catalog.
  const slash = draft.startsWith('/') && !draft.includes('\n');
  const catalog = slash ? filterCatalog(p.catalog, draft.slice(1)) : [];

  const pick = (prompt: string) => {
    setDraft(prompt);
    taRef.current?.focus();
  };

  // No chat open is fine: onSend creates one and sends into it.
  const submit = () => {
    const text = draft.trim();
    if (!text || p.streaming) return;
    p.onSend(text);
    setDraft('');
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && slash) {
      setDraft('');
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      if (slash) {
        if (catalog.length) pick(catalog[0].prompt);
        return;
      }
      submit();
    }
  };

  const showHome = !p.chat || p.messages.length === 0;

  return (
    <section class="chat">
      <header class="chat-head">
        <div class="chat-title">{p.chat ? p.chat.title || t('chat.untitled') : t('app.title')}</div>
        {p.init ? (
          <div class="chat-status" title={p.init.mcpServers.map((s) => `${s.name}: ${s.status}`).join('\n')}>
            <span>{t('chat.model')}: {p.init.model}</span>
            <span>{t('chat.version')} {p.init.engineVersion}</span>
            <span>
              {p.init.toolCount} {t('chat.toolsCount')}
            </span>
            <span>
              {t('chat.mcp')}:{' '}
              {p.init.mcpServers.length ? p.init.mcpServers.map((s) => `${s.name} (${s.status})`).join(', ') : '—'}
            </span>
          </div>
        ) : null}
      </header>

      <div class="messages" ref={listRef}>
        {showHome ? <div class="empty">{t('chat.empty')}</div> : null}
        {showHome ? <Home busy={p.streaming} onQuick={p.onQuick} /> : null}
        {p.messages.map((m, i) => (
          <Message key={i} m={m} />
        ))}
        {p.streaming ? (
          <div class="msg indicator">
            <span class="pulse" /> {p.connecting ? t('chat.connecting') : p.thinking ? t('chat.thinking') : t('chat.working')}
          </div>
        ) : null}
      </div>

      <footer class="composer">
        {slash ? (
          <div class="catalog">
            <div class="catalog-title">{t('catalog.title')}</div>
            {catalog.length ? (
              catalog.map((c) => (
                <button type="button" class="catalog-item" key={c.id} title={c.prompt} onClick={() => pick(c.prompt)}>
                  <span>{c.label}</span>
                  <span class="muted">{t('catalog.insert')} ↵</span>
                </button>
              ))
            ) : (
              <div class="muted">{t('catalog.empty')}</div>
            )}
          </div>
        ) : null}
        <textarea
          ref={taRef}
          rows={3}
          placeholder={t('composer.placeholder')}
          value={draft}
          onInput={(e) => setDraft((e.target as HTMLTextAreaElement).value)}
          onKeyDown={onKey}
        />
        <div class="composer-bar">
          <div class="mode" title={t('composer.modeHint')}>
            <button type="button" class={p.mode === 'saver' ? 'on' : ''} onClick={() => p.onModeChange('saver')}>
              {t('composer.modeSaver')}
            </button>
            <button type="button" class={p.mode === 'full' ? 'on' : ''} onClick={() => p.onModeChange('full')}>
              {t('composer.modeFull')}
            </button>
          </div>
          {p.chat ? (
            <div class="meter">
              {t('composer.tokens')}: {fmtTokens(p.chat.tokens.in)} / {fmtTokens(p.chat.tokens.out)} · {t('composer.cost')}: ${p.chat.costUsd.toFixed(3)}
            </div>
          ) : null}
          <div class="spacer" />
          {p.streaming ? (
            <button type="button" class="danger" onClick={p.onStop}>
              {t('composer.stop')}
            </button>
          ) : (
            <button type="button" class="primary" disabled={!draft.trim()} onClick={submit}>
              {t('composer.send')}
            </button>
          )}
        </div>
      </footer>
    </section>
  );
}
