// The Help tab shows the usage part of README.md. These pin the slice
// point to the real file (the marker is a coupling to a document this code
// does not own) and the glob-key → markdown-path mapping for the screenshots.
import { describe, expect, it } from 'vitest';
import { imageMap, sliceUsage, USAGE_MARKER } from './guide-text';
import guideMd from '../../README.md?raw';

describe('sliceUsage', () => {
  it('returns from the usage marker to the end', () => {
    const md = '# Title\n\n## 1. Install\n\nstuff\n\n## 5. Главный экран\n\nbody\n\n## 6. Утро\n\nmore\n';
    const out = sliceUsage(md);
    expect(out.startsWith(USAGE_MARKER)).toBe(true);
    expect(out).toContain('## 6. Утро');
    expect(out).not.toContain('## 1. Install');
  });

  it('matches the marker only as a whole line', () => {
    const md = 'see ## 5. Главный экран below\n\n## 5. Главный экран\nbody';
    expect(sliceUsage(md)).toBe('## 5. Главный экран\nbody');
  });

  it('falls back to the whole guide when the marker is missing', () => {
    const md = '# Title\n\n## 1. Install\n';
    expect(sliceUsage(md)).toBe(md);
  });

  it('finds the marker in the real README.md', () => {
    const out = sliceUsage(guideMd);
    expect(out.startsWith(USAGE_MARKER)).toBe(true);
    expect(out).toContain('## 14. Словарь');
    expect(out).not.toContain('## 4. Первый запуск');
  });
});

describe('imageMap', () => {
  it('maps glob keys onto the paths the markdown uses', () => {
    const map = imageMap({
      '../../docs/guide/img/01-desk-first-look.png': './assets/01-desk-first-look-abc123.png',
      '../../docs/guide/img/02-desk-after-read.png': './assets/02-desk-after-read-def456.png',
    });
    expect(map).toEqual({
      'docs/guide/img/01-desk-first-look.png': './assets/01-desk-first-look-abc123.png',
      'docs/guide/img/02-desk-after-read.png': './assets/02-desk-after-read-def456.png',
    });
  });

  it('tolerates an empty glob and drops non-string values', () => {
    expect(imageMap({})).toEqual({});
    expect(imageMap({ '../../docs/guide/img/x.png': () => Promise.resolve('') })).toEqual({});
    expect(imageMap({ 'nothing/here.png': 'u' })).toEqual({});
  });
});
