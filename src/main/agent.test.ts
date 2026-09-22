// The two suffixes appended after the frozen system prompt. Electron and the
// store are stubbed: both functions are pure string builders.
import { describe, expect, it, vi } from 'vitest';
import { langSuffix, modeSuffix } from './agent';

vi.mock('electron', () => ({ app: { getVersion: () => '0.0.0-test', getAppPath: () => process.cwd(), getPath: () => process.cwd(), isPackaged: false } }));

describe('language suffix', () => {
  it('names the language and says English prompts are app wiring', () => {
    const ru = langSuffix('ru');
    expect(ru).toContain('## Answer language');
    expect(ru).toContain('Always answer in Russian');
    expect(ru).toMatch(/quick actions|workflow prompts/);
    expect(langSuffix('en')).toContain('Always answer in English');
  });

  it('auto adds nothing, so the base "language the user writes in" rule stands', () => {
    expect(langSuffix('auto')).toBe('');
  });

  it('is a suffix that starts on its own blank line, like the mode suffix', () => {
    expect(langSuffix('ru').startsWith('\n\n')).toBe(true);
    expect(modeSuffix('saver').startsWith('\n\n')).toBe(true);
    expect(modeSuffix('full')).toBe('');
  });
});
