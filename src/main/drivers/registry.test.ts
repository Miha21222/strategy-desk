// Picking the engine for a chat. The interesting case is an engine the build
// knows about but cannot run yet: it must fail with a key the UI can explain,
// not with `undefined is not a function` halfway through a turn.
import { describe, expect, it, vi } from 'vitest';
import { ENGINE_IDS, ENGINES, engineOf, isEngineId } from '@shared/engine';

vi.mock('electron', () => ({
  app: { getVersion: () => '0.0.0-test', getAppPath: () => process.cwd(), getPath: () => process.cwd(), isPackaged: false },
  safeStorage: { isEncryptionAvailable: () => false, encryptString: () => Buffer.from(''), decryptString: () => '' },
}));

describe('engine ids', () => {
  it('falls back to Claude for anything unknown, including an old settings file', () => {
    expect(engineOf('codex')).toBe('codex');
    expect(engineOf(undefined)).toBe('claude');
    expect(engineOf('gpt')).toBe('claude');
    expect(isEngineId('claude')).toBe(true);
    expect(isEngineId('')).toBe(false);
  });

  it('describes every engine, so the UI can ask instead of branching', () => {
    for (const id of ENGINE_IDS) expect(ENGINES[id]).toBeTruthy();
    // Measured 2026-09-23: Codex cannot forbid a tool by name and reports no
    // USD on a subscription. If either ever changes, this row changes with it.
    expect(ENGINES.codex.toolDenyList).toBe(false);
    expect(ENGINES.codex.costReported).toBe(false);
    expect(ENGINES.claude.toolDenyList).toBe(true);
  });
});

describe('driver registry', () => {
  it('hands out the Claude driver', async () => {
    const { getDriver } = await import('./index');
    expect(getDriver('claude').id).toBe('claude');
  });

  it('hands out the Codex driver', async () => {
    const { getDriver } = await import('./index');
    expect(getDriver('codex').id).toBe('codex');
  });

  it('gives every driver its own fingerprint namespace', async () => {
    const { getDriver } = await import('./index');
    const spec = {
      engine: 'claude' as const,
      systemPrompt: 'p',
      model: '',
      effort: '' as const,
      cwd: 'C:/w',
      mcpServers: {},
      permissionPreset: 'standard' as const,
    };
    const claude = getDriver('claude').fingerprint(spec);
    const codex = getDriver('codex').fingerprint({ ...spec, engine: 'codex' });
    expect(claude.startsWith('claude:')).toBe(true);
    expect(codex.startsWith('codex:')).toBe(true);
    expect(claude).not.toBe(codex);
  });

  it('lists every driver, so "close everything" reaches all of them', async () => {
    const { allDrivers } = await import('./index');
    expect(allDrivers().map((d) => d.id).sort()).toEqual(['claude', 'codex']);
  });
});
