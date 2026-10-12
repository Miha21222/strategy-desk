// window.desk shim for looking at the renderer in a plain browser (screenshots).
(function () {
  const noop = () => {};
  const ok = (v) => Promise.resolve(v);
  const json = (u) => fetch(u).then((r) => (r.ok ? r.json() : null)).catch(() => null);
  let defaults = { settings: {}, profile: {} };
  const settingsPatch = JSON.parse(sessionStorage.getItem('mock.settings') || '{}');
  const chats = [];
  const changed = [];
  const written = {}; // human reads / answers written in this session, merged into the day
  const api = {
    settings: {
      get: async () => { await window.__mockReady; return ({ ...(defaults.settings || {}), setupCompletedAt: 1, vaultPath: 'C:\\vault', ...settingsPatch }); },
      set: async (p) => { Object.assign(settingsPatch, p); sessionStorage.setItem('mock.settings', JSON.stringify(settingsPatch)); return api.settings.get(); },
      setSecret: async () => true, clearSecret: async () => true,
    },
    profile: { get: async () => { await window.__mockReady; return defaults.profile || {}; }, set: async (p) => p },
    workflows: { list: async () => [] },
    chats: { list: async () => chats, create: async (kind, wf) => { const c = { key: 'c' + chats.length, sessionId: null, title: kind === 'macro' ? 'Обзор дня' : 'Чат', createdAt: Date.now(), updatedAt: Date.now(), pinned: false, kind, tokens: { in: 0, out: 0 }, costUsd: 0, workflowId: wf }; chats.push(c); return c; },
      open: async (key) => ({ meta: chats.find((c) => c.key === key), messages: [] }), rename: async () => {}, setKind: async () => {}, pin: async () => {}, delete: async () => {}, addReport: async () => {}, deleteReport: async () => {} },
    chat: { send: noop, stop: noop, prewarm: noop, onEvent: () => noop },
    status: { get: async () => ({ tvRunning: true, cdpUp: true, chartTab: true, cdpOwner: '', tvmcpOk: true, tvmcpTools: 59, tvmcpError: '', claude: { ok: true, method: 'subscription', detail: '' }, checkedAt: Date.now() }) },
    preflight: { run: async () => ({ checks: [], mode: 'external' }) },
    tasks: { list: async () => [], command: async () => '', run: async () => ({ ok: true }), stop: noop, onEvent: () => noop },
    ui: { zoom: async (f) => f }, open: { external: async () => true }, app: { version: async () => '0.1.0-mock' },
    dialog: { pick: async () => null }, tv: { launch: async () => ({ ok: true }), call: async () => ({}) }, rules: { get: async () => null }, runtime: { clear: async () => true },
    macro: {
      index: () => json('data/index.json'),
      day: async (d) => { const day = await json(`data/day_${d}.json`); if (!day) return null; const w = written[d] || {}; if (w.eu) day.eu.human = w.eu; if (w.us) day.us.human = w.us; if (w.answers) day.answers = w.answers; return day; },
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
