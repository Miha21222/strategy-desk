// The old app's answer for every hand-made edge vault (rust/fixtures/edge/<case>).
//
//   node dump-edge.cjs <edge_root>
//
// For each case: listDays() → expected/index.json, and loadDay() for every
// listed date plus the dates named in case.json → expected/day_<D>.json. A
// loadDay that throws is recorded as {"$error": "<message>"}: that is the
// old app's outcome, and the case's `compare` says what Rust must do instead.
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { listDays, loadDay } from '../../../src/main/macro-state';
import { canon } from './canon';

const root = path.resolve(process.argv[2]);

(async () => {
  for (const name of readdirSync(root).sort()) {
    const dir = path.join(root, name);
    const caseFile = path.join(dir, 'case.json');
    if (!existsSync(caseFile)) continue;
    const meta = JSON.parse(readFileSync(caseFile, 'utf8')) as { dates: string[] };
    const vault = path.join(dir, 'vault');
    const out = path.join(dir, 'expected');
    rmSync(out, { recursive: true, force: true });
    mkdirSync(out, { recursive: true });
    const index = await listDays(vault);
    writeFileSync(path.join(out, 'index.json'), canon(index, vault));
    const dates = [...new Set([...index.days, ...meta.dates])].sort();
    for (const d of dates) {
      let day: unknown;
      try {
        day = await loadDay(vault, d);
      } catch (e) {
        day = { $error: e instanceof Error ? `${e.name}: ${e.message}` : String(e) };
      }
      writeFileSync(path.join(out, `day_${d}.json`), canon(day, vault));
    }
    console.log('edge', name, 'days', dates.join(' '));
  }
})();
