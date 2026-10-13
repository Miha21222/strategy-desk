// window.desk shim for looking at the renderer in a plain browser (screenshots).
//
// Query parameters (read once, at load) pick the scene:
//   ?wizard=1            settings.setupCompletedAt is unset, so the setup wizard opens by itself
//   ?pf=ok|fresh|session-empty|schedule-none
//                        which fake preflight result `preflight.run` answers with (default: ok)
//   ?fresh=1             the owner has not answered yet: human reads and day answers are stripped
//                        from every day, so a day the demo already answered starts with the question
(function () {
  const noop = () => {};
  const ok = (v) => Promise.resolve(v);
  const json = (u) => fetch(u).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  const q = new URLSearchParams(location.search);
  const wizard = q.get('wizard') === '1';
  const freshDays = q.get('fresh') === '1';
  const pfVariant = q.get('pf') || 'ok';
  let defaults = { settings: {}, profile: {} };
  const settingsPatch = JSON.parse(sessionStorage.getItem('mock.settings') || '{}');
  const chats = [];
  const changed = [];
  const written = {}; // human reads / answers written in this session, merged into the day

  // ---------- the fake diagnosis: the MVP's check list, shapes copied from src/shared/preflight.ts ----------
  const VAULT = 'C:\\Users\\trader\\Documents\\Strategy Desk';
  const PY = 'C:\\Users\\trader\\AppData\\Roaming\\strategy-desk\\py\\venv\\Scripts\\python.exe';
  const sessionOk = pfVariant === 'ok' || pfVariant === 'schedule-none';
  const vaultOk = pfVariant !== 'fresh';
  function checks() {
    const list = [];
    list.push(
      vaultOk
        ? { id: 'vault', status: 'ok', detailKey: 'preflight.vault.okTemplate', vars: { path: VAULT, version: '2' } }
        : { id: 'vault', status: 'fail', detailKey: 'preflight.vault.missing', vars: { path: VAULT }, fix: { kind: 'run-task', labelKey: 'preflight.fix.vaultCreate', task: 'vault-create', noteKey: 'preflight.vault.createNote' } },
    );
    list.push(
      pfVariant === 'fresh'
        ? { id: 'claude', status: 'fail', detailKey: 'preflight.claude.none', fix: { kind: 'run-task', labelKey: 'preflight.fix.claudeLogin', task: 'claude-login', noteKey: 'preflight.claude.note' } }
        : { id: 'claude', status: 'ok', detailKey: 'preflight.claude.subscription' },
    );
    if (pfVariant === 'fresh') {
      list.push({ id: 'tvmcpRuntime', status: 'fail', detailKey: 'preflight.runtime.missing', vars: { path: 'C:\\Program Files\\Strategy Desk\\resources\\tvmcp' }, fix: { kind: 'run-task', labelKey: 'preflight.fix.provision', task: 'provision-runtime', noteKey: 'preflight.runtime.note' } });
      list.push({ id: 'tvmcpBoot', status: 'unknown', detailKey: 'preflight.boot.skipped' });
      list.push({ id: 'toolsPython', status: 'fail', detailKey: 'preflight.toolsPython.missing', vars: { python: PY }, fix: { kind: 'run-task', labelKey: 'preflight.fix.provision', task: 'provision-runtime', noteKey: 'preflight.runtime.note' } });
    } else {
      list.push({ id: 'tvmcpRuntime', status: 'ok', detailKey: 'preflight.runtime.ok', vars: { python: PY, lock: '3f9c1a2b' } });
      list.push({ id: 'tvmcpBoot', status: 'ok', detailKey: 'preflight.boot.ok', vars: { tools: 59, toolsets: 'default' } });
      list.push({ id: 'chromium', status: 'ok', optional: true, detailKey: 'preflight.chromium.ok' });
      list.push({ id: 'toolsPython', status: 'ok', detailKey: 'preflight.toolsPython.ok', vars: { python: PY } });
    }
    if (pfVariant === 'fresh') {
      list.push({ id: 'tvDesktop', status: 'fail', detailKey: 'preflight.tv.notRunning', fix: { kind: 'launch-tv', labelKey: 'preflight.fix.launchTv' } });
      list.push({ id: 'cdpPort', status: 'ok', detailKey: 'preflight.port.free', vars: { port: 9222 } });
    } else {
      list.push({ id: 'tvDesktop', status: 'ok', detailKey: 'preflight.tv.ok', vars: { port: 9222 } });
      list.push({ id: 'cdpPort', status: 'ok', detailKey: 'preflight.port.tv', vars: { port: 9222 } });
    }
    list.push(
      sessionOk
        ? { id: 'tvSession', status: 'ok', detailKey: 'preflight.tvSession.ok', vars: { time: '13.10.2026, 09:12' } }
        : { id: 'tvSession', status: 'fail', detailKey: 'preflight.tvSession.none', fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.tvSession.howNote' } },
    );
    const register = { kind: 'run-task', labelKey: 'preflight.fix.schedulerRegister', task: 'scheduler-register', noteKey: 'preflight.scheduler.registerNote' };
    if (pfVariant === 'ok' || pfVariant === 'session-empty') list.push({ id: 'scheduler', status: 'ok', detailKey: 'preflight.scheduler.ok' });
    else if (pfVariant === 'fresh') list.push({ id: 'scheduler', status: 'warn', detailKey: 'preflight.scheduler.noVault' });
    else list.push({ id: 'scheduler', status: 'warn', detailKey: 'preflight.scheduler.none', fix: register });
    list.push({ id: 'timezone', status: 'ok', detailKey: 'preflight.timezone.ok', vars: { tz: 'Europe/Kiev' } });
    return list;
  }
  const COMMANDS = {
    'provision-runtime': 'uv.exe sync --frozen --no-dev --extra kb (в %APPDATA%\\strategy-desk\\py\\venv)',
    'install-chromium': 'python.exe -m playwright install chromium',
    // vault-create is a copy plan: taskCommand() spells it as `copy "<from>" -> "<to>"; ...`, one per template file.
    'vault-create': ['.claude\\skills\\macro-review\\SKILL.md', 'indices-kb\\tools\\scale.py', 'indices-kb\\config\\scale.json'].map((f) => 'copy "C:\\Program Files\\Strategy Desk\\resources\\vault-template\\' + f + '" -> "' + VAULT + '\\' + f + '"').join('; '),
    'session-check': 'python.exe -c "fetch_auth_token(sessionid)"',
    'scheduler-register': 'powershell -NoProfile -File indices-kb\\tools\\macro_schedule.ps1 -Register -Vault "' + VAULT + '"',
    'scheduler-unregister': 'powershell -NoProfile -File indices-kb\\tools\\macro_schedule.ps1 -Unregister',
    'claude-login': 'claude.exe auth login --claudeai',
  };

  // The mock keeps no day in the file: when the scene says «not answered yet», the demo's own reads go.
  function stripAnswers(day) {
    if (!day) return day;
    day.eu.human = null;
    day.us.human = null;
    day.answers = null;
    return day;
  }

  const api = {
    settings: {
      get: async () => {
        await window.__mockReady;
        const base = { ...(defaults.settings || {}), vaultPath: VAULT, hasTvSession: sessionOk, tvSessionVerifiedAt: sessionOk ? Date.now() : undefined };
        if (!wizard) base.setupCompletedAt = 1;
        return { ...base, ...settingsPatch };
      },
      set: async (p) => { Object.assign(settingsPatch, p); sessionStorage.setItem('mock.settings', JSON.stringify(settingsPatch)); return api.settings.get(); },
      setSecret: async () => true, clearSecret: async () => true,
    },
    profile: { get: async () => { await window.__mockReady; return defaults.profile || {}; }, set: async (p) => p },
    workflows: { list: async () => [] },
    chats: { list: async () => chats, create: async (kind, wf) => { const c = { key: 'c' + chats.length, sessionId: null, title: kind === 'macro' ? 'Обзор дня' : 'Чат', createdAt: Date.now(), updatedAt: Date.now(), pinned: false, kind, tokens: { in: 0, out: 0 }, costUsd: 0, workflowId: wf }; chats.push(c); return c; },
      open: async (key) => ({ meta: chats.find((c) => c.key === key), messages: [] }), rename: async () => {}, setKind: async () => {}, pin: async () => {}, delete: async () => {}, addReport: async () => {}, deleteReport: async () => {} },
    chat: { send: noop, stop: noop, prewarm: noop, onEvent: () => noop },
    status: { get: async () => ({ tvRunning: true, cdpUp: true, chartTab: true, cdpOwner: '', tvmcpOk: true, tvmcpTools: 59, tvmcpError: '', claude: { ok: true, method: 'subscription', detail: '' }, checkedAt: Date.now() }) },
    preflight: { run: async () => { const list = checks(); return { checks: list, at: Date.now(), ok: !list.some((c) => c.status === 'fail' && !c.optional) }; } },
    tasks: { list: async () => [], command: async (id) => COMMANDS[id] || '', run: async (id) => ({ id, running: false, ok: true, error: '', lines: [], startedAt: Date.now(), finishedAt: Date.now() }), stop: noop, input: noop, onEvent: () => noop },
    ui: { zoom: async (f) => f }, open: { external: async () => true }, app: { version: async () => '0.2.0-mock' },
    dialog: { pick: async () => null }, tv: { launch: async () => ({ ok: true }), call: async () => ({}) }, rules: { get: async () => null }, runtime: { clear: async () => true },
    macro: {
      index: () => json('data/index.json'),
      day: async (d) => { let day = await json(`data/day_${d}.json`); if (!day) return null; if (freshDays) day = stripAnswers(day); const w = written[d] || {}; if (w.eu) day.eu.human = w.eu; if (w.us) day.us.human = w.us; if (w.answers) day.answers = w.answers; return day; },
      openNote: async () => true,
      image: async (rel) => { try { const b = await fetch('vault/' + rel).then((r) => (r.ok ? r.blob() : null)); if (!b) return null; return await new Promise((res) => { const fr = new FileReader(); fr.onload = () => res(fr.result); fr.readAsDataURL(b); }); } catch { return null; } },
      humanRead: async ({ date, region, side, beforeScale }) => { const rec = { date, region, side, at: date + ' 09:31', before_scale: beforeScale }; (written[date] ||= {})[region] = rec; changed.forEach((cb) => cb([date])); return rec; },
      dayAnswers: async ({ date, regions, note }) => { const rec = { date, at: date + ' 20:05', regions, note }; (written[date] ||= {}).answers = rec; changed.forEach((cb) => cb([date])); return rec; },
      onChanged: (cb) => { changed.push(cb); return () => { const i = changed.indexOf(cb); if (i >= 0) changed.splice(i, 1); }; },
    },
  };
  window.desk = api;
  window.__mockReady = fetch('data/defaults.json').then((r) => r.json()).then((d) => { defaults = d; });
})();
