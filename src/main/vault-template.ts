// Laying the vault template down in the user's folder — the `vault-create` task.
//
// The template (`resources/vault-template`, staged by `scripts/stage-vault.mjs`
// and shipped as `extraResources` → `<resources>/vault-template`) holds the
// skill, the tools, the configs and the empty state/journal/shots/inbox/cache
// directories a fresh vault needs. The copy has three rules, and the pure
// `planVaultCopy` is where they are decided so vitest can pin them down:
//
//   * an empty (or absent) target gets everything;
//   * a target that already IS a vault — even a partial one — gets only the
//     files it lacks. NOTHING existing is overwritten: the owner's state,
//     journal and configs are his, and a newer template must not reset them.
//     The one exception is `template.json`, the version marker, which says
//     which template the folder was last completed from;
//   * a non-empty folder that is not a vault is refused outright. The app
//     does not write into a stranger's files.
import { promises as fs, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

export const TEMPLATE_MARKER = 'template.json';

/** A relative path inside the template, forward slashes, plus whether it is a directory. */
export interface TemplateEntry {
  rel: string;
  dir: boolean;
}

export type VaultCopyPlan =
  | { kind: 'refuse'; reason: 'not-a-vault' }
  | { kind: 'copy'; mode: 'fresh' | 'complete'; entries: TemplateEntry[]; skipped: number };

/** The three files that make a folder a vault (same as the preflight's `vault` check). */
export const VAULT_MARKERS = ['.claude/skills/macro-review/SKILL.md', 'indices-kb/tools/scale.py', 'indices-kb/config/scale.json'];

/**
 * Decide what to copy. `existing` is every relative path already in the target
 * (files and directories), '' meaning an empty or absent target.
 */
export function planVaultCopy(template: TemplateEntry[], existing: Set<string>): VaultCopyPlan {
  const nonEmpty = existing.size > 0;
  if (!nonEmpty) return { kind: 'copy', mode: 'fresh', entries: template, skipped: 0 };
  const isVault = VAULT_MARKERS.some((m) => existing.has(m));
  if (!isVault) return { kind: 'refuse', reason: 'not-a-vault' };
  const entries = template.filter((e) => e.rel === TEMPLATE_MARKER || !existing.has(e.rel));
  return { kind: 'copy', mode: 'complete', entries, skipped: template.length - entries.length };
}

/** Walk a directory into relative entries, forward slashes, directories first. */
export async function listTree(root: string): Promise<TemplateEntry[]> {
  const out: TemplateEntry[] = [];
  const walk = async (dir: string, rel: string): Promise<void> => {
    let entries;
    try {
      entries = await fs.readdir(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      const childRel = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) {
        out.push({ rel: childRel, dir: true });
        await walk(path.join(dir, e.name), childRel);
      } else {
        out.push({ rel: childRel, dir: false });
      }
    }
  };
  await walk(root, '');
  return out;
}

/** The version string a template (or a vault laid down from one) carries. */
export async function readTemplateVersion(dir: string): Promise<string> {
  try {
    const raw = JSON.parse(await fs.readFile(path.join(dir, TEMPLATE_MARKER), 'utf8')) as { version?: unknown };
    return typeof raw?.version === 'string' || typeof raw?.version === 'number' ? String(raw.version) : '';
  } catch {
    return '';
  }
}

/**
 * Perform the copy. Reports one line per directory created and per file
 * written through `push`; throws on a refusal, so the task runner shows it as
 * a failure with the reason.
 */
export async function copyVaultTemplate(templateDir: string, target: string, push: (line: string) => void): Promise<{ mode: 'fresh' | 'complete'; files: number }> {
  const template = await listTree(templateDir);
  if (!template.length) throw new Error(`the vault template at ${templateDir} is empty`);
  const existing = new Set((await listTree(target)).map((e) => e.rel));
  const plan = planVaultCopy(template, existing);
  if (plan.kind === 'refuse') throw new Error(`refused: ${target} is not empty and not a vault (${plan.reason})`);
  push(plan.mode === 'fresh' ? `creating a vault in ${target}` : `completing the vault in ${target} (${plan.skipped} existing entries kept)`);
  await fs.mkdir(target, { recursive: true });
  let files = 0;
  for (const e of plan.entries) {
    const to = path.join(target, ...e.rel.split('/'));
    if (e.dir) {
      await fs.mkdir(to, { recursive: true });
      continue;
    }
    await fs.mkdir(path.dirname(to), { recursive: true });
    await fs.copyFile(path.join(templateDir, ...e.rel.split('/')), to);
    files += 1;
    push(`+ ${e.rel}`);
  }
  push(`${files} files written`);
  return { mode: plan.mode, files };
}

/** UTF-8 byte order mark. */
export const UTF8_BOM = Buffer.from([0xef, 0xbb, 0xbf]);

/**
 * The two scripts the Task Scheduler runs carry Russian window titles. Windows
 * PowerShell 5.1 reads a `.ps1` without a BOM in the system ANSI code page
 * (cp1251 here), so «Макро US 13.10» reached `wt.exe --title` and `claude -n`
 * as mojibake (issue #12). A BOM is the one thing that makes 5.1 read UTF-8.
 */
export const BOM_SCRIPTS = ['indices-kb/tools/macro_review.ps1', 'indices-kb/tools/macro_schedule.ps1'];

/**
 * The same bytes with a UTF-8 BOM in front. Unchanged when one is there
 * already, when the file is UTF-16 (its own BOM), or when the bytes are not
 * valid UTF-8 at all (an ANSI save): a BOM on those would corrupt them.
 */
export function withUtf8Bom(bytes: Buffer): Buffer {
  if (bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) return bytes;
  if (bytes.length >= 2 && ((bytes[0] === 0xff && bytes[1] === 0xfe) || (bytes[0] === 0xfe && bytes[1] === 0xff))) return bytes;
  try {
    new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return bytes;
  }
  return Buffer.concat([UTF8_BOM, bytes]);
}

/**
 * Prepend the BOM to `BOM_SCRIPTS` inside a vault laid down before the fix:
 * `copyVaultTemplate` never overwrites, so a 0.2.1 vault keeps its scripts
 * forever. Only the three bytes change — the text is the user's, byte for
 * byte. Synchronous on purpose: it runs between "task started" and the spawn,
 * and two small files are not worth a window in which a second press could
 * start the same task. Returns the relative paths it rewrote; missing files
 * are skipped.
 */
export function ensureScriptBoms(vault: string): string[] {
  const fixed: string[] = [];
  for (const rel of BOM_SCRIPTS) {
    const p = path.join(vault, ...rel.split('/'));
    let bytes: Buffer;
    try {
      bytes = readFileSync(p);
    } catch {
      continue;
    }
    const next = withUtf8Bom(bytes);
    if (next === bytes) continue;
    writeFileSync(p, next);
    fixed.push(rel);
  }
  return fixed;
}
