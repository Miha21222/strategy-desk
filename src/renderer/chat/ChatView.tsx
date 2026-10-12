// Chat panel: message list with streaming assistant text, collapsed tool
// rows, thinking indicator, composer (with the `/` report catalog), Stop,
// token/cost meter, mode toggle and the home chips on an empty chat.
import { useEffect, useRef, useState } from 'preact/hooks';
import { ATTACH_MAX_BYTES, ATTACH_MAX_FILES, type AgentInit, type Attachment, type ChatMeta, type ChatMode, type ToolCallItem, type WorkflowPrompt } from '@shared/api';
import { Markdown } from './Markdown';
import { t } from '../i18n';
import { ASK_CHIPS, filterCatalog, HOME_CHIPS, READ_CHART_VARIANTS, webStrategyPrompt } from '../prompts';
import type { UiMessage } from '../state';

export interface ChatViewProps {
  /** MVP (note 23): the chat is a question to the vault agent — no Strategy Lab chips, no tvmcp talk. */
  simple?: boolean;
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
  onSend: (text: string, attachments?: Attachment[]) => void;
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
        <div class="bubble">
          {m.text}
          {m.files?.length ? (
            <div class="files">
              {m.files.map((f) => (
                <span class="file-chip">📎 {f}</span>
              ))}
            </div>
          ) : null}
        </div>
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

/** The MVP's empty chat (note 23): questions to the agent that remembers everything. */
function AskHome({ busy, onQuick }: { busy: boolean; onQuick: (prompt: string) => void }) {
  return (
    <div class="home">
      <div class="home-title">{t('home.askTitle')}</div>
      <div class="muted small-hint">{t('home.askSubtitle')}</div>
      <div class="chips home-chips">
        {ASK_CHIPS.map((c) => (
          <button type="button" class="chip" key={c.id} disabled={busy} onClick={() => onQuick(t(c.labelKey))}>
            {t(c.labelKey)}
          </button>
        ))}
      </div>
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
  // Files waiting to go with the next message (note 23: a screenshot of the
  // chart, a PDF). Added with the clip button, by dropping onto the composer,
  // or by pasting an image (Ctrl+V after a screenshot).
  const [files, setFiles] = useState<PendingFile[]>([]);
  const [fileError, setFileError] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);
  const addFiles = async (list: ArrayLike<File> | null | undefined, pasted = false) => {
    if (!list || !list.length) return;
    const next = [...files];
    let total = next.reduce((n, f) => n + f.bytes, 0);
    for (const f of Array.from(list)) {
      if (next.length >= ATTACH_MAX_FILES) {
        setFileError(t('composer.tooMany', { n: ATTACH_MAX_FILES }));
        break;
      }
      if (total + f.size > ATTACH_MAX_BYTES) {
        setFileError(t('composer.tooBig', { mb: Math.round(ATTACH_MAX_BYTES / 1024 / 1024) }));
        break;
      }
      const data = await readBase64(f);
      const name = pasted && f.type.startsWith('image/') ? `screenshot-${stamp()}.${f.type.split('/')[1] === 'jpeg' ? 'jpg' : f.type.split('/')[1] || 'png'}` : f.name || `file-${stamp()}`;
      next.push({ name, type: f.type, data, bytes: f.size, preview: f.type.startsWith('image/') ? `data:${f.type};base64,${data}` : null });
      total += f.size;
    }
    setFiles(next);
  };
  const removeFile = (i: number) => setFiles((cur) => cur.filter((_, k) => k !== i));
  const onPaste = (e: ClipboardEvent) => {
    const items = Array.from(e.clipboardData?.items ?? []).filter((it) => it.kind === 'file');
    if (!items.length) return;
    const got = items.map((it) => it.getAsFile()).filter((f): f is File => Boolean(f));
    if (got.length) {
      e.preventDefault();
      void addFiles(got, true);
    }
  };
  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    void addFiles(e.dataTransfer?.files);
  };

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
    if ((!text && !files.length) || p.streaming) return;
    p.onSend(text, files.length ? files.map(({ name, type, data }) => ({ name, type, data })) : undefined);
    setDraft('');
    setFiles([]);
    setFileError('');
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
        {showHome ? <div class="empty">{t(p.simple ? 'chat.emptyAsk' : 'chat.empty')}</div> : null}
        {showHome ? p.simple ? <AskHome busy={p.streaming} onQuick={p.onQuick} /> : <Home busy={p.streaming} onQuick={p.onQuick} /> : null}
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
          onPaste={onPaste}
          onDrop={onDrop}
          onDragOver={(e) => e.preventDefault()}
        />
        {files.length || fileError ? (
          <div class="pending-files">
            {files.map((f, i) => (
              <span class="file-chip" title={`${f.type || 'file'} · ${Math.max(1, Math.round(f.bytes / 1024))} KB`}>
                {f.preview ? <img src={f.preview} alt="" /> : '📎'} {f.name}
                <button type="button" class="x" aria-label={t('composer.removeFile')} onClick={() => removeFile(i)}>
                  ×
                </button>
              </span>
            ))}
            {fileError ? <span class="file-error">{fileError}</span> : null}
          </div>
        ) : null}
        <div class="composer-bar">
          <input ref={fileRef} type="file" multiple hidden accept="image/*,.pdf,.csv,.txt,.md,.json,.xlsx" onChange={(e) => { void addFiles((e.target as HTMLInputElement).files); (e.target as HTMLInputElement).value = ''; }} />
          <button type="button" class="ghost attach" title={t('composer.attachHint')} disabled={p.streaming} onClick={() => fileRef.current?.click()}>
            📎 {t('composer.attach')}
          </button>
          {p.simple ? null : (
          <div class="mode" title={t('composer.modeHint')}>
            <button type="button" class={p.mode === 'saver' ? 'on' : ''} onClick={() => p.onModeChange('saver')}>
              {t('composer.modeSaver')}
            </button>
            <button type="button" class={p.mode === 'full' ? 'on' : ''} onClick={() => p.onModeChange('full')}>
              {t('composer.modeFull')}
            </button>
          </div>
          )}
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
            <button type="button" class="primary" disabled={!draft.trim() && !files.length} onClick={submit}>
              {t('composer.send')}
            </button>
          )}
        </div>
      </footer>
    </section>
  );
}

interface PendingFile extends Attachment {
  bytes: number;
  /** data: URL for an image thumbnail; null for other files. */
  preview: string | null;
}

function readBase64(f: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onerror = () => reject(r.error);
    r.onload = () => resolve(String(r.result).replace(/^data:[^,]*,/, ''));
    r.readAsDataURL(f);
  });
}

/** «1031-164512» — for a pasted screenshot that has no name. */
function stamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, '0');
  return `${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
}
