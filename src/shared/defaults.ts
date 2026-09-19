import type { Settings, TraderProfile } from './api';

export const DEFAULT_TVMCP_PATH = 'C:\Users\Admin\Projects\tradingview-mcp';
export const DEFAULT_CDP_PORT = 9223;
export const DEFAULT_TOOLSETS = 'hybrid,desktop';

export const DEFAULT_SETTINGS: Settings = {
  authMode: 'subscription',
  hasApiKey: false,
  hasOauthToken: false,
  tvmcpPath: DEFAULT_TVMCP_PATH,
  cdpPort: DEFAULT_CDP_PORT,
  toolsets: DEFAULT_TOOLSETS,
  officialMcp: false,
  model: '',
  effort: '',
  permissionPreset: 'standard',
  theme: 'system',
  tvDesktopExe: '',
  sidebar: {},
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
