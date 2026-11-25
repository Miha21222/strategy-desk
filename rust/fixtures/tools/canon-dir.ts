// Canonicalize what scripts/preview/dump.ts wrote (see canon.ts for the rules).
//
//   node canon-dir.cjs <dump_out_dir> <vault> <expected_dir>
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { canon } from './canon';

const [src, vault, dst] = process.argv.slice(2).map((p) => path.resolve(p));
mkdirSync(dst, { recursive: true });
for (const n of readdirSync(src).sort()) {
  if (!n.endsWith('.json')) continue;
  writeFileSync(path.join(dst, n), canon(JSON.parse(readFileSync(path.join(src, n), 'utf8')), vault));
}
