// What the «Экономный / Полный» toggle actually changes in a turn: the system
// prompt gains a mode block and the effort is pinned. Electron is stubbed; the
// prompt and the settings come from real files under a temp userData dir.
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'desk-mode-'));

vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getAppPath: () => process.cwd(), getPath: () => dir, isPackaged: false },
  safeStorage: { isEncryptionAvailable: () => false, encryptString: () => Buffer.from(''), decryptString: () => '' },
}));

const chat = {
  key: 'chat-1',
  sessionId: null,
  title: 'test',
  createdAt: 0,
  updatedAt: 0,
  pinned: false,
  kind: 'chat' as const,
  tokens: { in: 0, out: 0 },
  costUsd: 0,
  workflowId: null,
  promptHash: null,
};

beforeAll(() => {
  // effort "high" in the settings: full mode must honour it, saver must not.
  writeFileSync(join(dir, 'config.json'), JSON.stringify({ effort: 'high', model: '', answerLang: 'ru' }), 'utf8');
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

describe('saver / full mode', () => {
  it('saver adds the mode block and pins effort to medium', async () => {
    const { buildTurnSpec } = await import('./agent');
    const spec = await buildTurnSpec(chat, 'saver');
    expect(spec.systemPrompt).toContain('## Mode: saver');
    expect(spec.systemPrompt).toContain('Keep tool calls minimal');
    expect(spec.effort).toBe('medium');
  });

  it('full leaves the prompt alone and keeps the configured effort', async () => {
    const { buildTurnSpec } = await import('./agent');
    const spec = await buildTurnSpec(chat, 'full');
    expect(spec.systemPrompt).not.toContain('## Mode: saver');
    expect(spec.effort).toBe('high');
  });

  it('the two modes are different sessions, so switching cannot reuse one', async () => {
    const { buildTurnSpec } = await import('./agent');
    const { claudeDriver } = await import('./drivers/claude');
    const saver = await buildTurnSpec(chat, 'saver');
    const full = await buildTurnSpec(chat, 'full');
    expect(claudeDriver.fingerprint(saver)).not.toBe(claudeDriver.fingerprint(full));
  });
});

describe('saver never costs more than full', () => {
  it('caps effort at medium but keeps a lower one the user chose', async () => {
    const { saverEffort } = await import('./agent');
    expect(saverEffort('saver', 'high')).toBe('medium');
    expect(saverEffort('saver', 'max')).toBe('medium');
    expect(saverEffort('saver', '')).toBe('medium');
    expect(saverEffort('saver', 'low')).toBe('low');
    expect(saverEffort('full', 'high')).toBe('high');
    expect(saverEffort('full', '')).toBeUndefined();
  });
});
