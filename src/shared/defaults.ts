import type { Settings, TraderProfile } from './api';
import { DEFAULT_ENGINE } from './engine';

export const DEFAULT_TVMCP_PATH = 'C:\\Users\\Admin\\Projects\\tradingview-mcp';
export const DEFAULT_CDP_PORT = 9223;
/** The owner's vault, where the macro-review skill and its state files live. */
export const DEFAULT_VAULT_PATH = 'C:\\Users\\Admin\\Documents\\Obsidian Notes\\Trading\\Trading Strategy Assistant';
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
  knowledgeBase: DEFAULT_KNOWLEDGE_BASE,
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
