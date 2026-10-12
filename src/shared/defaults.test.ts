// What an existing config means where it is silent: the owner's install must
// not wake up with a second `tradingview` server in its macro chats or a
// switched-off knowledge base, while a fresh install takes the new defaults.
import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS, DEFAULT_TVMCP_PATH, DEFAULT_VAULT_PATH, legacyConfigPatch, macroMcpFor } from './defaults';

describe('macroMcpFor', () => {
  it('is bundled for a fresh install, user for a config from before the field, and otherwise what the file says', () => {
    expect(macroMcpFor(null)).toBe('bundled');
    expect(macroMcpFor({ vaultPath: 'C:\\vault' })).toBe('user');
    expect(macroMcpFor({ macroMcp: 'bundled' })).toBe('bundled');
    expect(macroMcpFor({ macroMcp: 'nonsense' })).toBe('user');
  });
});

describe('legacyConfigPatch', () => {
  it('adds nothing for a fresh install', () => {
    expect(legacyConfigPatch(null)).toEqual({});
    expect(DEFAULT_SETTINGS.macroMcp).toBe('bundled');
    expect(DEFAULT_SETTINGS.knowledgeBase).toBe(false);
  });

  it('keeps an old config on the user server and with the knowledge base on', () => {
    expect(legacyConfigPatch({ vaultPath: 'C:\\vault' })).toEqual({ macroMcp: 'user', knowledgeBase: true });
    expect(legacyConfigPatch({ vaultPath: 'C:\\vault', knowledgeBase: false })).toEqual({ macroMcp: 'user' });
  });
});

describe('the paths', () => {
  it('no longer name the owner`s machine', () => {
    expect(DEFAULT_TVMCP_PATH).toBe('');
    expect(DEFAULT_VAULT_PATH).toBe('');
    expect(JSON.stringify(DEFAULT_SETTINGS)).not.toContain('Admin');
  });
});
