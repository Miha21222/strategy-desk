// Chat panel: message list with streaming assistant text, collapsed tool
// rows, thinking indicator, composer, Stop, token/cost meter and mode toggle.
import { useEffect, useRef, useState } from 'preact/hooks';
import type { AgentInit, ChatMeta, ChatMode, ToolCallItem } from '@shared/api';
import { Markdown } from './Markdown';
import { t } from '../i18n';
import type { UiMessage } from '../state';

export interface ChatViewProps {
  chat: ChatMeta | null;
  messages: UiMessage[];
  streaming: boolean;
  thinking: boolean;
  init: AgentInit | null;
  mode: ChatMode;
  onModeChange: (m: ChatMode) => void;
  onSend: (text: string) => void;
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

  const submit = () => {
    const text = draft.trim();
    if (!text || p.streaming) return;
    p.onSend(text);
    setDraft('');
  };

  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <section class="chat">
      <header class="chat-head">
        <div class="chat-title">{p.chat ? p.chat.title || t('chat.untitled') : t('app.title')}</div>
        {p.init ? (
          <div class="chat-status" title={p.init.mcpServers.map((s) => `${s.name}: ${s.status}`).join('\n')}>
            <span>{t('chat.model')}: {p.init.model}</span>
            <span>{t('chat.version')} {p.init.claudeCodeVersion}</span>
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
        {!p.chat ? <div class="empty">{t('chat.noChat')}</div> : null}
        {p.chat && p.messages.length === 0 ? <div class="empty">{t('chat.empty')}</div> : null}
        {p.messages.map((m, i) => (
          <Message key={i} m={m} />
        ))}
        {p.streaming ? (
          <div class="msg indicator">
            <span class="pulse" /> {p.thinking ? t('chat.thinking') : t('chat.working')}
          </div>
        ) : null}
      </div>

      <footer class="composer">
        <textarea
          ref={taRef}
          rows={3}
          placeholder={t('composer.placeholder')}
          value={draft}
          disabled={!p.chat}
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
            <button type="button" class="primary" disabled={!p.chat || !draft.trim()} onClick={submit}>
              {t('composer.send')}
            </button>
          )}
        </div>
      </footer>
    </section>
  );
}
