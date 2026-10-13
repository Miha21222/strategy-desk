// Bridge between the renderer chat and whichever engine answers it.
//
// This file is engine-neutral since stage 6: it composes the system prompt,
// builds the `TurnSpec`, runs the turn on the chat's driver (`src/main/drivers`)
// and folds the driver's events into the `ChatEvent` stream, the persisted
// transcript and the token/cost tally. Everything that knows what an SDK or a
// CLI looks like lives in the driver.
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { ATTACH_MAX_BYTES, ATTACH_MAX_FILES, type AnswerLang, type Attachment, type ChatEvent, type ChatMeta, type ChatMode, type EffortLevel, type Settings, type TokenUsage, type ToolCallItem } from '@shared/api';
import { engineOf, type EngineId } from '@shared/engine';
import { macroEnvVars, macroPython } from './macro-env';
import { buildMcpServers, type ServerMap } from './mcp';
import { claudeBin, initRuntimePaths } from './runtime';
import { allDrivers, getDriver } from './drivers';
import type { DriverEvent, EngineDriver, TurnSpec } from './drivers/types';
import { taskInFlight } from './tasks';
import { assemblePrompt, promptForChat } from './prompt';
import { readResource } from './resources';
import { getWorkflow } from './workflows';
import * as store from './store';

type Send = (ev: ChatEvent) => void;

/** Chats with a turn in flight: which engine is running it. */
const active = new Map<string, EngineId>();
/** Chats the user stopped; cleared when the turn finishes unwinding. */
const stoppedChats = new Set<string>();

const RETRY_DELAY_MS = 1200;

/** Mode suffix appended AFTER the frozen prompt so the cached prefix stays stable while the mode is unchanged. */
export function modeSuffix(mode: ChatMode): string {
  if (mode !== 'saver') return '';
  return ['', '', '## Mode: saver', 'Keep tool calls minimal: one status check, one screenshot at most, no exploratory calls.'].join('\n');
}

/**
 * Language suffix, also appended AFTER the frozen prompt. It has to be a
 * suffix rather than part of the frozen text: the app sends many prompts on
 * the user's behalf in English (quick actions, workflow prompts, report
 * catalog), so "answer in the language the user writes in" alone makes the
 * copilot reply in English to a Russian-speaking owner. Changing the setting
 * then reaches every existing chat at once, and the cached prefix stays stable
 * as long as the setting does.
 */
export function langSuffix(lang: AnswerLang): string {
  if (lang === 'auto') return '';
  const name = lang === 'ru' ? 'Russian' : 'English';
  return [
    '',
    '',
    '## Answer language',
    `Always answer in ${name}, whatever language the incoming prompt is written in: the app sends quick actions, workflow prompts and report requests in English on the user's behalf, and those are wiring, not a language choice.`,
    `Strings inside blocks (titles, labels, notes, next-step labels) are ${name} too. Tool names, tickers, code, Pine identifiers and file paths stay verbatim.`,
  ].join('\n');
}

/**
 * Effort for the turn. Saver caps it at medium instead of setting it: a user
 * who chose "low" asked for something cheaper than medium, and the saver
 * button must never make a turn more expensive than the full one.
 */
export function saverEffort(mode: ChatMode, configured: EffortLevel): EffortLevel | undefined {
  if (mode !== 'saver') return configured || undefined;
  return configured === 'low' ? 'low' : 'medium';
}

/** Frozen prompt + the two per-turn suffixes, in the one order every caller uses. */
export function composePrompt(base: string, lang: AnswerLang, mode: ChatMode): string {
  return `${base}${langSuffix(lang)}${modeSuffix(mode)}`;
}

export async function buildSystemPrompt(chat: ChatMeta, mode: ChatMode, lang: AnswerLang = 'auto'): Promise<string> {
  return composePrompt(await promptForChat(chat), lang, mode);
}

/**
 * The spec for one turn of this chat. Prewarming calls it too, with the prompt
 * a fresh chat would get, so the fingerprints of a prepared session and of the
 * real turn come out identical and the session can be adopted.
 */
export async function buildTurnSpec(chat: ChatMeta, mode: ChatMode): Promise<TurnSpec> {
  if (chat.kind === 'macro' || chat.kind === 'ask') return macroSpec(mode, chat.kind);
  const cfg = await store.getSettings();
  const systemPrompt = await buildSystemPrompt(chat, mode, cfg.answerLang ?? 'auto');
  // A chat stays on the engine that answered it first: the id it resumes from
  // belongs to that engine, and conversations do not move between them.
  return specForPrompt(systemPrompt, mode, engineOf(chat.engine ?? cfg.engine));
}

/** The same spec around an already-composed system prompt. */
export async function specForPrompt(systemPrompt: string, mode: ChatMode, engine?: EngineId): Promise<TurnSpec> {
  // If the user pressed «Развернуть рантайм» and started typing while it ran,
  // wait for it instead of spawning the MCP server against a half-built venv.
  // Provisioning is never STARTED here: that is the wizard's button, not a side
  // effect of sending a message.
  await taskInFlight('provision-runtime');
  const cfg = await store.getSettings();
  const cwd = store.chatWorkspaceDir();
  await fs.mkdir(cwd, { recursive: true });
  return {
    engine: engine ?? engineOf(cfg.engine),
    systemPrompt,
    model: cfg.model.trim(),
    effort: saverEffort(mode, cfg.effort) ?? '',
    cwd,
    mcpServers: buildMcpServers(cfg),
    permissionPreset: cfg.permissionPreset,
  };
}

/**
 * The spec of a macro-review chat: Claude Code in the owner's vault, with the
 * vault's own CLAUDE.md, skills and the owner's MCP servers — the same session
 * the Task Scheduler opens at 09:30 / 16:00 / 20:00, so a run from the desk
 * and a scheduled run produce the same files. The desk's prompt is only a
 * short appendix (`prompt/macro-review.md`); the rules live in the skill.
 */
export async function macroSpec(mode: ChatMode, kind: 'macro' | 'ask' = 'macro'): Promise<TurnSpec> {
  const cfg = await store.getSettings();
  const vault = cfg.vaultPath.trim();
  if (!vault) throw new Error('vault-not-set');
  try {
    await fs.access(vault);
  } catch {
    throw new Error(`vault-not-found: ${vault}`);
  }
  // `ask` (note 23): the same agent in the same vault, told to answer from
  // memory — reviews, lessons, notes — rather than to run a review.
  const appendix = (await readResource('prompt', kind === 'ask' ? 'ask.md' : 'macro-review.md'))?.trim() ?? '';
  // The vault contract (MACRO_VAULT, MACRO_PYTHON, MACRO_CLAUDE, TV_SESSIONID,
  // TV_CDP_URL, PYTHONIOENCODING) — the same six things the Task Scheduler's
  // run gets, assembled in one place (`macro-env.ts`). Wait for a provisioning
  // in flight for the same reason `specForPrompt` does: the venv is MACRO_PYTHON.
  await taskInFlight('provision-runtime');
  const paths = await initRuntimePaths();
  const secrets = await store.getSecrets();
  const env = macroEnvVars({ vault, python: await existingPath(macroPython(cfg, paths)), claude: await existingPath(claudeBin()), sessionId: secrets.tvSession, cdpPort: cfg.cdpPort });
  return {
    engine: 'claude',
    systemPrompt: `${appendix}${modeSuffix(mode)}`,
    model: cfg.model.trim(),
    effort: saverEffort(mode, cfg.effort) ?? '',
    cwd: vault,
    mcpServers: macroMcpServers(cfg),
    permissionPreset: cfg.permissionPreset,
    profile: 'macro',
    env,
  };
}

/**
 * The MCP map of a macro chat. `user`: none — the owner's `~/.claude` has the
 * `tradingview` server and `settingSources: user` brings it in. `bundled`: the
 * desk's own tvmcp, so a fresh install's skill finds `mcp__tradingview__*`
 * (calendar, news) without any `~/.claude` of its own. Only that server: the
 * knowledge base and the official MCP are the Strategy Lab's business.
 */
export function macroMcpServers(cfg: Settings): ServerMap {
  if ((cfg.macroMcp ?? 'bundled') !== 'bundled') return {};
  const all = buildMcpServers(cfg);
  return all.tradingview ? { tradingview: all.tradingview } : {};
}

/** A path only when it exists on disk; the env contract says «unset» otherwise. */
async function existingPath(p: string): Promise<string> {
  if (!p) return '';
  try {
    await fs.access(p);
    return p;
  } catch {
    return '';
  }
}

export interface StartArgs {
  chatKey: string;
  text: string;
  mode: ChatMode;
  attachments?: Attachment[];
}

const SAFE_NAME = /[^\p{L}\p{N}._ -]+/gu;

/**
 * Save the message's attachments where the session can Read them and return
 * the lines that tell the model where they are. A vault session (macro / ask)
 * gets `indices-kb/inbox/<date>/`, the copilot its workspace's `attachments/`.
 * Names are sanitised and prefixed with the time so two screenshots in one
 * day never collide. Nothing here is ever deleted by the app.
 */
async function saveAttachments(spec: TurnSpec, kind: ChatMeta['kind'], files: Attachment[]): Promise<{ lines: string[]; names: string[] }> {
  if (!files.length) return { lines: [], names: [] };
  if (files.length > ATTACH_MAX_FILES) throw new Error(`too-many-attachments: ${files.length} > ${ATTACH_MAX_FILES}`);
  const now = new Date();
  const day = now.toISOString().slice(0, 10);
  const hhmm = `${String(now.getHours()).padStart(2, '0')}${String(now.getMinutes()).padStart(2, '0')}${String(now.getSeconds()).padStart(2, '0')}`;
  const dir = kind === 'macro' || kind === 'ask' ? path.join(spec.cwd, 'indices-kb', 'inbox', day) : path.join(spec.cwd, 'attachments', day);
  await fs.mkdir(dir, { recursive: true });
  const lines: string[] = [];
  const names: string[] = [];
  let total = 0;
  for (const f of files) {
    const buf = Buffer.from(f.data, 'base64');
    total += buf.length;
    if (!buf.length || total > ATTACH_MAX_BYTES) throw new Error(`attachment-too-big: ${ATTACH_MAX_BYTES} bytes in all`);
    const base = path.basename(f.name).replace(SAFE_NAME, '_').slice(0, 80) || 'file';
    const full = path.join(dir, `${hhmm}_${base}`);
    await fs.writeFile(full, buf);
    const kb = Math.max(1, Math.round(buf.length / 1024));
    lines.push(`- ${full} (${f.type || 'file'}, ${kb} KB)`);
    names.push(base);
  }
  return { lines, names };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function startChat(args: StartArgs, send: Send): Promise<void> {
  const { chatKey, mode } = args;
  let text = args.text;
  if (active.has(chatKey)) {
    send({ chatKey, kind: 'error', message: 'busy' });
    return;
  }
  const chat = await store.getChat(chatKey);
  if (!chat) {
    send({ chatKey, kind: 'error', message: 'chat-not-found' });
    return;
  }

  let spec: TurnSpec;
  let driver: EngineDriver;
  try {
    spec = await buildTurnSpec(chat, mode);
    driver = getDriver(spec.engine);
  } catch (e) {
    // An engine that is configured but not built yet (or a runtime that will
    // not start) must not swallow the user's message silently.
    send({ chatKey, kind: 'error', message: String((e as Error)?.message ?? e) });
    return;
  }

  const stderrTail: string[] = [];
  const onStderr = (data: string) => {
    stderrTail.push(data);
    if (stderrTail.length > 30) stderrTail.shift();
  };

  // Attachments land on disk first; the model is told the paths, the
  // transcript keeps only the names (the user sees chips, not paths).
  let files: string[] = [];
  try {
    const saved = await saveAttachments(spec, chat.kind, args.attachments ?? []);
    files = saved.names;
    if (saved.lines.length) text = `${text.trim() || 'Посмотри вложения.'}\n\n[Вложения — открой их через Read, не перемещай]\n${saved.lines.join('\n')}`;
  } catch (e) {
    send({ chatKey, kind: 'error', message: String((e as Error)?.message ?? e) });
    return;
  }

  const now = Date.now();
  await store.appendTranscript(chatKey, { role: 'user', text: args.text, at: now, ...(files.length ? { files } : {}) });
  const patch: Parameters<typeof store.updateChat>[1] = {};
  if (!chat.title) Object.assign(patch, { title: (args.text.trim() || files.join(', ')).slice(0, 60), updatedAt: now });
  if (!chat.engine) patch.engine = spec.engine;
  if (Object.keys(patch).length) await store.updateChat(chatKey, patch);

  // Accumulated state of this turn (used for the transcript + done event).
  let assistantText = '';
  let sawThinking = false;
  const tools: ToolCallItem[] = [];
  const toolIndex = new Map<string, ToolCallItem>();
  let gotResult = false;
  let sessionId: string | null = chat.sessionId;
  let tokens: TokenUsage = { in: 0, out: 0, cacheRead: 0, cacheWrite: 0 };
  let costUsd: number | null = null;
  let turns = 0;

  const emit = (ev: DriverEvent): void => {
    if (ev.kind === 'result') {
      gotResult = true;
      sessionId = ev.sessionId ?? sessionId;
      tokens = ev.tokens;
      costUsd = ev.costUsd;
      turns = ev.turns;
      return;
    }
    if (ev.kind === 'text') assistantText += ev.delta;
    else if (ev.kind === 'thinking') sawThinking = true;
    else if (ev.kind === 'init') sessionId = ev.init.sessionId ?? sessionId;
    else if (ev.kind === 'tool') {
      const item: ToolCallItem = { id: ev.id, name: ev.name, input: ev.input, ok: null, result: '' };
      tools.push(item);
      toolIndex.set(ev.id, item);
    } else if (ev.kind === 'tool-result') {
      const item = toolIndex.get(ev.id);
      if (item) {
        item.ok = ev.ok;
        item.result = ev.text;
      }
    }
    send({ chatKey, ...ev });
  };

  const runOnce = async (): Promise<void> => {
    active.set(chatKey, spec.engine);
    try {
      const { source } = await driver.runTurn({ chatKey, spec, resume: sessionId, text, emit, onStderr });
      // Dev-only: proves what the composer's toggle actually did to this turn.
      if (process.env.NODE_ENV !== 'production') console.log(`[agent] turn: engine=${spec.engine} mode=${mode} effort=${spec.effort || 'default'} session=${source}`);
    } finally {
      active.delete(chatKey);
    }
  };

  try {
    try {
      await runOnce();
    } catch (e) {
      const stopped = stoppedChats.has(chatKey);
      if (!gotResult && !stopped && driver.isCrashError(e)) {
        driver.closeFor(chatKey);
        send({ chatKey, kind: 'notice', message: 'retry-after-crash' });
        await sleep(RETRY_DELAY_MS);
        await runOnce();
      } else if (!stopped) {
        throw e;
      }
    }
  } catch (e) {
    const msg = String((e as Error)?.message ?? e);
    const tail = stderrTail.join('').trim().split('\n').slice(-5).join('\n');
    send({ chatKey, kind: 'error', message: tail ? `${msg}\n${tail}` : msg });
  }
  const stopped = stoppedChats.delete(chatKey);

  // Persist the turn regardless of how it ended.
  const finishedAt = Date.now();
  if (assistantText || tools.length) {
    await store.appendTranscript(chatKey, { role: 'assistant', text: assistantText, at: finishedAt, tools, thinking: sawThinking });
  }
  const fresh = await store.getChat(chatKey);
  if (fresh) {
    // total_cost_usd is a running total for a resumed session (per SDK docs);
    // when it is smaller than what we have, treat it as per-call and add.
    const nextCost = costUsd == null ? fresh.costUsd : costUsd >= fresh.costUsd ? costUsd : fresh.costUsd + costUsd;
    await store.updateChat(chatKey, {
      sessionId,
      updatedAt: finishedAt,
      tokens: { in: fresh.tokens.in + tokens.in + tokens.cacheRead + tokens.cacheWrite, out: fresh.tokens.out + tokens.out },
      costUsd: nextCost,
    });
  }
  send({ chatKey, kind: 'done', tokens, costUsd, sessionId, turns, stopped });
}

export async function stopChat(chatKey: string): Promise<void> {
  const engine = active.get(chatKey);
  if (!engine) return;
  stoppedChats.add(chatKey);
  await getDriver(engine).stop(chatKey);
}

export function isChatActive(chatKey: string): boolean {
  return active.has(chatKey);
}

/** Any chat with a turn in flight — the updater refuses to install over one. */
export function anyChatActive(): boolean {
  return active.size > 0;
}

/**
 * Prepare the session the next turn will most likely need: this chat's, or -
 * with no chat open - the one a brand-new chat on the active workflow gets.
 * Best effort: a failure here only means the turn pays for its own boot.
 */
export async function prewarmFor(chatKey: string | null, mode: ChatMode): Promise<void> {
  try {
    if (chatKey) {
      if (active.has(chatKey)) return;
      const chat = await store.getChat(chatKey);
      if (!chat) return;
      const spec = await buildTurnSpec(chat, mode);
      const driver = getDriver(spec.engine);
      // The chat's own session is still up: nothing to prepare.
      if (driver.hasLive(chatKey, spec)) return;
      await driver.prewarm(spec, chat.sessionId);
      return;
    }
    const cfg = await store.getSettings();
    const [profile, workflow] = await Promise.all([store.getProfile(), getWorkflow(cfg.activeWorkflow || null)]);
    const assembled = await assemblePrompt(profile, workflow, cfg.fieldNotesPath);
    const systemPrompt = composePrompt(assembled.text, cfg.answerLang ?? 'auto', mode);
    const spec = await specForPrompt(systemPrompt, mode);
    await getDriver(spec.engine).prewarm(spec, null);
  } catch {
    /* prewarming is an optimisation, never an error path */
  }
}

/**
 * The chat is gone (or crashed): drop its session on every engine. Asking all
 * of them is right — a chat that was answered on Claude and later switched
 * would otherwise leave a live process behind.
 */
export function closeChat(chatKey: string): void {
  for (const d of allDrivers()) d.closeFor(chatKey);
}

/** Drop every session; their options no longer describe the next turn. */
export function dropWarm(): void {
  for (const d of allDrivers()) d.closeAll();
}

export async function stopAll(): Promise<void> {
  for (const d of allDrivers()) d.closeAll();
  active.clear();
  stoppedChats.clear();
}
