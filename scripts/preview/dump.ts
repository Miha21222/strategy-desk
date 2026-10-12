import { writeFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { listDays, loadDay } from '../../src/main/macro-state';
import { DEFAULT_SETTINGS, DEFAULT_PROFILE } from '../../src/shared/defaults';

const vault = process.argv[2];
const out = process.argv[3];
mkdirSync(out, { recursive: true });
(async () => {
  const index = await listDays(vault);
  writeFileSync(path.join(out, 'index.json'), JSON.stringify({ ...index, demo: false }));
  for (const d of index.days) writeFileSync(path.join(out, `day_${d}.json`), JSON.stringify(await loadDay(vault, d)));
  writeFileSync(path.join(out, 'defaults.json'), JSON.stringify({ settings: DEFAULT_SETTINGS, profile: DEFAULT_PROFILE }));
  console.log('days', index.days);
})();
