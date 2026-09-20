// Prop-firm rule book. Shipped as data/prop-firms.json next to the app and read
// once per process; the renderer asks for it through `rules:get` and hands it to
// the analytics module. Rules change often, so every account carries its own
// `verified` date and `confidence` - the UI shows them rather than pretending
// the numbers are current.
import type { PropFirmsData } from '../analytics/rules';
import { readResource } from './resources';

let cached: PropFirmsData | null = null;

/**
 * `data/**` ships under `files:` in electron-builder.yml, so a packaged build
 * finds the rule book inside app.asar; the resourcesPath candidate (last in the
 * list) is what a hand-dropped override next to app.asar would use.
 */
export async function getPropFirms(): Promise<PropFirmsData | null> {
  if (cached) return cached;
  const raw = await readResource('data', 'prop-firms.json');
  if (raw === null) return null;
  try {
    cached = JSON.parse(raw) as PropFirmsData;
  } catch {
    return null;
  }
  return cached;
}
