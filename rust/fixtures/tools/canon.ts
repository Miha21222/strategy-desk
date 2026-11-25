// How a TS value becomes a committed golden file.
//
// 1. JSON round trip: exactly what the old app sends over IPC and what
//    scripts/preview/dump.ts writes (`undefined` fields vanish, -0 becomes 0).
// 2. The absolute vault path becomes "$VAULT", so the files do not depend on
//    the machine that captured them.
// 3. A screenshot payload (`data:image/png;base64,…`) becomes
//    {"$shot": "<vault-relative path>", "bytes": N}: desk-core hands out shot
//    paths and the UI decodes the PNG itself (TZ «Почему сейчас медленно»).
//    The path is found by matching the decoded bytes against the PNGs under
//    <vault>/indices-kb/shots; a payload that matches none fails the capture.
// 4. Pretty-printed with a one-space indent and a final newline, keys in the
//    TS order, so a diff in review reads like the object.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';

const DATA_URL = /^data:image\/png;base64,/;

function pngIndex(vault: string): Map<string, string> {
  const out = new Map<string, string>();
  const root = path.join(vault, 'indices-kb', 'shots');
  const walk = (dir: string) => {
    let names: string[];
    try {
      names = readdirSync(dir);
    } catch {
      return;
    }
    for (const n of names) {
      const full = path.join(dir, n);
      if (statSync(full).isDirectory()) walk(full);
      else if (n.toLowerCase().endsWith('.png')) out.set(readFileSync(full).toString('base64'), path.relative(vault, full).split(path.sep).join('/'));
    }
  };
  walk(root);
  return out;
}

export function shotRef(dataUrl: string, vault: string, index?: Map<string, string>): { $shot: string; bytes: number } {
  const b64 = dataUrl.replace(DATA_URL, '');
  const rel = (index ?? pngIndex(vault)).get(b64);
  if (!rel) throw new Error(`a base64 PNG payload matches no file under ${vault}/indices-kb/shots`);
  return { $shot: rel, bytes: Buffer.from(b64, 'base64').length };
}

export function canonValue(value: unknown, vault: string): unknown {
  let index: Map<string, string> | undefined;
  const fix = (x: unknown): unknown => {
    if (typeof x === 'string') {
      if (DATA_URL.test(x)) return shotRef(x, vault, (index ??= pngIndex(vault)));
      return vault ? x.split(vault).join('$VAULT') : x;
    }
    if (Array.isArray(x)) return x.map(fix);
    if (x && typeof x === 'object') return Object.fromEntries(Object.entries(x).map(([k, v]) => [k, fix(v)]));
    return x;
  };
  return fix(JSON.parse(JSON.stringify(value) ?? 'null'));
}

export function canon(value: unknown, vault: string): string {
  return JSON.stringify(canonValue(value, vault), null, 1) + '\n';
}

/** One case per line: big case tables stay greppable and diff line by line. */
export function canonLines(head: Record<string, unknown>, tables: Record<string, unknown[]>, vault = ''): string {
  const parts = Object.entries(head).map(([k, v]) => ` ${JSON.stringify(k)}: ${JSON.stringify(canonValue(v, vault))}`);
  for (const [k, rows] of Object.entries(tables)) {
    const body = rows.map((r) => '  ' + JSON.stringify(canonValue(r, vault))).join(',\n');
    parts.push(` ${JSON.stringify(k)}: [\n${body}\n ]`);
  }
  return `{\n${parts.join(',\n')}\n}\n`;
}
