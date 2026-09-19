// Renderer-local types and helpers (no runtime dependency on the bridge).
import type { ChatKind, ChatMeta, StoredReport, TranscriptItem } from '@shared/api';

export type UiMessage = TranscriptItem | { role: 'notice'; text: string; at: number; level: 'error' | 'info' };

export const QUICK_PROMPTS: { key: string; prompt: string }[] = [
  {
    key: 'readChart',
    prompt:
      'Read my chart: call tv_desktop_status, take a screenshot with tv_desktop_screenshot, Read the image and describe what you see (structure, key levels, indicators).',
  },
  { key: 'health', prompt: 'Give me a one-line health status of the TradingView connection.' },
  { key: 'studies', prompt: 'List the studies on my chart and summarize their current values.' },
  { key: 'levels', prompt: 'Capture a screenshot of my current chart and describe the key levels.' },
];

/** Newest stored report of a chat (grade tag in the chat list). */
export function latestReport(chat: ChatMeta): StoredReport | null {
  const reports = chat.reports ?? [];
  return reports.length ? reports.reduce((a, b) => (b.createdAt >= a.createdAt ? b : a)) : null;
}

/** Distinct `stressreport.version` values seen in a chat = strategy variants (selection-luck cap later). */
export function versionCount(chat: ChatMeta): number {
  return new Set((chat.reports ?? []).map((r) => r.version)).size;
}

export const SHELF_KINDS: ChatKind[] = ['strategy', 'indicator', 'performance', 'journal', 'ib-day'];

export type Bucket = 'today' | 'yesterday' | 'week' | 'older';

export function bucketOf(ts: number, now = Date.now()): Bucket {
  const d = new Date(now);
  const startToday = new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  if (ts >= startToday) return 'today';
  if (ts >= startToday - 86_400_000) return 'yesterday';
  if (ts >= startToday - 6 * 86_400_000) return 'week';
  return 'older';
}

export interface ChatGroups {
  pinned: ChatMeta[];
  shelves: Record<ChatKind, ChatMeta[]>;
  buckets: Record<Bucket, ChatMeta[]>;
}

export function groupChats(chats: ChatMeta[], filter: string): ChatGroups {
  const q = filter.trim().toLowerCase();
  const visible = q ? chats.filter((c) => (c.title || '').toLowerCase().includes(q)) : chats;
  const groups: ChatGroups = {
    pinned: [],
    shelves: { chat: [], strategy: [], indicator: [], performance: [], journal: [], 'ib-day': [] },
    buckets: { today: [], yesterday: [], week: [], older: [] },
  };
  for (const c of visible) {
    if (c.pinned) groups.pinned.push(c);
    else if (c.kind !== 'chat') groups.shelves[c.kind].push(c);
    else groups.buckets[bucketOf(c.updatedAt)].push(c);
  }
  return groups;
}
