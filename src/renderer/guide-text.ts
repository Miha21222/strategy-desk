// The pure half of the in-app user guide: which part of README.md (the user guide) the Help
// tab shows and how Vite's glob keys map onto the image paths the markdown
// uses. No DOM, no Vite — guide.ts owns the `?raw` import and the glob.

/** The first line of the usage part of README.md (the user guide); sections 1–4 are installation. */
export const USAGE_MARKER = '## 5. Главный экран';

/**
 * Everything from the usage marker to the end of the guide. If the marker is
 * missing (someone renamed the section), the whole guide comes back rather than
 * nothing: a longer Help tab is better than an empty one.
 */
export function sliceUsage(md: string): string {
  const lines = md.split('\n');
  const at = lines.findIndex((l) => l.trimEnd() === USAGE_MARKER);
  return at < 0 ? md : lines.slice(at).join('\n');
}

/**
 * `import.meta.glob('../../docs/guide/img/*.png', ...)` keys are the paths as
 * written in the glob ('../../docs/guide/img/01-x.png'); the markdown refers to
 * 'docs/guide/img/01-x.png'. Non-string values (a lazy loader) are dropped.
 */
export function imageMap(glob: Record<string, unknown>): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [key, url] of Object.entries(glob)) {
    if (typeof url !== 'string') continue;
    const i = key.indexOf('docs/guide/');
    if (i < 0) continue;
    out[key.slice(i)] = url;
  }
  return out;
}
