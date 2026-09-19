// Prop-firm rule book. Shipped as data/prop-firms.json next to the app and read
// once per process; the renderer asks for it through `rules:get` and hands it to
// the analytics module. Rules change often, so every account carries its own
// `verified` date and `confidence` - the UI shows them rather than pretending
// the numbers are current.
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { app } from 'electron';
import type { PropFirmsData } from '../analytics/rules';

let cached: PropFirmsData | null = null;

function candidates(): string[] {
  const appPath = app.getAppPath();
  return [
    join(appPath, 'data', 'prop-firms.json'),
    join(appPath, '..', 'data', 'prop-firms.json'),
    join(process.resourcesPath || appPath, 'data', 'prop-firms.json'),
  ];
}

export async function getPropFirms(): Promise<PropFirmsData | null> {
  if (cached) return cached;
  for (const p of candidates()) {
    try {
      const raw = await readFile(p, 'utf8');
      cached = JSON.parse(raw) as PropFirmsData;
      return cached;
    } catch {
      // try the next location
    }
  }
  return null;
}
