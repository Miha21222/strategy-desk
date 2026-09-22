// System prompt assembly and per-chat freezing.
//
// assembled = base prompt (prompt/system-prompt.md)
//           + "## Hub" (app version)
//           + "## Trader profile" (profile.json)
//           + "## Live field notes" (settings.fieldNotesPath, else data/copilot-notes.md — optional)
//           + workflow prompt block (workflows/<id>/prompt.md, optional)
//
// The text is assembled once when a chat is created, written to
// userData/prompts/<hash>.md and referenced from chats.json by hash, so a
// resumed chat always sends the byte-identical system prompt (stable prompt
// cache prefix) even after the profile or the notes change.
import { app } from 'electron';
import { createHash } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { ChatMeta, TraderProfile, WorkflowDef } from '@shared/api';
import { readResource, resourceCandidates } from './resources';
import * as store from './store';
import { getWorkflow } from './workflows';

const FALLBACK_PROMPT =
  'You are a trading research copilot inside Strategy Desk, connected to TradingView Desktop through the `tradingview` MCP server. Call `tv_desktop_status` first when the user asks about their chart. Conclusion first, then one reason; no praise for P&L and no motivational talk. Say plainly when the evidence disagrees with the user, and "I do not know" rather than guess. You never decide or place a trade. Be concise. Educational only. Answer in the user\'s language.';

/**
 * Candidate locations for a repo-relative resource: dev cwd, then the packaged
 * app dir (app.asar, where `files:` entries land), then process.resourcesPath.
 * The rule lives in `./resources` for every shipped file at once; see
 * `./resource-paths` for why each candidate exists.
 */
export { resourceCandidates };

export async function loadBasePrompt(): Promise<string> {
  return (await readResource('prompt', 'system-prompt.md'))?.trim() || FALLBACK_PROMPT;
}

/**
 * The "Live field notes" section: the landmines this project has already paid
 * for, handed to every chat as facts rather than guesses.
 *
 * With `fieldNotesPath` set the override wins outright — if the owner points at
 * a file that is not there, the answer is "no notes", not "quietly the shipped
 * ones". A missing file is never an error: the prompt simply has no such
 * section.
 */
export async function loadFieldNotes(overridePath?: string): Promise<string | null> {
  const override = (overridePath ?? '').trim();
  if (override) {
    try {
      return (await fs.readFile(override, 'utf8')).trim() || null;
    } catch {
      return null;
    }
  }
  return (await readResource('data', 'copilot-notes.md'))?.trim() ?? null;
}

export function profileBlock(p: TraderProfile): string {
  const lines: string[] = [];
  if (p.callName) lines.push(`- Call the user: ${p.callName}`);
  if (p.traderType) lines.push(`- Trader type: ${p.traderType}`);
  if (p.markets) lines.push(`- Markets: ${p.markets}`);
  if (p.style) lines.push(`- Style: ${p.style}`);
  if (p.riskRules) lines.push(`- Risk rules: ${p.riskRules}`);
  if (p.propFirms) lines.push(`- Prop firms: ${p.propFirms}`);
  if (p.morningChecklist.length) lines.push(`- Morning checklist:\n${p.morningChecklist.map((s) => `  - ${s}`).join('\n')}`);
  return lines.length ? `## Trader profile\n${lines.join('\n')}` : '';
}

export function notesBlock(notes: string | null): string {
  if (!notes) return '';
  // The notes file carries its own "# Live field notes" heading; normalise to h2.
  const body = notes.replace(/^#\s+Live field notes\s*\n/i, '').trim();
  return `## Live field notes\n${body}`;
}

function workflowBlock(w: WorkflowDef | null): string {
  if (!w || !w.prompt) return '';
  return `## Workflow: ${w.id}\n${w.prompt}`;
}

export interface AssembledPrompt {
  text: string;
  hash: string;
}

export function hashPrompt(text: string): string {
  return createHash('sha256').update(text, 'utf8').digest('hex').slice(0, 16);
}

export async function assemblePrompt(profile: TraderProfile, workflow: WorkflowDef | null, fieldNotesPath?: string): Promise<AssembledPrompt> {
  const [base, notes] = await Promise.all([loadBasePrompt(), loadFieldNotes(fieldNotesPath)]);
  const parts = [base, `## Hub\nStrategy Desk app version ${app.getVersion()}.`, profileBlock(profile), notesBlock(notes), workflowBlock(workflow)].filter(Boolean);
  const text = parts.join('\n\n');
  return { text, hash: hashPrompt(text) };
}

// ---------- frozen prompts on disk ----------

const promptsDir = () => path.join(app.getPath('userData'), 'prompts');
const promptFile = (hash: string) => path.join(promptsDir(), `${hash}.md`);

export async function saveFrozen(p: AssembledPrompt): Promise<void> {
  await fs.mkdir(promptsDir(), { recursive: true });
  const file = promptFile(p.hash);
  try {
    await fs.access(file);
  } catch {
    const tmp = `${file}.${process.pid}.tmp`;
    await fs.writeFile(tmp, p.text, 'utf8');
    await fs.rename(tmp, file);
  }
}

export async function loadFrozen(hash: string): Promise<string | null> {
  try {
    return await fs.readFile(promptFile(hash), 'utf8');
  } catch {
    return null;
  }
}

/** Assemble + freeze the prompt for a freshly created chat; returns the updated meta. */
export async function freezeForChat(chat: ChatMeta, workflowId: string | null): Promise<ChatMeta> {
  const [profile, workflow, cfg] = await Promise.all([store.getProfile(), getWorkflow(workflowId), store.getSettings()]);
  // The freeze happens once per chat, so later edits to the notes reach new
  // chats only — the cache prefix of an existing chat stays byte-identical.
  const p = await assemblePrompt(profile, workflow, cfg.fieldNotesPath);
  await saveFrozen(p);
  const chats = await store.updateChat(chat.key, { workflowId: workflow?.id ?? null, promptHash: p.hash });
  return chats.find((c) => c.key === chat.key) ?? { ...chat, workflowId: workflow?.id ?? null, promptHash: p.hash };
}

/**
 * The system prompt for a turn: the frozen text when present, otherwise
 * assembled now and frozen (migration for chats created before stage 2).
 */
export async function promptForChat(chat: ChatMeta): Promise<string> {
  if (chat.promptHash) {
    const frozen = await loadFrozen(chat.promptHash);
    if (frozen) return frozen;
  }
  const meta = await freezeForChat(chat, chat.workflowId ?? null);
  return (await loadFrozen(meta.promptHash ?? '')) ?? (await assemblePrompt(await store.getProfile(), null)).text;
}
