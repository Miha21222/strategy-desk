// Laying the template down: everything into an empty folder, only the
// missing files into a vault, nothing into a stranger's folder — and never an
// overwrite of a file the user already has, except the version marker.
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { copyVaultTemplate, listTree, planVaultCopy, readTemplateVersion, TEMPLATE_MARKER, type TemplateEntry } from './vault-template';

const template: TemplateEntry[] = [
  { rel: '.claude', dir: true },
  { rel: '.claude/skills', dir: true },
  { rel: '.claude/skills/macro-review', dir: true },
  { rel: '.claude/skills/macro-review/SKILL.md', dir: false },
  { rel: 'indices-kb', dir: true },
  { rel: 'indices-kb/tools', dir: true },
  { rel: 'indices-kb/tools/scale.py', dir: false },
  { rel: 'indices-kb/config', dir: true },
  { rel: 'indices-kb/config/scale.json', dir: false },
  { rel: 'indices-kb/state', dir: true },
  { rel: 'CLAUDE.md', dir: false },
  { rel: TEMPLATE_MARKER, dir: false },
];

describe('planVaultCopy', () => {
  it('copies everything into an empty folder', () => {
    const plan = planVaultCopy(template, new Set());
    expect(plan).toMatchObject({ kind: 'copy', mode: 'fresh', skipped: 0 });
    expect(plan.kind === 'copy' && plan.entries.length).toBe(template.length);
  });

  it('refuses a non-empty folder that is not a vault', () => {
    expect(planVaultCopy(template, new Set(['photos', 'photos/cat.jpg']))).toEqual({ kind: 'refuse', reason: 'not-a-vault' });
  });

  it('completes a vault with only what it lacks, and always refreshes the version marker', () => {
    const existing = new Set(['.claude', '.claude/skills', '.claude/skills/macro-review', '.claude/skills/macro-review/SKILL.md', 'indices-kb', 'indices-kb/config', 'indices-kb/config/scale.json', 'indices-kb/state', 'indices-kb/state/scale_2026-10-10_eu.json', TEMPLATE_MARKER]);
    const plan = planVaultCopy(template, existing);
    expect(plan.kind).toBe('copy');
    if (plan.kind !== 'copy') return;
    expect(plan.mode).toBe('complete');
    expect(plan.entries.map((e) => e.rel)).toEqual(['indices-kb/tools', 'indices-kb/tools/scale.py', 'CLAUDE.md', TEMPLATE_MARKER]);
    // The owner's config is NOT in the list: nothing existing is overwritten.
    expect(plan.entries.some((e) => e.rel === 'indices-kb/config/scale.json')).toBe(false);
  });
});

describe('copyVaultTemplate', () => {
  let root: string;
  beforeEach(async () => {
    root = await mkdtemp(path.join(os.tmpdir(), 'sd-vault-'));
  });
  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  async function makeTemplate(): Promise<string> {
    const t = path.join(root, 'template');
    await mkdir(path.join(t, '.claude', 'skills', 'macro-review'), { recursive: true });
    await mkdir(path.join(t, 'indices-kb', 'tools'), { recursive: true });
    await mkdir(path.join(t, 'indices-kb', 'config'), { recursive: true });
    await mkdir(path.join(t, 'indices-kb', 'state'), { recursive: true });
    await writeFile(path.join(t, '.claude', 'skills', 'macro-review', 'SKILL.md'), '# skill');
    await writeFile(path.join(t, 'indices-kb', 'tools', 'scale.py'), 'print(1)');
    await writeFile(path.join(t, 'indices-kb', 'config', 'scale.json'), '{"template":true}');
    await writeFile(path.join(t, TEMPLATE_MARKER), JSON.stringify({ version: '2026-10-12' }));
    return t;
  }

  it('creates a fresh vault, empty directories included', async () => {
    const t = await makeTemplate();
    const target = path.join(root, 'vault');
    const lines: string[] = [];
    const r = await copyVaultTemplate(t, target, (l) => lines.push(l));
    expect(r).toEqual({ mode: 'fresh', files: 4 });
    expect((await listTree(target)).some((e) => e.rel === 'indices-kb/state' && e.dir)).toBe(true);
    expect(await readTemplateVersion(target)).toBe('2026-10-12');
    expect(lines[0]).toContain('creating a vault');
  });

  it('keeps the owner`s files when completing, and refuses a stranger`s folder', async () => {
    const t = await makeTemplate();
    const target = path.join(root, 'vault');
    await mkdir(path.join(target, 'indices-kb', 'config'), { recursive: true });
    await writeFile(path.join(target, 'indices-kb', 'config', 'scale.json'), '{"mine":true}');
    const r = await copyVaultTemplate(t, target, () => undefined);
    expect(r.mode).toBe('complete');
    expect(await readFile(path.join(target, 'indices-kb', 'config', 'scale.json'), 'utf8')).toBe('{"mine":true}');
    expect(await readFile(path.join(target, 'indices-kb', 'tools', 'scale.py'), 'utf8')).toBe('print(1)');

    const other = path.join(root, 'other');
    await mkdir(other, { recursive: true });
    await writeFile(path.join(other, 'notes.txt'), 'hi');
    await expect(copyVaultTemplate(t, other, () => undefined)).rejects.toThrow(/not a vault/);
  });
});
