// System prompt assembly and per-chat freezing.
//
// assembled = base prompt (prompt/system-prompt.md)
//           + "## Hub" (app version)
//           + "## Trader profile" (profile.json)
//           + "## Live field notes" (data/copilot-notes.md, optional)
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
import * as store from './store';
import { getWorkflow } from './workflows';

const FALLBACK_PROMPT =
  'You are a trading research copilot inside Strategy Desk, connected to TradingView Desktop through the `tradingview` MCP server. Call `tv_desktop_status` first when the user asks about their chart. Be concise. Educational only. Answer in the user\'s language.';

/** Candidate locations for a repo-relative resource (dev cwd first, then the packaged app dir). */
export function resourceCandidates(...rel: string[]): string[] {
  return [...new Set([path.join(process.cwd(), ...rel), path.join(app.getAppPath(), ...rel)])];
}

async function readFirst(candidates: string[]): Promise<string | null> {
  for (const p of candidates) {
    try {
      return (await fs.readFile(p, 'utf8')).trim();
    } catch {
      /* try next */
    }
  }
  return null;
}

export async function loadBasePrompt(): Promise<string> {
  return (await readFirst(resourceCandidates('prompt', 'system-prompt.md'))) ?? FALLBACK_PROMPT;
}

export async function loadFieldNotes(): Promise<string | null> {
  return readFirst(resourceCandidates('data', 'copilot-notes.md'));
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

function notesBlock(notes: string | null): string {
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

export async function assemblePrompt(profile: TraderProfile, workflow: WorkflowDef | null): Promise<AssembledPrompt> {
  const [base, notes] = await Promise.all([loadBasePrompt(), loadFieldNotes()]);
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
  const [profile, workflow] = await Promise.all([store.getProfile(), getWorkflow(workflowId)]);
  const p = await assemblePrompt(profile, workflow);
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
