import { app, BrowserWindow, shell } from 'electron';
import { existsSync, promises as fs, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { BLOCK_KINDS } from '@shared/blocks';
import { sampleAssistantMessage } from '@shared/blocks-sample';
import { adrFromScan, buildSentinelSpec, levelsArgs, levelsFromScan, todayISO } from '@shared/panel';
import { parseBlocks } from '../renderer/chat/blocks';
import { buildEnv, stopAll } from './agent';
import { registerIpc } from './ipc';
import { runPreflight } from './preflight';
import { assemblePrompt, loadBasePrompt, loadFieldNotes } from './prompt';
import { resourceRoots } from './resources';
import { getPropFirms } from './rules';
import { getStatus } from './status';
import { chatWorkspaceDir, getSettings } from './store';
import { callTool } from './tv-cli';
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

/**
 * The packaging self-test: does an INSTALLED build still find everything it
 * ships with? In dev every resource happens to sit in the working directory,
 * which hides a missing `files:` entry in electron-builder.yml completely — so
 * this runs against the packaged binary (`npm run dist:check`), with the
 * project directory renamed away, and prints one `[pack] …` line per resource.
 */
async function runPackSmoke(): Promise<void> {
  const problems: string[] = [];
  console.log('[pack] app', app.getVersion(), `packaged=${app.isPackaged}`);
  console.log('[pack] anchors', JSON.stringify({ cwd: process.cwd(), appPath: app.getAppPath(), resourcesPath: process.resourcesPath }));
  console.log('[pack] roots', JSON.stringify(resourceRoots()));

  const workflows = await listWorkflows();
  console.log('[pack] workflows', workflows.length, workflows.map((w) => `${w.id}${w.panel ? `[${w.panel.kind}]` : ''}:${w.prompt.length}c`).join(' ') || 'NONE');
  if (!workflows.length) problems.push('no workflows');
  if (workflows.some((w) => !w.prompt)) problems.push('a workflow lost its prompt block');

  const firms = await getPropFirms();
  const accounts = firms ? firms.firms.reduce((n, f) => n + f.accounts.length, 0) : 0;
  console.log('[pack] rules', firms ? `${firms.firms.length} firms, ${accounts} accounts, updated ${firms.updated}` : 'NOT FOUND');
  if (!firms || !firms.firms.length) problems.push('no prop-firm rule book');

  const base = await loadBasePrompt();
  const notes = await loadFieldNotes();
  console.log('[pack] prompt', `${base.length} chars`, `notes:${notes ? `${notes.length} chars` : 'none'}`);
  if (base.length < 400) problems.push('the base system prompt fell back to the built-in stub');

  // The Agent SDK resolves the native Claude binary with require.resolve() from
  // its OWN location, so it must sit next to the SDK in app.asar.unpacked.
  const exe = process.platform === 'win32' ? 'claude.exe' : 'claude';
  const spec = `@anthropic-ai/claude-agent-sdk-${process.platform}-${process.arch}/${exe}`;
  let claudePath: string | null = null;
  try {
    claudePath = createRequire(require.resolve('@anthropic-ai/claude-agent-sdk')).resolve(spec);
    if (!existsSync(claudePath)) claudePath = null;
  } catch {
    claudePath = null;
  }
  console.log('[pack] claude', claudePath ? `${claudePath} (${Math.round(statSync(claudePath).size / 1048576)} MB)` : `NOT FOUND (${spec})`);
  if (!claudePath) problems.push('the Claude binary is not reachable from the packaged SDK');

  if (problems.length) {
    console.log('[pack] FAILED', problems.join('; '));
    throw new Error(problems.join('; '));
  }
  console.log('[pack] ok');
}

// Headless self-test for CI / agents: DESK_SMOKE=status logs the connection
// status; DESK_SMOKE=query additionally runs a trivial SDK query with no MCP
// servers; DESK_SMOKE=blocks assembles the prompt, loads the workflows and
// feeds a canned assistant message with one of every block through the
// parser, failing on any schema error; DESK_SMOKE=preflight prints the real
// setup check list (id, status, detail key, fix kind); DESK_SMOKE=panel walks the day panel's
// own deterministic path (levels -> sentinel spec) and checks the tv:call
// allowlist refuses an unlisted tool; DESK_SMOKE=pack checks that an INSTALLED
// build still finds everything it ships with — this is what `npm run dist:check`
// runs against the packaged binary. The app quits by itself afterwards.
async function runSmoke(kind: string): Promise<void> {
  if (kind === 'pack') return runPackSmoke();
  const cfg = await getSettings();
  console.log('[smoke] status', JSON.stringify(await getStatus(cfg)));
  if (kind === 'preflight') {
    // The real check list on this machine, in the order the wizard shows it.
    const r = await runPreflight(cfg);
    console.log(`[smoke] preflight ok=${r.ok} checks=${r.checks.length}`);
    for (const c of r.checks) {
      const vars = c.vars ? ` ${JSON.stringify(c.vars)}` : '';
      const fix = c.fix ? ` fix=${c.fix.kind}${c.fix.kind === 'copy' ? ` «${c.fix.command}»` : c.fix.kind === 'set-port' ? ` ${c.fix.port}` : ''}` : '';
      console.log(`[smoke] ${c.status.toUpperCase().padEnd(7)} ${c.id.padEnd(11)} ${c.detailKey}${vars}${fix}`);
    }
    return;
  }
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
  if (kind === 'panel') {
    // The day panel's own path, without a window: load the workflows, build the
    // sentinel spec the way the renderer does, run the deterministic calls.
    for (const w of await listWorkflows()) {
      if (!w.panel) continue;
      console.log(`[smoke] panel ${w.id} kind=${w.panel.kind} askModelOn=${w.panel.askModelOn.join(',') || 'none'}`);
      if (!w.panel.levels) continue;
      const day = todayISO();
      const scan = await callTool(cfg, 'tv_scan_levels', levelsArgs(w.panel, day) ?? {});
      if (!scan.ok) {
        console.log(`[smoke] panel ${w.id} levels FAILED ${scan.error}`);
        continue;
      }
      const levels = levelsFromScan(scan.data, w.panel.levelNames);
      const spec = buildSentinelSpec(w.panel, { day, levels, adr: adrFromScan(scan.data) });
      console.log(`[smoke] panel ${w.id} levels ${levels.length} adr ${adrFromScan(scan.data) ?? '—'} spec ${JSON.stringify(spec).length} chars in ${scan.ms} ms`);
    }
    const refused = await callTool(cfg, 'tv_desktop_draw', {});
    console.log('[smoke] allowlist', refused.ok ? 'LEAKED' : 'ok');
    if (refused.ok) throw new Error('the allowlist let an unlisted tool through');
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

  // Packaged too: DESK_SMOKE=pack is the only way to prove an INSTALLED build
  // still finds its workflows and its rule book. The variable has to be set on
  // purpose, and nothing here is reachable without it.
  if (process.env.DESK_SMOKE) {
    runSmoke(process.env.DESK_SMOKE)
      .catch((e) => {
        console.error('[smoke] error', (e as Error)?.message ?? e);
        process.exitCode = 1;
      })
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
