// JSON file store under app.getPath('userData'): config.json (settings +
// encrypted secrets), chats.json (chat metadata), profile.json (trader
// profile) and transcripts/<key>.json. All writes are atomic (tmp + rename).
import { app, safeStorage } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import type { ChatKind, ChatMeta, Settings, SettingsPatch, TraderProfile, TranscriptItem } from '@shared/api';
import { DEFAULT_PROFILE, DEFAULT_SETTINGS } from '@shared/defaults';

interface ConfigFile extends Omit<Settings, 'hasApiKey' | 'hasOauthToken'> {
  /** base64 of safeStorage.encryptString(); absent when not set. */
  apiKeyEnc?: string;
  oauthTokenEnc?: string;
}

const userData = () => app.getPath('userData');
const filePath = (name: string) => path.join(userData(), name);

async function readJson<T>(file: string, fallback: T): Promise<T> {
  try {
    const raw = await fs.readFile(file, 'utf8');
    return { ...fallback, ...(JSON.parse(raw) as object) } as T;
  } catch {
    return fallback;
  }
}

async function writeJsonAtomic(file: string, data: unknown): Promise<void> {
  await fs.mkdir(path.dirname(file), { recursive: true });
  const tmp = `${file}.${process.pid}.${Date.now()}.tmp`;
  await fs.writeFile(tmp, JSON.stringify(data, null, 2), 'utf8');
  await fs.rename(tmp, file);
}

// ---------- secrets ----------

function encrypt(value: string): string {
  if (!safeStorage.isEncryptionAvailable()) {
    throw new Error('safeStorage encryption is not available on this system');
  }
  return safeStorage.encryptString(value).toString('base64');
}

function decrypt(enc: string | undefined): string | null {
  if (!enc) return null;
  try {
    return safeStorage.decryptString(Buffer.from(enc, 'base64'));
  } catch {
    return null;
  }
}

// ---------- config ----------

const CONFIG_DEFAULTS: ConfigFile = (() => {
  const { hasApiKey: _a, hasOauthToken: _b, ...rest } = DEFAULT_SETTINGS;
  return rest;
})();

async function readConfig(): Promise<ConfigFile> {
  return readJson<ConfigFile>(filePath('config.json'), CONFIG_DEFAULTS);
}

function toSettings(cfg: ConfigFile): Settings {
  const { apiKeyEnc, oauthTokenEnc, ...rest } = cfg;
  return { ...rest, hasApiKey: Boolean(apiKeyEnc), hasOauthToken: Boolean(oauthTokenEnc) };
}

export async function getSettings(): Promise<Settings> {
  return toSettings(await readConfig());
}

export async function setSettings(patch: SettingsPatch): Promise<Settings> {
  const cfg = await readConfig();
  const next: ConfigFile = { ...cfg, ...patch };
  if (typeof next.cdpPort !== 'number' || !Number.isFinite(next.cdpPort)) next.cdpPort = CONFIG_DEFAULTS.cdpPort;
  await writeJsonAtomic(filePath('config.json'), next);
  return toSettings(next);
}

export async function setSecret(name: 'apiKey' | 'oauthToken', value: string): Promise<Settings> {
  const cfg = await readConfig();
  const field = name === 'apiKey' ? 'apiKeyEnc' : 'oauthTokenEnc';
  const next: ConfigFile = { ...cfg, [field]: encrypt(value.trim()) };
  await writeJsonAtomic(filePath('config.json'), next);
  return toSettings(next);
}

export async function clearSecret(name: 'apiKey' | 'oauthToken'): Promise<Settings> {
  const cfg = await readConfig();
  const field = name === 'apiKey' ? 'apiKeyEnc' : 'oauthTokenEnc';
  const next: ConfigFile = { ...cfg };
  delete next[field];
  await writeJsonAtomic(filePath('config.json'), next);
  return toSettings(next);
}

/** Main-process only: plaintext secrets for the agent environment. */
export async function getSecrets(): Promise<{ apiKey: string | null; oauthToken: string | null }> {
  const cfg = await readConfig();
  return { apiKey: decrypt(cfg.apiKeyEnc), oauthToken: decrypt(cfg.oauthTokenEnc) };
}

// ---------- profile ----------

export async function getProfile(): Promise<TraderProfile> {
  const p = await readJson<TraderProfile>(filePath('profile.json'), DEFAULT_PROFILE);
  if (!Array.isArray(p.morningChecklist)) p.morningChecklist = [];
  return p;
}

export async function setProfile(profile: TraderProfile): Promise<TraderProfile> {
  const clean: TraderProfile = {
    ...DEFAULT_PROFILE,
    ...profile,
    morningChecklist: (profile.morningChecklist ?? []).map((s) => String(s).trim()).filter(Boolean),
  };
  await writeJsonAtomic(filePath('profile.json'), clean);
  return clean;
}

// ---------- chats ----------

interface ChatsFile {
  chats: ChatMeta[];
}

// Serialize chats.json mutations so concurrent IPC calls cannot interleave.
let chatsLock: Promise<unknown> = Promise.resolve();
function withChatsLock<T>(fn: () => Promise<T>): Promise<T> {
  const run = chatsLock.then(fn, fn);
  chatsLock = run.catch(() => undefined);
  return run;
}

const byRecent = (a: ChatMeta, b: ChatMeta) => b.updatedAt - a.updatedAt;

async function readChats(): Promise<ChatMeta[]> {
  const f = await readJson<ChatsFile>(filePath('chats.json'), { chats: [] });
  return Array.isArray(f.chats) ? f.chats : [];
}

async function writeChats(chats: ChatMeta[]): Promise<void> {
  await writeJsonAtomic(filePath('chats.json'), { chats } satisfies ChatsFile);
}

export async function listChats(): Promise<ChatMeta[]> {
  return (await readChats()).sort(byRecent);
}

export async function newChat(kind: ChatKind = 'chat'): Promise<ChatMeta> {
  return withChatsLock(async () => {
    const now = Date.now();
    const chat: ChatMeta = {
      key: randomUUID(),
      sessionId: null,
      title: '',
      createdAt: now,
      updatedAt: now,
      pinned: false,
      kind,
      tokens: { in: 0, out: 0 },
      costUsd: 0,
    };
    const chats = await readChats();
    chats.push(chat);
    await writeChats(chats);
    return chat;
  });
}

export async function getChat(key: string): Promise<ChatMeta | null> {
  const chats = await readChats();
  return chats.find((c) => c.key === key) ?? null;
}

export async function updateChat(key: string, patch: Partial<ChatMeta>): Promise<ChatMeta[]> {
  return withChatsLock(async () => {
    const chats = await readChats();
    const idx = chats.findIndex((c) => c.key === key);
    if (idx >= 0) chats[idx] = { ...chats[idx], ...patch, key };
    await writeChats(chats);
    return chats.sort(byRecent);
  });
}

export async function deleteChat(key: string): Promise<ChatMeta[]> {
  return withChatsLock(async () => {
    const chats = (await readChats()).filter((c) => c.key !== key);
    await writeChats(chats);
    await fs.rm(transcriptPath(key), { force: true }).catch(() => undefined);
    return chats.sort(byRecent);
  });
}

// ---------- transcripts ----------

const transcriptPath = (key: string) => path.join(userData(), 'transcripts', `${key}.json`);

export async function getTranscript(key: string): Promise<TranscriptItem[]> {
  const t = await readJson<{ items: TranscriptItem[] }>(transcriptPath(key), { items: [] });
  return Array.isArray(t.items) ? t.items : [];
}

export async function appendTranscript(key: string, item: TranscriptItem): Promise<void> {
  const items = await getTranscript(key);
  items.push(item);
  await writeJsonAtomic(transcriptPath(key), { items });
}

export function chatWorkspaceDir(): string {
  return path.join(userData(), 'chat-workspace');
}
