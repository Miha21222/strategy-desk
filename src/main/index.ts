import { app, BrowserWindow, shell } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { buildEnv, stopAll } from './agent';
import { registerIpc } from './ipc';
import { getStatus } from './status';
import { chatWorkspaceDir, getSettings } from './store';

const isDev = !app.isPackaged;
let mainWindow: BrowserWindow | null = null;

function createWindow(): BrowserWindow {
  const win = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 1000,
    minHeight: 680,
    show: false,
    autoHideMenuBar: true,
    backgroundColor: '#14161a',
    title: 'Strategy Desk',
    webPreferences: {
      preload: path.join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      spellcheck: false,
    },
  });

  win.once('ready-to-show', () => win.show());

  // Only https links may leave the app, and only in the system browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (/^https:\/\//i.test(url)) void shell.openExternal(url);
    return { action: 'deny' };
  });
  win.webContents.on('will-navigate', (e, url) => {
    const allowed = isDev && process.env.ELECTRON_RENDERER_URL && url.startsWith(process.env.ELECTRON_RENDERER_URL);
    if (!allowed) e.preventDefault();
  });

  if (isDev) {
    // Mirror renderer console into the terminal so a headless run shows errors.
    win.webContents.on('console-message', (event) => {
      if (event.level === 'error' || event.level === 'warning') {
        console.log(`[renderer:${event.level}] ${event.message}`);
      }
    });
  }

  if (isDev && process.env.ELECTRON_RENDERER_URL) {
    void win.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    void win.loadFile(path.join(__dirname, '../renderer/index.html'));
  }
  return win;
}

// Headless self-test for CI / agents: DESK_SMOKE=status logs the connection
// status; DESK_SMOKE=query additionally runs a trivial SDK query with no MCP
// servers. The app quits by itself afterwards.
async function runSmoke(kind: string): Promise<void> {
  const cfg = await getSettings();
  console.log('[smoke] status', JSON.stringify(await getStatus(cfg)));
  if (kind === 'query') {
    const { query } = await import('@anthropic-ai/claude-agent-sdk');
    let text = '';
    for await (const m of query({
      prompt: 'Reply with exactly: pong',
      options: { systemPrompt: 'Test.', permissionMode: 'dontAsk', maxTurns: 1, settingSources: [], strictMcpConfig: true, mcpServers: {}, env: buildEnv(cfg, { apiKey: null, oauthToken: null }), includePartialMessages: true },
    })) {
      if (m.type === 'system' && m.subtype === 'init') console.log('[smoke] init', m.model, m.claude_code_version, m.apiKeySource);
      if (m.type === 'stream_event' && m.event.type === 'content_block_delta' && m.event.delta.type === 'text_delta') text += m.event.delta.text;
      if (m.type === 'result') console.log('[smoke] result', m.subtype, 'cost', m.total_cost_usd, 'text', JSON.stringify(text));
    }
  }
}

app.whenReady().then(async () => {
  await fs.mkdir(chatWorkspaceDir(), { recursive: true });
  registerIpc(() => mainWindow);
  mainWindow = createWindow();
  console.log(`[main] Strategy Desk ${app.getVersion()} ready; userData=${app.getPath('userData')}`);
  mainWindow.webContents.once('did-finish-load', () => console.log('[main] renderer loaded'));

  if (isDev && process.env.DESK_SMOKE) {
    runSmoke(process.env.DESK_SMOKE)
      .catch((e) => console.error('[smoke] error', (e as Error)?.message ?? e))
      .finally(() => setTimeout(() => app.quit(), 1500));
  }

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) mainWindow = createWindow();
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});

app.on('before-quit', () => {
  void stopAll();
});
