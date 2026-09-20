import type { Settings, TraderProfile } from './api';

export const DEFAULT_TVMCP_PATH = 'C:\\Users\\Admin\\Projects\\tradingview-mcp';
export const DEFAULT_CDP_PORT = 9223;
/** Where the preflight's `git clone` fix points. The app never runs it itself. */
export const DEFAULT_TVMCP_REPO = 'https://github.com/Miha21222/tradingview-mcp.git';
// `hybrid` already carries scan / journal / calendar / sentinel; `session` is
// the opt-in account-cookie tier the day panel needs for broker CFDs like
// PEPPERSTONE:US500 (it also wants TV_SESSIONID in the environment).
export const DEFAULT_TOOLSETS = 'hybrid,desktop,session';

export const DEFAULT_SETTINGS: Settings = {
  authMode: 'subscription',
  hasApiKey: false,
  hasOauthToken: false,
  tvmcpPath: DEFAULT_TVMCP_PATH,
  tvmcpRepo: DEFAULT_TVMCP_REPO,
  cdpPort: DEFAULT_CDP_PORT,
  toolsets: DEFAULT_TOOLSETS,
  officialMcp: false,
  model: '',
  effort: '',
  permissionPreset: 'standard',
  theme: 'system',
  tvDesktopExe: '',
  sidebar: {},
  activeWorkflow: 'strategy-lab',
  shelfWorkflows: {},
  riskRulesPath: '',
  journalPath: '',
  fieldNotesPath: '',
  setupCompletedAt: 0,
  setupSkipped: [],
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
