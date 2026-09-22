// The preflight contract: the shape and order of the list, the status mapping
// of every branch, and the port-conflict case naming the owner of the port.
//
// Everything here runs against faked facts — no process is spawned and no port
// is touched, which is the point of keeping `buildChecks` pure.
import { describe, expect, it } from 'vitest';
import type { Settings } from './api';
import { DEFAULT_SETTINGS } from './defaults';
import { buildChecks, CHECK_IDS, cdpOwner, EMPTY_CDP, resultOf, worstStatus, type CdpProbe, type CheckId, type CheckStatus, type KbFact, type PreflightFacts, type RuntimeFact } from './preflight';

const cfg: Settings = { ...DEFAULT_SETTINGS };

const tvCdp: CdpProbe = {
  up: true,
  browser: 'Chrome/128.0.6613.36',
  chartTab: true,
  tradingview: true,
  targets: [{ url: 'https://www.tradingview.com/chart/abc/', title: 'US500 · 15 · PEPPERSTONE' }],
};

/** A dev run's runtime: nothing shipped, so `uv` and the clone are the story. */
const externalRuntime: RuntimeFact = {
  mode: 'external',
  tvmcpDir: 'C:\\tvmcp',
  payloadOk: false,
  projectName: '',
  provisioned: false,
  stale: false,
  pythonPath: '',
  uvPath: '',
  lockHash: '',
  chromium: null,
};

/** A packaged install whose environment is already provisioned. */
const bundledRuntime: RuntimeFact = {
  mode: 'bundled',
  tvmcpDir: 'C:\\app\\resources\\tvmcp',
  payloadOk: true,
  projectName: 'tvmcp',
  provisioned: true,
  stale: false,
  pythonPath: 'C:\\userData\\py\\venv\\Scripts\\python.exe',
  uvPath: 'C:\\app\\resources\\uv.exe',
  lockHash: 'abc123',
  chromium: true,
};

/** The knowledge base as a working bundled install has it. */
const seededKb: KbFact = {
  mode: 'bundled',
  enabled: true,
  dir: 'C:\app\resources\gt-rag',
  payloadOk: true,
  seeded: true,
  stale: false,
  model: true,
  modelShipped: true,
  pages: 109,
};

/** A machine where literally everything is in place. */
function healthyFacts(): PreflightFacts {
  return {
    uv: { found: true, version: '0.9.7' },
    clone: { path: 'C:\\tvmcp', repo: 'https://example.invalid/tvmcp.git', dirExists: true, pyproject: 'ok', projectName: 'tvmcp' },
    runtime: { ...externalRuntime },
    kb: { ...seededKb },
    boot: { ran: true, ok: true, tools: 59, error: '' },
    claude: 'subscription',
    official: { enabled: true, registered: true, scope: 'User config', status: 'connected', url: 'https://mcp.tradingview.com/mcp', raw: 'Connected' },
    tv: { running: true, port: 9223, cdp: tvCdp },
    freePort: null,
    workflows: { loaded: ['ib-es', 'strategy-lab'], failed: [] },
    files: { riskRules: { path: 'C:\\rules.json', exists: true }, journal: { path: 'C:\\journal.csv', exists: true } },
    platform: 'win32',
  };
}

function statusOf(checks: ReturnType<typeof buildChecks>, id: CheckId): CheckStatus {
  const c = checks.find((x) => x.id === id);
  if (!c) throw new Error(`no check ${id}`);
  return c.status;
}

/** Every id that can appear, minus the ones belonging to the other runtime mode. */
function idsFor(mode: 'bundled' | 'external'): CheckId[] {
  const drop: CheckId[] = mode === 'bundled' ? ['uv', 'tvmcpClone'] : ['tvmcpRuntime', 'chromium'];
  return CHECK_IDS.filter((id) => !drop.includes(id));
}

describe('the check list', () => {
  it('shows the user toolchain in a dev run, in the declared order', () => {
    const checks = buildChecks(cfg, healthyFacts());
    expect(checks.map((c) => c.id)).toEqual(idsFor('external'));
  });

  // A packaged install owns its Python: asking the user about `uv` or about a
  // clone they never made would be asking about someone else's machine.
  it('replaces uv and the clone with one runtime check in a packaged install', () => {
    const f = healthyFacts();
    f.runtime = { ...bundledRuntime };
    const checks = buildChecks(cfg, f);
    expect(checks.map((c) => c.id)).toEqual(idsFor('bundled'));
    expect(statusOf(checks, 'tvmcpRuntime')).toBe('ok');
  });

  it('is all green on a healthy machine, and the run is ok', () => {
    const checks = buildChecks(cfg, healthyFacts());
    expect(checks.every((c) => c.status === 'ok')).toBe(true);
    expect(resultOf(checks).ok).toBe(true);
  });

  it('gives every non-ok check a way forward', () => {
    const f = healthyFacts();
    f.uv = { found: false, version: '' };
    f.claude = 'none';
    f.tv = { running: false, port: 9223, cdp: EMPTY_CDP };
    f.files.riskRules = { path: '', exists: false };
    for (const c of buildChecks(cfg, f)) {
      if (c.status === 'ok' || c.status === 'unknown') continue;
      expect(c.fix, `check ${c.id} has no fix`).toBeTruthy();
      expect(c.fix?.labelKey).toMatch(/^preflight\.fix\./);
    }
  });

  // The official MCP, the knowledge base and the two files are optional for the
  // same reason: the desk works without them, so a missing one must not paint
  // the run red.
  it('marks the optional checks optional, so they never fail the run', () => {
    const f = healthyFacts();
    f.files.riskRules = { path: 'C:\\gone.json', exists: false };
    f.files.journal = { path: 'C:\\gone.csv', exists: false };
    const checks = buildChecks(cfg, f);
    expect(checks.filter((c) => c.optional).map((c) => c.id)).toEqual(['knowledgeBase', 'kbModel', 'officialMcp', 'riskRules', 'journal']);
    expect(statusOf(checks, 'riskRules')).toBe('fail');
    expect(resultOf(checks).ok).toBe(true);
  });
});

describe('status mapping, one faked probe at a time', () => {
  it('uv: found -> ok with the version, missing -> fail with both install lines', () => {
    expect(statusOf(buildChecks(cfg, healthyFacts()), 'uv')).toBe('ok');
    const f = healthyFacts();
    f.uv = { found: false, version: '' };
    const uv = buildChecks(cfg, f).find((c) => c.id === 'uv')!;
    expect(uv.status).toBe('fail');
    expect(uv.fix?.kind).toBe('copy');
    // Windows line first on Windows, the macOS/Linux one still reachable.
    expect(uv.fix?.kind === 'copy' && uv.fix.command).toContain('install.ps1');
    expect(uv.fix?.kind === 'copy' && uv.fix.altCommand).toContain('install.sh');
    f.platform = 'darwin';
    const mac = buildChecks(cfg, f).find((c) => c.id === 'uv')!;
    expect(mac.fix?.kind === 'copy' && mac.fix.command).toContain('install.sh');
  });

  it('the clone: unset / missing / no pyproject / wrong project name each fail differently', () => {
    const cases: [Partial<PreflightFacts['clone']>, string][] = [
      [{ path: '' }, 'preflight.clone.unset'],
      [{ dirExists: false }, 'preflight.clone.missing'],
      [{ pyproject: 'missing' }, 'preflight.clone.noPyproject'],
      [{ pyproject: 'wrong-name', projectName: 'something-else' }, 'preflight.clone.wrongName'],
    ];
    for (const [patch, key] of cases) {
      const f = healthyFacts();
      f.clone = { ...f.clone, ...patch };
      const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpClone')!;
      expect(c.status, key).toBe('fail');
      expect(c.detailKey).toBe(key);
    }
  });

  it('a missing clone offers an explicit git clone command and never runs it', () => {
    const f = healthyFacts();
    f.clone = { ...f.clone, dirExists: false, repo: 'https://github.com/Miha21222/tradingview-mcp.git', path: 'C:\\Projects\\tvmcp' };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpClone')!;
    expect(c.fix).toEqual({
      kind: 'copy',
      labelKey: 'preflight.fix.copyClone',
      command: 'git clone https://github.com/Miha21222/tradingview-mcp.git "C:\\Projects\\tvmcp"',
      noteKey: 'preflight.clone.note',
    });
  });

  it('the boot check is unknown (not failed) when it was never attempted', () => {
    const f = healthyFacts();
    f.uv = { found: false, version: '' };
    f.boot = { ran: false, ok: false, tools: 0, error: '' };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpBoot')!;
    expect(c.status).toBe('unknown');
    expect(c.detailKey).toBe('preflight.boot.skipped');
  });

  it('a boot failure shows the FIRST line of stderr — where the real cause is', () => {
    const f = healthyFacts();
    f.boot = { ran: true, ok: false, tools: 0, error: "ModuleNotFoundError: No module named 'httpx'\n  at line 3\n  at line 4" };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpBoot')!;
    expect(c.status).toBe('fail');
    expect(c.vars?.error).toBe("ModuleNotFoundError: No module named 'httpx'");
  });

  it('Claude: each login route is ok, "none" fails with a copyable command, not a button', () => {
    for (const mode of ['subscription', 'api-key', 'setup-token'] as const) {
      const f = healthyFacts();
      f.claude = mode;
      const c = buildChecks(cfg, f).find((x) => x.id === 'claude')!;
      expect(c.status).toBe('ok');
      expect(c.detailKey).toBe(`preflight.claude.${mode}`);
    }
    const f = healthyFacts();
    f.claude = 'none';
    const c = buildChecks(cfg, f).find((x) => x.id === 'claude')!;
    expect(c.status).toBe('fail');
    expect(c.fix?.kind).toBe('copy');
    expect(c.fix?.kind === 'copy' && c.fix.noteKey).toBe('preflight.claude.note');
  });

  it('TradingView: not running -> the launch button; running flagless -> an explanation, never a kill', () => {
    const off = healthyFacts();
    off.tv = { running: false, port: 9223, cdp: EMPTY_CDP };
    const notRunning = buildChecks(cfg, off).find((c) => c.id === 'tvDesktop')!;
    expect(notRunning.status).toBe('fail');
    expect(notRunning.fix?.kind).toBe('launch-tv');

    const flagless = healthyFacts();
    flagless.tv = { running: true, port: 9223, cdp: EMPTY_CDP };
    const c = buildChecks(cfg, flagless).find((x) => x.id === 'tvDesktop')!;
    expect(c.status).toBe('fail');
    expect(c.detailKey).toBe('preflight.tv.flagless');
    expect(c.fix?.kind).toBe('explain');
  });

  it('TradingView up with no chart tab only warns', () => {
    const f = healthyFacts();
    f.tv.cdp = { ...tvCdp, chartTab: false, targets: [{ url: 'https://www.tradingview.com/', title: 'TradingView' }] };
    expect(statusOf(buildChecks(cfg, f), 'tvDesktop')).toBe('warn');
  });

  it('workflows: none -> fail, some broken -> warn naming them, all good -> ok', () => {
    const none = healthyFacts();
    none.workflows = { loaded: [], failed: ['ib-es'] };
    expect(statusOf(buildChecks(cfg, none), 'workflows')).toBe('fail');

    const partial = healthyFacts();
    partial.workflows = { loaded: ['strategy-lab'], failed: ['ib-es', 'v1b3-fundingpips'] };
    const c = buildChecks(cfg, partial).find((x) => x.id === 'workflows')!;
    expect(c.status).toBe('warn');
    expect(c.vars?.failed).toBe('ib-es, v1b3-fundingpips');

    expect(statusOf(buildChecks(cfg, healthyFacts()), 'workflows')).toBe('ok');
  });

  it('the optional files: unset warns, set-but-missing fails, present is ok', () => {
    const unset = healthyFacts();
    unset.files = { riskRules: { path: '', exists: false }, journal: { path: '  ', exists: false } };
    const c1 = buildChecks(cfg, unset);
    expect(statusOf(c1, 'riskRules')).toBe('warn');
    expect(statusOf(c1, 'journal')).toBe('warn');
    expect(c1.find((c) => c.id === 'riskRules')?.fix?.kind).toBe('pick-file');

    const gone = healthyFacts();
    gone.files.journal = { path: 'C:\\nope.csv', exists: false };
    expect(statusOf(buildChecks(cfg, gone), 'journal')).toBe('fail');

    expect(statusOf(buildChecks(cfg, healthyFacts()), 'journal')).toBe('ok');
  });
});

describe('the CDP port conflict', () => {
  const foreign: CdpProbe = {
    up: true,
    browser: 'Chrome/140.0.7259.5',
    chartTab: false,
    tradingview: false,
    targets: [{ url: 'https://claude.ai/code', title: 'wmux browser panel' }],
  };

  it('names what owns the port and offers to switch to a free one', () => {
    const f = healthyFacts();
    f.tv = { running: false, port: 9222, cdp: foreign };
    f.freePort = 9223;
    const c = buildChecks(cfg, f).find((x) => x.id === 'cdpPort')!;
    expect(c.status).toBe('fail');
    expect(c.detailKey).toBe('preflight.port.conflict');
    expect(c.vars?.owner).toBe('wmux browser panel — claude.ai');
    expect(c.vars?.browser).toBe('Chrome/140.0.7259.5');
    expect(c.fix).toEqual({ kind: 'set-port', labelKey: 'preflight.fix.setPort', port: 9223 });
  });

  it('falls back to an explanation when no candidate port is free', () => {
    const f = healthyFacts();
    f.tv = { running: false, port: 9222, cdp: foreign };
    f.freePort = null;
    const c = buildChecks(cfg, f).find((x) => x.id === 'cdpPort')!;
    expect(c.fix?.kind).toBe('explain');
  });

  it('a free port and a TradingView-owned port are both fine', () => {
    const free = healthyFacts();
    free.tv = { running: false, port: 9223, cdp: EMPTY_CDP };
    expect(statusOf(buildChecks(cfg, free), 'cdpPort')).toBe('ok');
    expect(statusOf(buildChecks(cfg, healthyFacts()), 'cdpPort')).toBe('ok');
  });

  it('a foreign owner on the configured port also fails the TradingView check', () => {
    const f = healthyFacts();
    f.tv = { running: true, port: 9222, cdp: foreign };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvDesktop')!;
    expect(c.status).toBe('fail');
    expect(c.detailKey).toBe('preflight.tv.otherOwner');
    expect(c.vars?.owner).toContain('wmux');
  });

  it('cdpOwner degrades gracefully: title, then host, then the browser string', () => {
    expect(cdpOwner(EMPTY_CDP)).toBe('');
    expect(cdpOwner({ ...foreign, targets: [{ url: '', title: 'Some App' }] })).toBe('Some App');
    expect(cdpOwner({ ...foreign, targets: [{ url: 'http://localhost:3000/x', title: '' }] })).toBe('localhost:3000');
    expect(cdpOwner({ ...foreign, targets: [] })).toBe('Chrome/140.0.7259.5');
  });
});

describe('worstStatus', () => {
  it('reports the worst status of a step, fail first', () => {
    const checks = buildChecks(cfg, healthyFacts());
    expect(worstStatus(checks)).toBe('ok');
    expect(worstStatus([...checks, { id: 'uv', status: 'warn', detailKey: 'x' }])).toBe('warn');
    expect(worstStatus([...checks, { id: 'uv', status: 'warn', detailKey: 'x' }, { id: 'uv', status: 'fail', detailKey: 'y' }])).toBe('fail');
    expect(worstStatus([])).toBe('ok');
  });
});

// The app's own Python environment. The button that builds it is the only
// thing on this list that installs anything, so each state has to be told
// apart: nothing built, built from an older lock, and a broken installation
// (which no button can repair).
describe('the bundled runtime', () => {
  const bundledFacts = () => {
    const f = healthyFacts();
    f.runtime = { ...bundledRuntime };
    return f;
  };

  it('offers to provision when there is no environment yet', () => {
    const f = bundledFacts();
    f.runtime = { ...bundledRuntime, provisioned: false };
    f.boot = { ran: false, ok: false, tools: 0, error: '' };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpRuntime')!;
    expect(c.status).toBe('fail');
    expect(c.fix).toMatchObject({ kind: 'run-task', task: 'provision-runtime' });
  });

  it('only warns when the environment is there but was built from another lock', () => {
    const f = bundledFacts();
    f.runtime = { ...bundledRuntime, provisioned: false, stale: true };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpRuntime')!;
    expect(c.status).toBe('warn');
    expect(c.detailKey).toBe('preflight.runtime.stale');
    expect(resultOf(buildChecks(cfg, f)).ok).toBe(true);
  });

  // A damaged installation is not the user's doing and not theirs to repair:
  // offering a button that cannot help would waste their time twice.
  it('explains a broken payload instead of offering to run something', () => {
    const f = bundledFacts();
    f.runtime = { ...bundledRuntime, payloadOk: false, projectName: '' };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpRuntime')!;
    expect(c.status).toBe('fail');
    expect(c.fix?.kind).toBe('explain');
  });

  // The browser is a separate 270 MB download that only chart images and
  // backtest reports need, so a missing one warns and never blocks.
  it('offers the browser download when the environment has none, and never fails the run', () => {
    const f = bundledFacts();
    f.runtime = { ...bundledRuntime, chromium: false };
    const c = buildChecks(cfg, f).find((x) => x.id === 'chromium')!;
    expect(c.status).toBe('warn');
    expect(c.optional).toBe(true);
    expect(c.fix).toMatchObject({ kind: 'run-task', task: 'install-chromium' });
    expect(resultOf(buildChecks(cfg, f)).ok).toBe(true);
  });

  // Nothing to install it into yet: asking about a browser before there is an
  // environment would be a second red row for one problem.
  it('says nothing about the browser until the environment exists', () => {
    const f = bundledFacts();
    f.runtime = { ...bundledRuntime, provisioned: false, chromium: null };
    expect(buildChecks(cfg, f).some((x) => x.id === 'chromium')).toBe(false);
  });

  it('sends a failed boot to the provisioning button, not to a uv command the user has no uv for', () => {
    const f = bundledFacts();
    f.boot = { ran: true, ok: false, tools: 0, error: 'ModuleNotFoundError: numba' };
    const c = buildChecks(cfg, f).find((x) => x.id === 'tvmcpBoot')!;
    expect(c.fix).toMatchObject({ kind: 'run-task', task: 'provision-runtime' });
  });
});

// The official TradingView MCP: registered in Claude Code's own store, because
// that is where its OAuth token lives.
describe('the official MCP', () => {
  const withOfficial = (patch: Partial<PreflightFacts['official']>) => {
    const f = healthyFacts();
    f.official = { ...f.official, ...patch };
    return buildChecks(cfg, f).find((x) => x.id === 'officialMcp')!;
  };

  it('offers to register it when claude has never heard of it', () => {
    const c = withOfficial({ registered: false, status: 'absent' });
    expect(c.status).toBe('warn');
    expect(c.fix).toMatchObject({ kind: 'run-task', task: 'official-mcp-add' });
  });

  it('offers to log in when it is registered but not authenticated', () => {
    const c = withOfficial({ status: 'needs-auth', raw: 'Needs authentication' });
    expect(c.fix).toMatchObject({ kind: 'run-task', task: 'official-mcp-login' });
    expect(c.vars?.raw).toBe('Needs authentication');
  });

  // Connected but switched off is the confusing one: everything on the machine
  // is fine and the chat still cannot see the tools.
  it('offers the settings switch when it works but the chat is not told to use it', () => {
    const c = withOfficial({ enabled: false });
    expect(c.fix).toEqual({ kind: 'set-flag', labelKey: 'preflight.fix.officialEnable', field: 'officialMcp', value: true, noteKey: 'preflight.official.enableNote' });
  });

  it('says "not checked" rather than "not installed" when claude could not be asked', () => {
    const c = withOfficial({ status: 'unknown', registered: false });
    expect(c.status).toBe('unknown');
    expect(c.fix).toBeUndefined();
  });

  it('never fails the run, whatever state it is in', () => {
    for (const status of ['absent', 'needs-auth', 'error', 'unknown'] as const) {
      const f = healthyFacts();
      f.official = { ...f.official, status, registered: status !== 'absent' };
      expect(resultOf(buildChecks(cfg, f)).ok, status).toBe(true);
    }
  });
});
