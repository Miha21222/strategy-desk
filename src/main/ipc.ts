// IPC surface. Every handler is registered once; the renderer reaches them
// through the typed `window.desk` bridge in src/preload/index.ts.
import { app, ipcMain, shell, type BrowserWindow } from 'electron';
import type { ChatEvent, ChatKind, NewReport, SendArgs, SettingsPatch, TraderProfile } from '@shared/api';
import { StressReportSchema } from '@shared/blocks';
import { CHAT_KINDS } from '@shared/api';
import { isAllowedTvTool, type PanelDayState, type TvCallResult } from '@shared/panel';
import { startChat, stopChat } from './agent';
import { freezeForChat } from './prompt';
import { getPropFirms } from './rules';
import { getStatus, invalidateTvmcpCache } from './status';
import * as store from './store';
import { callTool } from './tv-cli';
import { launchTradingView } from './tv-launch';
import { listWorkflows } from './workflows';

const SECRET_NAMES = new Set(['apiKey', 'oauthToken']);

export function registerIpc(getWindow: () => BrowserWindow | null): void {
  const emit = (ev: ChatEvent) => {
    const win = getWindow();
    if (win && !win.isDestroyed()) win.webContents.send('chat:event', ev);
  };

  ipcMain.handle('status:get', async () => getStatus(await store.getSettings()));

  ipcMain.handle('tv:launch', async () => {
    const cfg = await store.getSettings();
    return launchTradingView(cfg.cdpPort, cfg.tvDesktopExe);
  });

  ipcMain.handle('settings:get', () => store.getSettings());
  ipcMain.handle('settings:set', async (_e, patch: SettingsPatch) => {
    const before = await store.getSettings();
    const next = await store.setSettings(patch ?? {});
    if (before.tvmcpPath !== next.tvmcpPath || before.toolsets !== next.toolsets) invalidateTvmcpCache();
    return next;
  });
  ipcMain.handle('settings:setSecret', (_e, name: 'apiKey' | 'oauthToken', value: string) => {
    if (!SECRET_NAMES.has(name)) throw new Error('unknown secret');
    return store.setSecret(name, String(value ?? ''));
  });
  ipcMain.handle('settings:clearSecret', (_e, name: 'apiKey' | 'oauthToken') => {
    if (!SECRET_NAMES.has(name)) throw new Error('unknown secret');
    return store.clearSecret(name);
  });

  // One channel for every deterministic tvmcp call the day panel makes. The
  // allowlist is enforced HERE, in the main process: the renderer names a tool,
  // it does not get to name a command. Nothing in the list draws on the chart,
  // writes a journal row or touches a script.
  ipcMain.handle('tv:call', async (_e, name: string, args: Record<string, unknown>): Promise<TvCallResult> => {
    if (!isAllowedTvTool(name)) {
      return { ok: false, error: `tool ${String(name).slice(0, 60)} is not in the allowlist`, warnings: [], ms: 0 };
    }
    const cfg = await store.getSettings();
    return callTool(cfg, name, args && typeof args === 'object' ? args : {});
  });

  ipcMain.handle('panel:get', (_e, workflowId: string, date: string) => store.getPanelState(String(workflowId), String(date)));
  ipcMain.handle('panel:set', (_e, state: PanelDayState) => store.setPanelState(state));
  ipcMain.handle('panel:clear', (_e, workflowId: string, date: string) => store.clearPanelState(String(workflowId), String(date)));

  ipcMain.handle('rules:get', () => getPropFirms());

  ipcMain.handle('profile:get', () => store.getProfile());
  ipcMain.handle('profile:set', (_e, p: TraderProfile) => store.setProfile(p));

  ipcMain.handle('chats:list', () => store.listChats());
  ipcMain.handle('chats:new', async (_e, kind?: ChatKind, workflowId?: string | null) => {
    const shelf: ChatKind = kind && CHAT_KINDS.includes(kind) ? kind : 'chat';
    const chat = await store.newChat(shelf);
    // The caller's choice wins; then the shelf's own workflow, then the active one.
    const cfg = await store.getSettings();
    const wf = typeof workflowId === 'string' ? workflowId : (cfg.shelfWorkflows?.[shelf] ?? cfg.activeWorkflow);
    // Freeze the system prompt now so every turn of this chat reuses the same bytes.
    return freezeForChat(chat, wf || null);
  });
  ipcMain.handle('chats:open', async (_e, key: string) => {
    const chat = await store.getChat(key);
    if (!chat) return null;
    return { chat, messages: await store.getTranscript(key) };
  });
  ipcMain.handle('chats:rename', (_e, key: string, title: string) => store.updateChat(key, { title: String(title ?? '').slice(0, 120) }));
  ipcMain.handle('chats:delete', async (_e, key: string) => {
    await stopChat(key);
    return store.deleteChat(key);
  });
  ipcMain.handle('chats:pin', (_e, key: string, pinned: boolean) => store.updateChat(key, { pinned: Boolean(pinned) }));
  ipcMain.handle('chats:setKind', (_e, key: string, kind: ChatKind) => {
    if (!CHAT_KINDS.includes(kind)) throw new Error('unknown kind');
    return store.updateChat(key, { kind });
  });

  ipcMain.handle('chats:addReport', (_e, key: string, report: NewReport) => {
    if (!report || report.kind !== 'stressreport') throw new Error('unknown report kind');
    const data = StressReportSchema.parse(report.data);
    return store.addReport(String(key), { kind: 'stressreport', version: data.version, grade: data.grade, strategy: data.strategy, data });
  });
  ipcMain.handle('chats:deleteReport', (_e, key: string, reportId: string) => store.deleteReport(String(key), String(reportId)));

  ipcMain.handle('workflows:list', () => listWorkflows());

  ipcMain.on('chat:send', (_e, args: SendArgs) => {
    if (!args || typeof args.chatKey !== 'string' || typeof args.text !== 'string') return;
    const mode = args.mode === 'saver' ? 'saver' : 'full';
    void startChat({ chatKey: args.chatKey, text: args.text, mode }, emit).catch((e) => {
      emit({ chatKey: args.chatKey, kind: 'error', message: String((e as Error)?.message ?? e) });
    });
  });
  ipcMain.on('chat:stop', (_e, chatKey: string) => {
    void stopChat(String(chatKey));
  });

  ipcMain.handle('open:external', async (_e, url: string) => {
    if (typeof url !== 'string' || !/^https:\/\//i.test(url)) return false;
    await shell.openExternal(url);
    return true;
  });

  ipcMain.handle('app:version', () => app.getVersion());
}
