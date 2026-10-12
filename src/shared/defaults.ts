import type { MacroMcp, Settings, TraderProfile } from './api';
import { DEFAULT_ENGINE } from './engine';

/**
 * Empty = «bundled»: a packaged install carries tvmcp and never reads this
 * field (`projectDir` prefers the payload). A developer names their own
 * checkout in the settings; the preflight's `tvmcpClone` check asks for it.
 */
export const DEFAULT_TVMCP_PATH = '';
export const DEFAULT_CDP_PORT = 9223;
/**
 * The vault: the folder holding the macro-review skill, its tools and its
 * state files. Empty here because this module is shared with the renderer and
 * has no `os.homedir()`; the store substitutes `defaultVaultPath()` (Documents\
 * Strategy Desk) when the config holds nothing, so a fresh install points at a
 * folder the wizard can create. The owner's config names his own vault.
 */
export const DEFAULT_VAULT_PATH = '';
/** The folder name under Documents the store falls back to. */
export const DEFAULT_VAULT_DIRNAME = 'Strategy Desk';
/**
 * `macroMcp` for a config written BEFORE the field existed: those installs are
 * the owner's, whose `~/.claude` already provides the `tradingview` server, so
 * the desk must not add a second one. A config with no file at all (fresh
 * install) takes the default, `bundled`.
 */
export const DEFAULT_MACRO_MCP: MacroMcp = 'bundled';
export function macroMcpFor(existing: Record<string, unknown> | null): MacroMcp {
  if (!existing) return DEFAULT_MACRO_MCP;
  const v = existing.macroMcp;
  if (v === 'bundled' || v === 'user') return v;
  return 'user';
}

/**
 * What an EXISTING config file means where it is silent. The two fields whose
 * fresh-install default differs from what an older install should keep:
 * `macroMcp` (the owner's `~/.claude` provides the server) and
 * `knowledgeBase` (absent has always meant «on», see `kbEnabled`). Pure, so
 * the store's one call site is testable; `null` = no file, nothing to migrate.
 */
export function legacyConfigPatch(existing: Record<string, unknown> | null): Partial<Settings> {
  if (!existing) return {};
  const patch: Partial<Settings> = { macroMcp: macroMcpFor(existing) };
  if (!('knowledgeBase' in existing)) patch.knowledgeBase = DEFAULT_KNOWLEDGE_BASE;
  return patch;
}
/** Where the preflight's `git clone` fix points. The app never runs it itself. */
export const DEFAULT_TVMCP_REPO = 'https://github.com/Miha21222/tradingview-mcp.git';
// `hybrid` already carries scan / journal / calendar / sentinel; `session` is
// the opt-in account-cookie tier the day panel needs for broker CFDs like
// PEPPERSTONE:US500 (it also wants TV_SESSIONID in the environment).
export const DEFAULT_TOOLSETS = 'hybrid,desktop,session';
/**
 * The knowledge base is ON by default: it is the reason the installer carries
 * the course at all, and a copilot that cannot cite the course the user learned
 * from is the weaker product. The switch exists for the two cases where it is
 * the wrong answer — a build shipped without the payload, and a user who wants
 * neither the disk nor the extra tools.
 */
export const DEFAULT_KNOWLEDGE_BASE = true;
/**
 * The macro-review build ships no course (SD_BUNDLE_KB=0) and the two traders
 * it is for never saw it; a fresh install starts with the knowledge base off,
 * so the wizard has no orange row about a payload that is not there. An
 * existing config keeps whatever it holds (`kbEnabled` treats absent as on).
 */
export const DEFAULT_KNOWLEDGE_BASE_FRESH = false;

export const DEFAULT_SETTINGS: Settings = {
  engine: DEFAULT_ENGINE,
  codexPath: '',
  authMode: 'subscription',
  hasApiKey: false,
  hasOauthToken: false,
  tvmcpPath: DEFAULT_TVMCP_PATH,
  tvmcpRepo: DEFAULT_TVMCP_REPO,
  cdpPort: DEFAULT_CDP_PORT,
  toolsets: DEFAULT_TOOLSETS,
  officialMcp: false,
  knowledgeBase: DEFAULT_KNOWLEDGE_BASE_FRESH,
  gtRagPath: '',
  model: '',
  effort: '',
  permissionPreset: 'standard',
  theme: 'system',
  uiScale: 1,
  answerLang: 'ru',
  tvDesktopExe: '',
  sidebar: {},
  activeWorkflow: 'strategy-lab',
  shelfWorkflows: {},
  riskRulesPath: '',
  journalPath: '',
  fieldNotesPath: '',
  setupCompletedAt: 0,
  setupSkipped: [],
  vaultPath: DEFAULT_VAULT_PATH,
  legacyUi: false,
  macroMcp: DEFAULT_MACRO_MCP,
  hasTvSession: false,
  tvSessionVerifiedAt: 0,
};

export const DEFAULT_PROFILE: TraderProfile = {
  callName: '',
  traderType: '',
  markets: '',
  style: '',
  riskRules: '',
  propFirms: '',
  morningChecklist: [],
};
