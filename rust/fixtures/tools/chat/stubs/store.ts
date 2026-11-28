// In-memory stand-in for src/main/store.ts: only what agent.ts' startChat and
// macroSpec call. The chats and transcripts it holds are what the capture
// records as «persisted» — the shape the old app wrote to userData.
import type { ChatMeta, Settings, TranscriptItem } from '@shared/api';
import { DEFAULT_SETTINGS } from '@shared/defaults';

export const chats = new Map<string, ChatMeta>();
export const transcripts = new Map<string, TranscriptItem[]>();
export let settings: Settings = { ...DEFAULT_SETTINGS, vaultPath: process.env.CHAT_FOLD_VAULT || '/tmp' } as Settings;

export function reset(): void {
  chats.clear();
  transcripts.clear();
}
export async function getSettings(): Promise<Settings> {
  return settings;
}
export async function getSecrets(): Promise<{ apiKey: string | null; oauthToken: string | null; tvSession: string | null }> {
  return { apiKey: null, oauthToken: null, tvSession: null };
}
export function chatWorkspaceDir(): string {
  return '/tmp/chat-workspace';
}
export async function getChat(key: string): Promise<ChatMeta | null> {
  const c = chats.get(key);
  return c ? { ...c } : null;
}
export async function updateChat(key: string, patch: Partial<ChatMeta>): Promise<ChatMeta[]> {
  const c = chats.get(key);
  if (c) chats.set(key, { ...c, ...patch });
  return [...chats.values()];
}
export async function appendTranscript(key: string, item: TranscriptItem): Promise<void> {
  transcripts.set(key, [...(transcripts.get(key) ?? []), item]);
}
