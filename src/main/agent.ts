// Bridge between the renderer chat and whichever engine answers it.
//
// This file is engine-neutral since stage 6: it composes the system prompt,
// builds the `TurnSpec`, runs the turn on the chat's driver (`src/main/drivers`)
// and folds the driver's events into the `ChatEvent` stream, the persisted
// transcript and the token/cost tally. Everything that knows what an SDK or a
// CLI looks like lives in the driver.
import { promises as fs } from 'node:fs';
import type { AnswerLang, ChatEvent, ChatMeta, ChatMode, EffortLevel, TokenUsage, ToolCallItem } from '@shared/api';
import { engineOf, type EngineId } from '@shared/engine';
import { buildMcpServers } from './mcp';
import { allDrivers, getDriver } from './drivers';
import type { DriverEvent, EngineDriver, TurnSpec } from './drivers/types';
import { taskInFlight } from './tasks';
import { assemblePrompt, promptForChat } from './prompt';
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

export interface StartArgs {
  chatKey: string;
  text: string;
  mode: ChatMode;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function startChat(args: StartArgs, send: Send): Promise<void> {
  const { chatKey, text, mode } = args;
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

  const now = Date.now();
  await store.appendTranscript(chatKey, { role: 'user', text, at: now });
  const patch: Parameters<typeof store.updateChat>[1] = {};
  if (!chat.title) Object.assign(patch, { title: text.trim().slice(0, 60), updatedAt: now });
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
