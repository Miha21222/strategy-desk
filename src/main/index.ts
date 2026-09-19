import { app, BrowserWindow, shell } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { stopAll } from './agent';
import { registerIpc } from './ipc';
import { chatWorkspaceDir } from './store';

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

app.whenReady().then(async () => {
  await fs.mkdir(chatWorkspaceDir(), { recursive: true });
  registerIpc(() => mainWindow);
  mainWindow = createWindow();
  console.log(`[main] Strategy Desk ${app.getVersion()} ready; userData=${app.getPath('userData')}`);

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
