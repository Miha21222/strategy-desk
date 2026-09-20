// Prompt assembly around the "Live field notes" section: it is there when the
// file is, absent when it is not, and a missing file is never an error.
//
// Electron is stubbed because the module only wants the app version and the
// app path from it; the rest is plain file reading.
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'desk-prompt-'));
const notesFile = join(dir, 'my-notes.md');
const missingFile = join(dir, 'not-here.md');

vi.mock('electron', () => ({
  app: {
    getVersion: () => '0.0.0-test',
    getAppPath: () => process.cwd(),
    getPath: () => dir,
    isPackaged: false,
  },
}));

const profile = { callName: 'Тест', traderType: '', markets: '', style: '', riskRules: '', propFirms: '', morningChecklist: [] };

let assemblePrompt: typeof import('./prompt').assemblePrompt;
let loadFieldNotes: typeof import('./prompt').loadFieldNotes;
let notesBlock: typeof import('./prompt').notesBlock;

beforeAll(async () => {
  writeFileSync(notesFile, '# Live field notes\n\n- the owner already paid for this one\n', 'utf8');
  ({ assemblePrompt, loadFieldNotes, notesBlock } = await import('./prompt'));
});

afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('loadFieldNotes', () => {
  it('reads the shipped data/copilot-notes.md when no override is set', async () => {
    const notes = await loadFieldNotes();
    expect(notes).toBeTruthy();
    expect(notes).toContain('Live field notes');
  });

  it('an override wins outright', async () => {
    expect(await loadFieldNotes(notesFile)).toContain('the owner already paid for this one');
  });

  it('a missing override file is "no notes", not an error and not a silent fallback', async () => {
    await expect(loadFieldNotes(missingFile)).resolves.toBeNull();
    // Whitespace is not a path.
    expect(await loadFieldNotes('   ')).toContain('Live field notes');
  });

  it('normalises the file\'s own h1 to the section heading, and empties to nothing', () => {
    expect(notesBlock('# Live field notes\n- one')).toBe('## Live field notes\n- one');
    expect(notesBlock(null)).toBe('');
  });
});

describe('the assembled system prompt', () => {
  it('carries the field notes section when the file exists', async () => {
    const p = await assemblePrompt(profile, null, notesFile);
    expect(p.text).toContain('## Live field notes');
    expect(p.text).toContain('the owner already paid for this one');
    expect(p.hash).toHaveLength(16);
  });

  it('has no such section at all when the file does not exist', async () => {
    const p = await assemblePrompt(profile, null, missingFile);
    expect(p.text).not.toContain('## Live field notes');
    // ...and the rest of the prompt is still there.
    expect(p.text).toContain('## Trader profile');
    expect(p.text).toContain('## Hub');
  });

  it('the notes change the hash, so a frozen chat and a new one are distinguishable', async () => {
    const withNotes = await assemblePrompt(profile, null, notesFile);
    const without = await assemblePrompt(profile, null, missingFile);
    expect(withNotes.hash).not.toBe(without.hash);
    // Same inputs, same bytes: the prompt cache prefix stays stable.
    expect((await assemblePrompt(profile, null, notesFile)).hash).toBe(withNotes.hash);
  });
});
