import { app, BrowserWindow, shell } from 'electron';
import { existsSync, promises as fs, statSync } from 'node:fs';
import { execFile } from 'node:child_process';
import { createRequire } from 'node:module';
import path from 'node:path';
import { BLOCK_KINDS } from '@shared/blocks';
import { sampleAssistantMessage } from '@shared/blocks-sample';
import { adrFromScan, buildSentinelSpec, levelsArgs, levelsFromScan, todayISO } from '@shared/panel';
import { parseBlocks } from '../renderer/chat/blocks';
import { buildEnv, stopAll } from './agent';
import { registerIpc } from './ipc';
import { buildMcpServers } from './mcp';
import { runPreflight } from './preflight';
import { assemblePrompt, loadBasePrompt, loadFieldNotes } from './prompt';
import { resourceRoots } from './resources';
import { getPropFirms } from './rules';
import { initRuntimePaths, kbFact, runtimeFact } from './runtime';
import { kbCheckSpec, modeOf } from './runtime-spec';
import { runTask, taskCommand } from './tasks';
import { getStatus, tvmcpStatus } from './status';
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
  else {
    // Resolving it is not enough: the path points INTO app.asar and only works
    // because Electron redirects it to app.asar.unpacked when spawning. Prove
    // that by actually running it — offline, no tokens.
    const version = await new Promise<string>((done) => {
      execFile(claudePath as string, ['--version'], { timeout: 30_000, windowsHide: true }, (err, stdout) => done(err ? `FAILED ${err.message.slice(0, 120)}` : stdout.trim()));
    });
    console.log('[pack] claude --version', version || 'no output');
    if (!version || version.startsWith('FAILED')) problems.push('the Claude binary resolves but will not launch from the packaged location');
  }

  // The bundled tvmcp. This is the check that catches a missing
  // `extraResources:` entry: in a dev run there is no payload at all and the
  // app correctly falls back to the developer's checkout, so nothing else here
  // would notice that an INSTALLED build shipped without one.
  const paths = await initRuntimePaths();
  const cfg = await getSettings();
  const runtime = await runtimeFact(cfg, paths);
  console.log(
    '[pack] tvmcp',
    JSON.stringify({ mode: runtime.mode, dir: runtime.tvmcpDir, payloadOk: runtime.payloadOk, uv: runtime.uvPath, lock: runtime.lockHash, provisioned: runtime.provisioned }),
  );
  if (runtime.mode !== 'bundled') problems.push('the packaged app shipped no tvmcp payload (extraResources)');
  else if (!runtime.payloadOk) problems.push('the tvmcp payload is there but incomplete (pyproject/uv.lock)');
  else if (!runtime.uvPath) problems.push('the tvmcp payload shipped without a uv binary');
  // NOT a failure: the environment is provisioned on the user's machine, from a
  // button. A packaged build that has never been run has none, and must not.

  // The knowledge base, for the same reason: a dev run finds no payload and
  // falls back to a clone, so only a packaged build can say whether the course
  // shipped. An installer built with SD_BUNDLE_KB=0 is a choice, not a failure
  // — a payload carrying HALF the knowledge base is a failure.
  const kb = await kbFact(cfg, paths);
  console.log('[pack] gt-rag', JSON.stringify({ mode: kb.mode, dir: kb.dir, payloadOk: kb.payloadOk, pages: kb.pages, model: kb.modelShipped }));
  if (kb.mode === 'bundled' && !kb.payloadOk) problems.push('the gt-rag payload is incomplete (server.py / raw / store)');

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
// allowlist refuses an unlisted tool; DESK_SMOKE=runtime provisions the bundled
// Python environment for real and boots tvmcp out of it (minutes, ~600 MB, and
// it needs a payload — see CLAUDE.md for how to fake one in a dev run);
// DESK_SMOKE=kb seeds the course knowledge base and asks the store how many
// chunks it holds, with the environment a chat would give it;
// DESK_SMOKE=pack checks that an INSTALLED
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
  if (kind === 'runtime') {
    // The whole bundled path end to end, which no unit test can reach: resolve
    // the payload, run the real `uv sync` through the task runner, then boot
    // tvmcp with the interpreter it just built.
    const paths = await initRuntimePaths();
    console.log('[smoke] runtime mode', modeOf(paths), paths.payloadDir ?? '');
    if (modeOf(paths) !== 'bundled') throw new Error('no bundled payload: nothing to provision');
    console.log('[smoke] runtime command', await taskCommand(cfg, 'provision-runtime'));
    const before = await runtimeFact(cfg, paths);
    console.log('[smoke] runtime before', JSON.stringify({ payloadOk: before.payloadOk, provisioned: before.provisioned, stale: before.stale }));
    const state = await runTask(cfg, 'provision-runtime', () => {});
    console.log('[smoke] runtime task', state.ok ? 'ok' : `FAILED ${state.error}`, `${state.lines.length} lines`);
    for (const line of state.lines.slice(-5)) console.log(`[smoke]   ${line}`);
    if (!state.ok) throw new Error(`provisioning failed: ${state.error}`);
    const after = await runtimeFact(cfg, paths);
    console.log('[smoke] runtime after', JSON.stringify({ provisioned: after.provisioned, python: after.pythonPath, lock: after.lockHash }));
    if (!after.provisioned) throw new Error('provisioning reported success but the marker does not match');
    const boot = await tvmcpStatus(cfg);
    console.log('[smoke] runtime boot', boot.ok ? `${boot.tools} tools` : `FAILED ${boot.error}`);
    if (!boot.ok) throw new Error(`the provisioned runtime does not boot: ${boot.error}`);
    return;
  }
  if (kind === 'kb') {
    // The knowledge base end to end, the half no unit test can reach: resolve
    // the payload, seed the store into userData through the real task, then
    // import the server with the same environment a chat would give it and ask
    // the store how much it holds. Needs a provisioned runtime with the `kb`
    // extra (DESK_SMOKE=runtime first) or a clone in `gtRagPath`.
    const paths = await initRuntimePaths();
    const before = await kbFact(cfg, paths);
    console.log('[smoke] kb mode', before.mode, before.dir || '—');
    if (before.mode === 'off') throw new Error('no knowledge base: neither a payload nor a gtRagPath');
    console.log('[smoke] kb before', JSON.stringify({ payloadOk: before.payloadOk, pages: before.pages, seeded: before.seeded, stale: before.stale, model: before.model, modelShipped: before.modelShipped }));
    if (!before.payloadOk) throw new Error('the knowledge base payload is incomplete');

    if (before.mode === 'bundled' && !before.seeded) {
      console.log('[smoke] kb seed command', await taskCommand(cfg, 'kb-import'));
      const seed = await runTask(cfg, 'kb-import', () => {});
      console.log('[smoke] kb seed', seed.ok ? 'ok' : `FAILED ${seed.error}`);
      for (const line of seed.lines.slice(-4)) console.log(`[smoke]   ${line}`);
      if (!seed.ok) throw new Error(`seeding failed: ${seed.error}`);
    }
    const after = await kbFact(cfg, paths);
    console.log('[smoke] kb after', JSON.stringify({ seeded: after.seeded, model: after.model }));

    const probe = kbCheckSpec(cfg, paths);
    if (!probe) throw new Error('no spawn spec for the knowledge base');
    console.log('[smoke] kb probe', [probe.spec.command, ...probe.spec.args].join(' ').slice(0, 160));
    const out = await new Promise<string>((done) => {
      execFile(probe.spec.command, probe.spec.args, { timeout: 10 * 60_000, windowsHide: true, env: { ...process.env, ...probe.env }, maxBuffer: 1024 * 1024 }, (err, stdout, stderr) =>
        done(err ? `FAILED ${(stderr || err.message).trim().split('\n').slice(-2).join(' | ').slice(0, 300)}` : stdout.trim()),
      );
    });
    console.log('[smoke] kb store', out || 'no output');
    if (!out || out.startsWith('FAILED')) throw new Error(`the knowledge base does not answer: ${out}`);

    // Last link: the map the Agent SDK is actually handed. A store that answers
    // but a server the chat never receives is the failure this catches.
    const servers = buildMcpServers(cfg);
    console.log('[smoke] kb servers', Object.keys(servers).join(', '));
    if (!servers['gt-database']) throw new Error('the knowledge base is usable but not registered for the chat');
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
  // Work out where tvmcp comes from before anything can spawn it: the shipped
  // payload plus the provisioned environment in a packaged install, `uv` and
  // the configured checkout in a dev run. Read-only — it resolves paths, it
  // never creates the environment; that is a task, and a task needs a press.
  const runtime = await initRuntimePaths();
  console.log(`[main] tvmcp runtime: ${modeOf(runtime)}${runtime.payloadDir ? ` (${runtime.payloadDir})` : ''}`);
  registerIpc(() => mainWindow);
  mainWindow = createWindow();
  console.log(`[main] Strategy Desk ${app.getVersion()} ready; userData=${app.getPath('userData')}`);
  // `dist:check` asserts on this line, so a smoke run must not quit before it:
  // in a packaged build the renderer finishes loading well after a short smoke
  // is done, and the check was failing on the race rather than on a real
  // packaging problem.
  const rendererLoaded = new Promise<void>((resolve) => {
    mainWindow?.webContents.once('did-finish-load', () => {
      console.log('[main] renderer loaded');
      resolve();
    });
  });

  // Packaged too: DESK_SMOKE=pack is the only way to prove an INSTALLED build
  // still finds its workflows and its rule book. The variable has to be set on
  // purpose, and nothing here is reachable without it.
  if (process.env.DESK_SMOKE) {
    runSmoke(process.env.DESK_SMOKE)
      .catch((e) => {
        console.error('[smoke] error', (e as Error)?.message ?? e);
        process.exitCode = 1;
      })
      .finally(async () => {
        // Give the window its own chance to finish; a renderer that never loads
        // is a real failure, so the cap is generous and the quit unconditional.
        await Promise.race([rendererLoaded, new Promise((r) => setTimeout(r, 30_000))]);
        setTimeout(() => app.quit(), 500);
      });
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
