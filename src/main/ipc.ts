// IPC surface. Every handler is registered once; the renderer reaches them
// through the typed `window.desk` bridge in src/preload/index.ts.
import { app, dialog, ipcMain, shell, type BrowserWindow } from 'electron';
import type { ChatEvent, ChatKind, NewReport, PickKind, SendArgs, SettingsPatch, TraderProfile } from '@shared/api';
import { StressReportSchema } from '@shared/blocks';
import { CHAT_KINDS } from '@shared/api';
import { isAllowedTvTool, type PanelDayState, type TvCallResult } from '@shared/panel';
import { isTaskId, type TaskEvent, type TaskId } from '@shared/tasks';
import { clampZoom } from '@shared/zoom';
import { dropWarm, prewarmFor, startChat, stopChat } from './agent';
import { closeFor } from './session';
import { runPreflight } from './preflight';
import { freezeForChat } from './prompt';
import { getPropFirms } from './rules';
import { clearRuntime } from './runtime';
import { allTaskStates, anyTaskRunning, runTask, stopTask, taskCommand, taskState } from './tasks';
import { getStatus, invalidateTvmcpCache } from './status';
import * as store from './store';
import { callTool } from './tv-cli';
import { launchTradingView } from './tv-launch';
import { listWorkflows } from './workflows';

const SECRET_NAMES = new Set(['apiKey', 'oauthToken']);


/** What each `dialog:pick` kind opens. The renderer names a kind, not a filter. */
const PICKERS: Record<PickKind, { properties: ('openFile' | 'openDirectory')[]; filters?: Electron.FileFilter[] }> = {
  'tvmcp-dir': { properties: ['openDirectory'] },
  'gt-rag-dir': { properties: ['openDirectory'] },
  'risk-rules': { properties: ['openFile'], filters: [{ name: 'JSON', extensions: ['json'] }, { name: 'All', extensions: ['*'] }] },
  journal: { properties: ['openFile'], filters: [{ name: 'CSV / TSV / JSON', extensions: ['csv', 'tsv', 'json', 'txt'] }, { name: 'All', extensions: ['*'] }] },
  'field-notes': { properties: ['openFile'], filters: [{ name: 'Markdown', extensions: ['md', 'markdown', 'txt'] }, { name: 'All', extensions: ['*'] }] },
};

export function registerIpc(getWindow: () => BrowserWindow | null): void {
  const emit = (ev: ChatEvent) => {
    const win = getWindow();
    if (win && !win.isDestroyed()) win.webContents.send('chat:event', ev);
  };

  ipcMain.handle('status:get', async () => getStatus(await store.getSettings()));

  // The setup diagnosis. Read-only by construction: it probes, it never fixes.
  // A fix is either copyable text (the user's own toolchain and apps) or a task
  // id the renderer sends back through `tasks:run` — see @shared/tasks.
  ipcMain.handle('preflight:run', async () => runPreflight(await store.getSettings()));

  // A native open dialog and nothing else — the renderer gets a path back and
  // stores it through settings:set like any other field.
  ipcMain.handle('dialog:pick', async (_e, kind: PickKind): Promise<string> => {
    const win = getWindow();
    const spec = PICKERS[kind];
    if (!spec || !win || win.isDestroyed()) return '';
    const res = await dialog.showOpenDialog(win, { properties: spec.properties, filters: spec.filters });
    return res.canceled ? '' : (res.filePaths[0] ?? '');
  });

  ipcMain.handle('tv:launch', async () => {
    const cfg = await store.getSettings();
    return launchTradingView(cfg.cdpPort, cfg.tvDesktopExe);
  });

  ipcMain.handle('settings:get', () => store.getSettings());
  ipcMain.handle('settings:set', async (_e, patch: SettingsPatch) => {
    const before = await store.getSettings();
    const next = await store.setSettings(patch ?? {});
    if (before.tvmcpPath !== next.tvmcpPath || before.toolsets !== next.toolsets) invalidateTvmcpCache();
    // Anything the session was built with changed: the waiting one is stale.
    dropWarm();
    return next;
  });
  // Window zoom. The whole UI is laid out in px, so the only honest "make it
  // bigger" is Chromium's zoom factor; the renderer owns the number (it lives
  // in settings) and this handler just applies it, clamped.
  ipcMain.handle('ui:zoom', (_e, factor: number) => {
    const win = getWindow();
    const z = clampZoom(factor);
    if (win && !win.isDestroyed()) win.webContents.setZoomFactor(z);
    return z;
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

  // The four things the app may do for the user. The renderer sends an id from
  // the closed list in @shared/tasks; the command is assembled in tasks.ts. An
  // unknown id is refused here, exactly like an unlisted tv tool above.
  const emitTask = (ev: TaskEvent) => {
    const win = getWindow();
    if (win && !win.isDestroyed()) win.webContents.send('task:event', ev);
  };
  ipcMain.handle('tasks:run', async (_e, id: TaskId) => {
    if (!isTaskId(id)) throw new Error('unknown task');
    return runTask(await store.getSettings(), id, emitTask);
  });
  ipcMain.on('tasks:stop', (_e, id: TaskId) => {
    if (isTaskId(id)) stopTask(id);
  });
  ipcMain.handle('tasks:state', (_e, id: TaskId) => {
    if (!isTaskId(id)) throw new Error('unknown task');
    return taskState(id);
  });
  ipcMain.handle('tasks:list', () => allTaskStates());
  ipcMain.handle('tasks:command', async (_e, id: TaskId) => {
    if (!isTaskId(id)) throw new Error('unknown task');
    return taskCommand(await store.getSettings(), id);
  });

  // Deleting the provisioned environment. Destructive, so the renderer asks
  // first; the tvmcp cache is dropped with it or the next probe would report a
  // server that no longer has an interpreter.
  ipcMain.handle('runtime:clear', async () => {
    // Deleting the venv from under a running `uv sync` leaves a half-built
    // environment with a stale marker — refuse rather than race it.
    if (anyTaskRunning()) throw new Error('a setup task is running');
    const r = await clearRuntime();
    invalidateTvmcpCache();
    dropWarm();
    return r;
  });

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
    // The chat is gone; its session (a CLI process plus tvmcp) must go too.
    closeFor(key);
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
  // The renderer says which chat the user is looking at; the session for its
  // next turn is started now, so the boot happens while they type.
  ipcMain.on('chat:prewarm', (_e, args: { chatKey?: string | null; mode?: string }) => {
    const mode = args?.mode === 'saver' ? 'saver' : 'full';
    void prewarmFor(typeof args?.chatKey === 'string' ? args.chatKey : null, mode);
  });

  ipcMain.handle('open:external', async (_e, url: string) => {
    if (typeof url !== 'string' || !/^https:\/\//i.test(url)) return false;
    await shell.openExternal(url);
    return true;
  });

  ipcMain.handle('app:version', () => app.getVersion());
}
