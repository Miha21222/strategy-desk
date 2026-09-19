import { app, BrowserWindow, shell } from 'electron';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { BLOCK_KINDS } from '@shared/blocks';
import { sampleAssistantMessage } from '@shared/blocks-sample';
import { parseBlocks } from '../renderer/chat/blocks';
import { buildEnv, stopAll } from './agent';
import { registerIpc } from './ipc';
import { assemblePrompt } from './prompt';
import { getStatus } from './status';
import { chatWorkspaceDir, getSettings } from './store';
import { listWorkflows } from './workflows';

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
// servers; DESK_SMOKE=blocks assembles the prompt, loads the workflows and
// feeds a canned assistant message with one of every block through the
// parser, failing on any schema error. The app quits by itself afterwards.
async function runSmoke(kind: string): Promise<void> {
  const cfg = await getSettings();
  console.log('[smoke] status', JSON.stringify(await getStatus(cfg)));
  if (kind === 'blocks') {
    const workflows = await listWorkflows();
    console.log('[smoke] workflows', workflows.map((w) => `${w.id} (${w.quickPrompts.length} quick, ${w.reports.length} reports, prompt ${w.prompt.length} chars)`).join('; ') || 'none');
    const prompt = await assemblePrompt({ callName: 'Smoke', traderType: '', markets: '', style: '', riskRules: '', propFirms: '', morningChecklist: [] }, workflows[0] ?? null);
    const has = (s: string) => (prompt.text.includes(s) ? 'yes' : 'no');
    console.log('[smoke] prompt', prompt.hash, `${prompt.text.length} chars`, `notes:${has('## Live field notes')}`, `workflow:${has('## Workflow:')}`, `blocks:${has('## Blocks')}`);
    const segs = parseBlocks(sampleAssistantMessage());
    const cards = segs.filter((s) => s.type === 'card');
    const errors = cards.flatMap((s) => (s.type === 'card' && s.parsed && !s.parsed.ok ? [`${s.lang}: ${s.parsed.error}`] : []));
    console.log('[smoke] blocks', `${cards.length}/${BLOCK_KINDS.length} cards`, errors.length ? `ERRORS ${errors.join(' | ')}` : 'ok');
    if (errors.length || cards.length !== BLOCK_KINDS.length) throw new Error('block smoke failed');
    return;
  }
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
