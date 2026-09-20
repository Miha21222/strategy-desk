// Workflow loader: reads workflows/<id>/workflow.yaml (+ its promptBlock file)
// from the app directory, validates the shape and caches the result.
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { parse as parseYaml } from 'yaml';
import type { WorkflowDef } from '@shared/api';
import { describeIssues, WorkflowFileSchema } from '@shared/workflow-schema';
import { resourceCandidates } from './resources';

/**
 * Directories that may contain workflows/, in priority order. `workflows/**` is
 * listed under `files:` in electron-builder.yml, so in a packaged build the hit
 * is inside app.asar; `./resource-paths` explains why cwd is dev-only.
 */
export function workflowRoots(): string[] {
  return resourceCandidates('workflows');
}

/**
 * Directory names that held a workflow.yaml but did not load, from the last
 * scan. A yaml typo used to be a console warning nobody reads plus a workflow
 * silently missing from the list; the preflight check names it instead.
 */
let problems: string[] = [];

export function workflowProblems(): string[] {
  return [...problems];
}

async function loadOne(dir: string): Promise<WorkflowDef | null> {
  const yamlPath = path.join(dir, 'workflow.yaml');
  const name = path.basename(dir);
  let raw: string;
  try {
    raw = await fs.readFile(yamlPath, 'utf8');
  } catch {
    return null; // not a workflow directory at all — not a problem to report
  }
  let parsed: ReturnType<typeof WorkflowFileSchema.safeParse>;
  try {
    parsed = WorkflowFileSchema.safeParse(parseYaml(raw));
  } catch (e) {
    // A yaml syntax error throws out of the parser before the schema sees it.
    console.warn(`[workflows] ${yamlPath}: ${String((e as Error)?.message ?? e)}`);
    problems.push(name);
    return null;
  }
  if (!parsed.success) {
    console.warn(`[workflows] ${yamlPath}: ${describeIssues(parsed.error)}`);
    problems.push(name);
    return null;
  }
  const w = parsed.data;
  // The prompt block must live inside the workflow directory.
  const promptPath = path.resolve(dir, w.promptBlock);
  if (!promptPath.startsWith(path.resolve(dir))) {
    console.warn(`[workflows] ${w.id}: promptBlock escapes the workflow directory`);
    problems.push(name);
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
    panel: w.panel ?? null,
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
  problems = [];
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
  // A directory that failed under one root but loaded under another is fine.
  problems = [...new Set(problems.filter((name) => !seen.has(name)))];
  return [...seen.values()];
}

export async function getWorkflow(id: string | null | undefined): Promise<WorkflowDef | null> {
  if (!id) return null;
  return (await listWorkflows()).find((w) => w.id === id) ?? null;
}
