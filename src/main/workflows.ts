// Workflow loader: reads workflows/<id>/workflow.yaml (+ its promptBlock file)
// from the app directory, validates the shape and caches the result.
import { app } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { parse as parseYaml } from 'yaml';
import { z } from 'zod';
import type { WorkflowDef } from '@shared/api';
import { CHAT_KINDS } from '@shared/api';

const PromptSchema = z.object({ id: z.string().min(1), label: z.string().min(1), prompt: z.string().min(1) });

const WorkflowFileSchema = z.object({
  id: z.string().regex(/^[a-z0-9-]+$/),
  title: z.string().min(1),
  description: z.string().default(''),
  promptBlock: z.string().min(1),
  shelves: z.array(z.enum(CHAT_KINDS as unknown as [string, ...string[]])).default([]),
  quickPrompts: z.array(PromptSchema).default([]),
  reports: z.array(PromptSchema).default([]),
});

/** Directories that may contain workflows/, in priority order (dev cwd first). */
export function workflowRoots(): string[] {
  const roots = [path.join(process.cwd(), 'workflows'), path.join(app.getAppPath(), 'workflows')];
  return [...new Set(roots)];
}

async function loadOne(dir: string): Promise<WorkflowDef | null> {
  const yamlPath = path.join(dir, 'workflow.yaml');
  let raw: string;
  try {
    raw = await fs.readFile(yamlPath, 'utf8');
  } catch {
    return null;
  }
  const parsed = WorkflowFileSchema.safeParse(parseYaml(raw));
  if (!parsed.success) {
    console.warn(`[workflows] ${yamlPath}: ${parsed.error.issues.map((i) => `${i.path.join('.')}: ${i.message}`).join('; ')}`);
    return null;
  }
  const w = parsed.data;
  // The prompt block must live inside the workflow directory.
  const promptPath = path.resolve(dir, w.promptBlock);
  if (!promptPath.startsWith(path.resolve(dir))) {
    console.warn(`[workflows] ${w.id}: promptBlock escapes the workflow directory`);
    return null;
  }
  let prompt = '';
  try {
    prompt = (await fs.readFile(promptPath, 'utf8')).trim();
  } catch {
    console.warn(`[workflows] ${w.id}: promptBlock ${w.promptBlock} not found`);
  }
  return {
    id: w.id,
    title: w.title,
    description: w.description,
    prompt,
    quickPrompts: w.quickPrompts,
    reports: w.reports,
    shelves: w.shelves as WorkflowDef['shelves'],
  };
}

let cache: Promise<WorkflowDef[]> | null = null;

export function listWorkflows(): Promise<WorkflowDef[]> {
  if (!cache) cache = scan();
  return cache;
}

export function invalidateWorkflows(): void {
  cache = null;
}

async function scan(): Promise<WorkflowDef[]> {
  const seen = new Map<string, WorkflowDef>();
  for (const root of workflowRoots()) {
    let entries: string[] = [];
    try {
      entries = await fs.readdir(root);
    } catch {
      continue;
    }
    for (const name of entries.sort()) {
      const def = await loadOne(path.join(root, name));
      if (def && !seen.has(def.id)) seen.set(def.id, def);
    }
  }
  return [...seen.values()];
}

export async function getWorkflow(id: string | null | undefined): Promise<WorkflowDef | null> {
  if (!id) return null;
  return (await listWorkflows()).find((w) => w.id === id) ?? null;
}
