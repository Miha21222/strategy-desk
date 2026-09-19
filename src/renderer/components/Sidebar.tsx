// Left sidebar: connection lights, quick actions and the chat list with
// shelves, pinned items and date buckets.
import { useState } from 'preact/hooks';
import type { ChatKind, ChatMeta, Settings, Status, WorkflowPrompt } from '@shared/api';
import { CHAT_KINDS } from '@shared/api';
import { GradeBadge } from '../blocks';
import { t } from '../i18n';
import { groupChats, latestReport, QUICK_PROMPTS, SHELF_KINDS, versionCount, type Bucket } from '../state';

export interface SidebarProps {
  status: Status | null;
  settings: Settings;
  chats: ChatMeta[];
  currentKey: string | null;
  launching: boolean;
  tvMessage: string | null;
  onToggleSection: (id: string) => void;
  onLaunchTv: () => void;
  onRefreshStatus: () => void;
  /** Active workflow's quick prompts; falls back to the built-in list. */
  quickPrompts: WorkflowPrompt[];
  onQuick: (prompt: string) => void;
  /** Strategy shelf hover action: send the health-check prompt into that chat. */
  onHealth: (key: string) => void;
  onNewChat: () => void;
  onOpen: (key: string) => void;
  onRename: (key: string, title: string) => void;
  onPin: (key: string, pinned: boolean) => void;
  onDelete: (key: string) => void;
  onSetKind: (key: string, kind: ChatKind) => void;
}

function Light({ on, label, hint }: { on: boolean | null; label: string; hint?: string }) {
  const cls = on === null ? 'unknown' : on ? 'on' : 'off';
  return (
    <div class="light" title={hint || ''}>
      <span class={`dot ${cls}`} />
      <span>{label}</span>
    </div>
  );
}

function Section({ id, title, collapsed, onToggle, children }: { id: string; title: string; collapsed: boolean; onToggle: (id: string) => void; children: preact.ComponentChildren }) {
  return (
    <div class={`section${collapsed ? ' collapsed' : ''}`}>
      <button type="button" class="section-head" onClick={() => onToggle(id)}>
        <span class="caret">{collapsed ? '▸' : '▾'}</span>
        {title}
      </button>
      {!collapsed ? <div class="section-body">{children}</div> : null}
    </div>
  );
}

function ChatRow(p: { chat: ChatMeta; active: boolean; onOpen: () => void; onRename: (title: string) => void; onPin: () => void; onDelete: () => void; onSetKind: (k: ChatKind) => void; onHealth: () => void }) {
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(p.chat.title);
  const [confirm, setConfirm] = useState(false);
  const [moving, setMoving] = useState(false);
  const latest = latestReport(p.chat);
  const versions = versionCount(p.chat);

  const commit = () => {
    setEditing(false);
    if (title.trim() !== p.chat.title) p.onRename(title.trim());
  };

  return (
    <div class={`chat-row${p.active ? ' active' : ''}`}>
      {editing ? (
        <input
          class="rename"
          value={title}
          autoFocus
          onInput={(e) => setTitle((e.target as HTMLInputElement).value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit();
            if (e.key === 'Escape') {
              setTitle(p.chat.title);
              setEditing(false);
            }
          }}
        />
      ) : (
        <button type="button" class="chat-open" onClick={p.onOpen} title={p.chat.title}>
          {p.chat.title || t('chat.untitled')}
        </button>
      )}
      {latest ? <GradeBadge grade={latest.grade} small /> : null}
      {p.chat.kind === 'strategy' && versions > 0 ? (
        <span class="versions" title={t('drawer.versions')}>
          {t('shelfRow.versions', { n: versions })}
        </span>
      ) : null}
      <div class="row-actions">
        {p.chat.kind === 'strategy' && !moving ? (
          <button type="button" class="health" title={t('shelfRow.health')} onClick={p.onHealth}>
            +
          </button>
        ) : null}
        {moving ? (
          <select
            class="kind-select"
            value={p.chat.kind}
            onChange={(e) => {
              p.onSetKind((e.target as HTMLSelectElement).value as ChatKind);
              setMoving(false);
            }}
            onBlur={() => setMoving(false)}
          >
            {CHAT_KINDS.map((k) => (
              <option value={k}>{t(`shelf.${k}`)}</option>
            ))}
          </select>
        ) : (
          <>
            <button type="button" title={t('chat.rename')} onClick={() => setEditing(true)}>
              ✎
            </button>
            <button type="button" title={p.chat.pinned ? t('chat.unpin') : t('chat.pin')} onClick={p.onPin}>
              {p.chat.pinned ? '★' : '☆'}
            </button>
            <button type="button" title={t('chat.move')} onClick={() => setMoving(true)}>
              ⇢
            </button>
            {confirm ? (
              <button type="button" class="confirm" onClick={p.onDelete} onBlur={() => setConfirm(false)}>
                {t('chat.confirmDelete')}
              </button>
            ) : (
              <button type="button" title={t('chat.delete')} onClick={() => setConfirm(true)}>
                ✕
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}

export function Sidebar(p: SidebarProps) {
  const [filter, setFilter] = useState('');
  const collapsed = (id: string) => Boolean(p.settings.sidebar?.[id]);
  const groups = groupChats(p.chats, filter);
  const s = p.status;

  const row = (c: ChatMeta) => (
    <ChatRow
      key={c.key}
      chat={c}
      active={c.key === p.currentKey}
      onOpen={() => p.onOpen(c.key)}
      onRename={(title) => p.onRename(c.key, title)}
      onPin={() => p.onPin(c.key, !c.pinned)}
      onDelete={() => p.onDelete(c.key)}
      onSetKind={(k) => p.onSetKind(c.key, k)}
      onHealth={() => p.onHealth(c.key)}
    />
  );

  const group = (title: string, items: ChatMeta[]) =>
    items.length ? (
      <div class="chat-group">
        <div class="group-title">{title}</div>
        {items.map(row)}
      </div>
    ) : null;

  const buckets: Bucket[] = ['today', 'yesterday', 'week', 'older'];
  const total = groups.pinned.length + SHELF_KINDS.reduce((n, k) => n + groups.shelves[k].length, 0) + buckets.reduce((n, b) => n + groups.buckets[b].length, 0);

  return (
    <aside class="sidebar">
      <Section id="connections" title={t('sidebar.connections')} collapsed={collapsed('connections')} onToggle={p.onToggleSection}>
        <Light on={s ? s.tvRunning : null} label={t('sidebar.tvRunning')} />
        <Light on={s ? s.cdpUp : null} label={`${t('sidebar.cdpPort')} ${p.settings.cdpPort}${s?.chartTab ? ' · chart' : ''}`} />
        <Light on={s ? s.tvmcpOk : null} label={`${t('sidebar.tvmcp')}${s?.tvmcpOk ? ` · ${s.tvmcpTools} ${t('sidebar.tools')}` : ''}`} hint={s?.tvmcpError} />
        <Light on={s ? s.claude !== 'none' : null} label={`${t('sidebar.claude')} · ${s ? t(`claude.${s.claude}`) : '…'}`} />
        <div class="btn-row">
          <button type="button" class="primary" disabled={p.launching} onClick={p.onLaunchTv}>
            {p.launching ? t('sidebar.launching') : t('sidebar.launchTv')}
          </button>
          <button type="button" title={t('sidebar.refresh')} onClick={p.onRefreshStatus}>
            ↻
          </button>
        </div>
        {p.tvMessage ? <div class="tv-message">{p.tvMessage}</div> : null}
      </Section>

      <Section id="quick" title={t('sidebar.quick')} collapsed={collapsed('quick')} onToggle={p.onToggleSection}>
        {(p.quickPrompts.length ? p.quickPrompts : QUICK_PROMPTS.map((q) => ({ id: q.key, label: t(`quick.${q.key}`), prompt: q.prompt }))).map((q) => (
          <button type="button" class="quick" key={q.id} onClick={() => p.onQuick(q.prompt)} title={q.prompt}>
            {q.label}
          </button>
        ))}
      </Section>

      <Section id="chats" title={t('sidebar.chats')} collapsed={collapsed('chats')} onToggle={p.onToggleSection}>
        <div class="btn-row">
          <button type="button" class="primary" onClick={p.onNewChat}>
            + {t('sidebar.newChat')}
          </button>
        </div>
        <input class="filter" placeholder={t('sidebar.filter')} value={filter} onInput={(e) => setFilter((e.target as HTMLInputElement).value)} />
        <div class="chat-list">
          {group(t('sidebar.pinned'), groups.pinned)}
          {SHELF_KINDS.map((k) => group(t(`shelf.${k}`), groups.shelves[k]))}
          {buckets.map((b) => group(t(`sidebar.${b}`), groups.buckets[b]))}
          {total === 0 ? <div class="empty small">{t('sidebar.empty')}</div> : null}
        </div>
      </Section>
    </aside>
  );
}
