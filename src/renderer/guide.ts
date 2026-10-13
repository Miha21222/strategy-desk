// The Vite glue for the in-app user guide: docs/GUIDE.md comes in as a string,
// the screenshots under docs/guide/img become bundled assets (an absent image
// is simply not in the map). The GIFs are not shipped — too big for the
// installer — so the markdown's gif lines render as a note, see GuideView.
import { imageMap, sliceUsage } from './guide-text';
import guideMd from '../../docs/GUIDE.md?raw';

// Only the screenshots sections 5–14 show; the installer and wizard shots
// (01, 12–18) stay on GitHub, ~1.5 MB the installer does not need.
const imgs = import.meta.glob(['../../docs/guide/img/*.png', '!../../docs/guide/img/01-*.png', '!../../docs/guide/img/1[2-8]*-*.png'], { eager: true, query: '?url', import: 'default' });

export function usageGuide(): { md: string; images: Record<string, string> } {
  return { md: sliceUsage(guideMd), images: imageMap(imgs) };
}
