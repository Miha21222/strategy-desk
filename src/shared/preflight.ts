// The preflight contract: what a check looks like, what a fix may be, and the
// pure mapping from gathered facts to the ordered check list.
//
// Electron-free on purpose, like `panel.ts`: main builds the list, the renderer
// renders it, vitest feeds it faked facts. The probes themselves live in
// `src/main/preflight.ts`.
//
// Two rules the whole feature keeps:
//   * the app NEVER acts on its own initiative, and never hides what it will
//     run. Installing a toolchain, cloning into the user's folders, logging in
//     to Claude or closing their TradingView stays copyable text. What the app
//     itself OWNS — the Python environment it ships and provisions inside its
//     own userData, and the registration of the official TradingView MCP — is a
//     button (a `run-task` fix): one press, the command shown, the output
//     streamed. Nothing here ever starts by itself at launch;
//   * nothing is cached. "Проверить заново" means a real re-probe.
import type { ClaudeAuthStatus, Settings } from './api';
import type { OfficialMcpFact } from './official-mcp';
import type { TaskId } from './tasks';

export const CHECK_IDS = [
  'uv',
  'tvmcpClone',
  'tvmcpRuntime',
  'tvmcpBoot',
  'chromium',
  'knowledgeBase',
  'kbModel',
  'claude',
  'officialMcp',
  'codex',
  'codexAuth',
  'codexOfficialMcp',
  'tvDesktop',
  'cdpPort',
  'workflows',
  'riskRules',
  'journal',
] as const;
export type CheckId = (typeof CHECK_IDS)[number];

export type CheckStatus = 'ok' | 'warn' | 'fail' | 'unknown';

/** Settings fields a `pick-file` fix may write. */
export type PathField = 'tvmcpPath' | 'gtRagPath' | 'riskRulesPath' | 'journalPath' | 'fieldNotesPath';

/**
 * What the UI may offer for a broken check.
 *
 * `copy` is the honest answer wherever the app must not act for the user:
 * installing a toolchain, cloning a repository, logging in to Claude (the
 * SDK's login is an interactive terminal flow), closing their TradingView.
 * Everything else is something the app can do safely with one press.
 *
 * `labelKey` / `noteKey` are i18n keys, not text: UI strings live in
 * `src/renderer/i18n/ru.ts` (CLAUDE.md), so the main process holds no Russian.
 */
export type PreflightFix =
  | { kind: 'copy'; labelKey: string; command: string; altCommand?: string; noteKey?: string }
  | { kind: 'launch-tv'; labelKey: string }
  | { kind: 'set-port'; labelKey: string; port: number }
  | { kind: 'pick-file'; labelKey: string; field: PathField; directory?: boolean }
  | { kind: 'explain'; labelKey: string; noteKey: string }
  // Something the app owns and may do, once asked: provision its own Python
  // environment, fetch the headless browser, register or authenticate the
  // official MCP. The renderer sends the id; the main process owns the command.
  | { kind: 'run-task'; labelKey: string; task: TaskId; noteKey?: string }
  // A settings switch the diagnosis can flip, because the check just proved the
  // thing behind it works.
  | { kind: 'set-flag'; labelKey: string; field: FlagField; value: boolean; noteKey?: string };

/** Settings booleans a `set-flag` fix may write. */
export type FlagField = 'officialMcp' | 'knowledgeBase';

/**
 * The Codex engine, as the app sees it. `home` is the app's OWN CODEX_HOME:
 * the engine never runs out of the user's ~/.codex, because everything in it
 * — their MCP servers, their AGENTS.md, their notify hook — would land in a
 * chat (measured 2026-09-23), and `app-server` has no `--ignore-user-config`.
 */
export interface CodexFact {
  found: boolean;
  version: string;
  /** The app's own CODEX_HOME. */
  home: string;
  /** A login exists in the app's home. */
  hasAuth: boolean;
  /** The user has their own login, so importing it is a real offer. */
  userAuth: boolean;
  /** The official TradingView MCP as registered in the app's own Codex home. */
  official: OfficialMcpFact;
}

export interface PreflightCheck {
  id: CheckId;
  status: CheckStatus;
  /** i18n key for the one-line explanation; `vars` fills its {placeholders}. */
  detailKey: string;
  vars?: Record<string, string | number>;
  fix?: PreflightFix;
  /** https link with the background, opened in the system browser. */
  docs?: string;
  /** Optional checks warn but never make the whole run unhealthy. */
  optional?: boolean;
}

export interface PreflightResult {
  checks: PreflightCheck[];
  at: number;
  /** No required check came back `fail`. */
  ok: boolean;
}

/** Who answers on a CDP port, not merely whether someone does. */
export interface CdpProbe {
  up: boolean;
  /** The `Browser` string from /json/version. */
  browser: string;
  /** A tradingview.com/chart tab is open. */
  chartTab: boolean;
  /** Anything on this port looks like TradingView at all, chart tab or not. */
  tradingview: boolean;
  /** The first few targets, so a conflict can name what is actually there. */
  targets: { url: string; title: string }[];
}

export const EMPTY_CDP: CdpProbe = { up: false, browser: '', chartTab: false, tradingview: false, targets: [] };

// ---------- the facts the checks are built from ----------

export interface FileFact {
  path: string;
  exists: boolean;
}

/**
 * How tvmcp is going to be started, and whether it can be.
 *
 * `bundled` — the installer shipped the payload and the app provisions its own
 * environment in userData; `uv` and a clone of the repository are then the
 * app's business, not the user's, and the `uv` / `tvmcpClone` checks are not
 * shown at all. `external` — a dev run or a user with their own checkout: the
 * old two checks apply and this one is not shown.
 */
export interface RuntimeFact {
  mode: 'bundled' | 'external';
  tvmcpDir: string;
  /** The shipped payload is intact (pyproject named tvmcp + uv.lock present). */
  payloadOk: boolean;
  projectName: string;
  /** An environment exists and was built from exactly this uv.lock. */
  provisioned: boolean;
  /** An environment exists but was built from a different uv.lock. */
  stale: boolean;
  pythonPath: string;
  uvPath: string;
  lockHash: string;
  /**
   * The headless browser is installed separately and on demand: it is ~270 MB
   * and only three tvmcp modules use it (chart rendering, the backtest report,
   * the doctor). `null` = not applicable, i.e. there is no environment of ours
   * to install it into.
   */
  chromium: boolean | null;
}

/**
 * The course knowledge base (gt-rag): a second MCP server, shipped in the same
 * installer and run out of the same Python environment as tvmcp.
 *
 * `bundled` — the app carries the content and seeds its writable half into
 * userData; `external` — a developer's own clone named in the settings, with
 * its own data and its own venv; `off` — neither, or switched off, and the
 * model simply has no `gt-database` tools.
 */
export interface KbFact {
  mode: 'bundled' | 'external' | 'off';
  /** The settings switch. `enabled` with mode `off` means "nothing to enable". */
  enabled: boolean;
  dir: string;
  /** The payload (or the clone) holds what the server needs in order to start. */
  payloadOk: boolean;
  /** The store was copied into userData from exactly this payload. */
  seeded: boolean;
  /** A store is there, but it came from a different payload. */
  stale: boolean;
  /** The embedding model is in the cache the server will use. */
  model: boolean;
  /** The installer carried a model to seed from, so no download is needed. */
  modelShipped: boolean;
  /** Course pages in the payload — the one number worth showing. */
  pages: number;
}

export interface PreflightFacts {
  uv: { found: boolean; version: string };
  clone: { path: string; repo: string; dirExists: boolean; pyproject: 'ok' | 'missing' | 'wrong-name'; projectName: string };
  runtime: RuntimeFact;
  kb: KbFact;
  /** `ran: false` = the runtime is not usable yet, so booting was not attempted. */
  boot: { ran: boolean; ok: boolean; tools: number; error: string };
  claude: ClaudeAuthStatus;
  official: OfficialMcpFact;
  codex: CodexFact;
  tv: { running: boolean; port: number; cdp: CdpProbe };
  /** A port nobody answers on, so "switch the port" is a real offer. */
  freePort: number | null;
  workflows: { loaded: string[]; failed: string[] };
  files: { riskRules: FileFact; journal: FileFact };
  platform: string;
}

const UV_WINDOWS = 'powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"';
const UV_UNIX = 'curl -LsSf https://astral.sh/uv/install.sh | sh';
const UV_DOCS = 'https://docs.astral.sh/uv/';

/** Candidate ports offered when something else owns the configured one. */
export const PORT_CANDIDATES = [9223, 9224, 9225, 9226];

/** A short human name for whoever answers on a CDP port. */
export function cdpOwner(cdp: CdpProbe): string {
  if (!cdp.up) return '';
  const target = cdp.targets.find((t) => (t.title || '').trim()) ?? cdp.targets[0];
  const title = (target?.title || '').trim().slice(0, 60);
  const url = (target?.url || '').trim();
  let host = '';
  const m = url.match(/^[a-z-]+:\/\/([^/?#]+)/i);
  if (m) host = m[1];
  return [title, host].filter(Boolean).join(' — ') || cdp.browser || '?';
}

export function firstLine(s: string): string {
  const line = String(s || '')
    .split('\n')
    .map((l) => l.trim())
    .find(Boolean);
  return (line || '—').slice(0, 300);
}

// ---------- facts -> checks ----------

export function buildChecks(cfg: Settings, f: PreflightFacts): PreflightCheck[] {
  const checks: PreflightCheck[] = [];
  const isWin = f.platform === 'win32';

  // 1-2. Where tvmcp comes from. Bundled: one check about the app's own
  // environment. External: the two old ones about the user's uv and clone.
  if (f.runtime.mode === 'bundled') checks.push(runtimeCheck(f.runtime));
  else checks.push(...externalToolchainChecks(f, isWin));

  // 3. does it boot — where a missing dependency or the wrong Python shows up.
  checks.push(bootCheck(cfg, f));

  // 3b. the headless browser, in bundled mode only: in a dev run the developer
  // owns their own playwright install and we have no business reporting on it.
  if (f.runtime.mode === 'bundled' && f.runtime.chromium !== null) checks.push(chromiumCheck(f.runtime));

  // 3c. the course knowledge base — optional like the browser: without it the
  // copilot still reads charts and runs scans, it just cannot cite the course.
  checks.push(...kbChecks(f.kb));

  // 4. the engine that answers the chat. Only the selected one is diagnosed:
  // a red row about a Codex that the user never chose is noise, and the wizard
  // drops the step that produced no checks.
  if (cfg.engine === 'codex') {
    checks.push(...codexChecks(f.codex));
    checks.push(...chartChecks(cfg, f));
    return checks;
  }
  if (f.claude === 'none') {
    checks.push({
      id: 'claude',
      status: 'fail',
      detailKey: 'preflight.claude.none',
      // Logging in is an interactive terminal flow; a button here would lie.
      fix: { kind: 'copy', labelKey: 'preflight.fix.copyLogin', command: 'claude', noteKey: 'preflight.claude.note' },
    });
  } else {
    checks.push({ id: 'claude', status: 'ok', detailKey: `preflight.claude.${f.claude}` });
  }

  // 5. the official TradingView MCP — optional, and the app can set it up.
  checks.push(officialCheck(f.official));

  checks.push(...chartChecks(cfg, f));
  return checks;
}

/** The old pair: a `uv` on PATH and a directory that really holds tvmcp. */
function externalToolchainChecks(f: PreflightFacts, isWin: boolean): PreflightCheck[] {
  const checks: PreflightCheck[] = [];

  checks.push(
    f.uv.found
      ? { id: 'uv', status: 'ok', detailKey: 'preflight.uv.ok', vars: { version: f.uv.version } }
      : {
          id: 'uv',
          status: 'fail',
          detailKey: 'preflight.uv.missing',
          docs: UV_DOCS,
          fix: {
            kind: 'copy',
            labelKey: 'preflight.fix.copyInstall',
            command: isWin ? UV_WINDOWS : UV_UNIX,
            altCommand: isWin ? UV_UNIX : UV_WINDOWS,
            noteKey: 'preflight.uv.note',
          },
        },
  );

  // 2. the tvmcp clone — a path in the settings that must really hold tvmcp.
  const c = f.clone;
  if (!c.path.trim()) {
    checks.push({
      id: 'tvmcpClone',
      status: 'fail',
      detailKey: 'preflight.clone.unset',
      fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickDir', field: 'tvmcpPath', directory: true },
    });
  } else if (!c.dirExists) {
    checks.push({
      id: 'tvmcpClone',
      status: 'fail',
      detailKey: 'preflight.clone.missing',
      vars: { path: c.path },
      // We never clone anything ourselves: this is the command, the user runs it.
      fix: { kind: 'copy', labelKey: 'preflight.fix.copyClone', command: `git clone ${c.repo} "${c.path}"`, noteKey: 'preflight.clone.note' },
    });
  } else if (c.pyproject === 'missing') {
    checks.push({
      id: 'tvmcpClone',
      status: 'fail',
      detailKey: 'preflight.clone.noPyproject',
      vars: { path: c.path },
      fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickDir', field: 'tvmcpPath', directory: true },
    });
  } else if (c.pyproject === 'wrong-name') {
    checks.push({
      id: 'tvmcpClone',
      status: 'fail',
      detailKey: 'preflight.clone.wrongName',
      vars: { path: c.path, name: c.projectName || '—' },
      fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickDir', field: 'tvmcpPath', directory: true },
    });
  } else {
    checks.push({ id: 'tvmcpClone', status: 'ok', detailKey: 'preflight.clone.ok', vars: { path: c.path } });
  }

  return checks;
}

/**
 * The app's own Python environment: shipped payload, provisioned venv, and
 * whether the venv still matches the uv.lock that shipped with this version.
 *
 * A broken payload is not something a button can repair — the installation
 * itself is damaged — so it explains instead of offering a fix.
 */
function runtimeCheck(r: RuntimeFact): PreflightCheck {
  if (!r.payloadOk) {
    return {
      id: 'tvmcpRuntime',
      status: 'fail',
      detailKey: 'preflight.runtime.payloadBroken',
      vars: { path: r.tvmcpDir, name: r.projectName || '—' },
      fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.runtime.payloadNote' },
    };
  }
  if (r.provisioned) {
    return { id: 'tvmcpRuntime', status: 'ok', detailKey: 'preflight.runtime.ok', vars: { python: r.pythonPath, lock: r.lockHash } };
  }
  return {
    id: 'tvmcpRuntime',
    status: r.stale ? 'warn' : 'fail',
    detailKey: r.stale ? 'preflight.runtime.stale' : 'preflight.runtime.missing',
    vars: { path: r.tvmcpDir },
    fix: { kind: 'run-task', labelKey: 'preflight.fix.provision', task: 'provision-runtime', noteKey: 'preflight.runtime.note' },
  };
}

/**
 * The knowledge base, in two checks: is the content usable at all, and is the
 * embedding model there. They are split because they fail for different reasons
 * and are fixed by different presses — a copy the app owns, and a download.
 *
 * Every one of them is `optional`: a desk with no course behind it is a smaller
 * product, not a broken one.
 */
function kbChecks(kb: KbFact): PreflightCheck[] {
  if (!kb.enabled) {
    return [
      {
        id: 'knowledgeBase',
        status: 'warn',
        optional: true,
        detailKey: 'preflight.kb.disabled',
        fix: { kind: 'set-flag', labelKey: 'preflight.fix.enableKb', field: 'knowledgeBase', value: true, noteKey: 'preflight.kb.disabledNote' },
      },
    ];
  }
  if (kb.mode === 'off') {
    // Nothing shipped and no clone named: offer to point at one, which is the
    // only thing that can make this check pass on a build without the payload.
    return [
      {
        id: 'knowledgeBase',
        status: 'warn',
        optional: true,
        detailKey: 'preflight.kb.absent',
        fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickDir', field: 'gtRagPath', directory: true },
      },
    ];
  }
  if (kb.mode === 'external') {
    return [
      kb.payloadOk
        ? { id: 'knowledgeBase', status: 'ok', optional: true, detailKey: 'preflight.kb.external', vars: { path: kb.dir } }
        : {
            id: 'knowledgeBase',
            status: 'warn',
            optional: true,
            detailKey: 'preflight.kb.externalBroken',
            vars: { path: kb.dir },
            fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickDir', field: 'gtRagPath', directory: true },
          },
    ];
  }

  const checks: PreflightCheck[] = [];
  if (!kb.payloadOk) {
    checks.push({
      id: 'knowledgeBase',
      status: 'warn',
      optional: true,
      detailKey: 'preflight.kb.payloadBroken',
      vars: { path: kb.dir },
      fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.kb.payloadNote' },
    });
    return checks;
  }
  if (kb.seeded) {
    checks.push({ id: 'knowledgeBase', status: 'ok', optional: true, detailKey: 'preflight.kb.ok', vars: { pages: kb.pages } });
  } else {
    checks.push({
      id: 'knowledgeBase',
      status: 'warn',
      optional: true,
      detailKey: kb.stale ? 'preflight.kb.stale' : 'preflight.kb.unseeded',
      vars: { pages: kb.pages },
      fix: { kind: 'run-task', labelKey: 'preflight.fix.seedKb', task: 'kb-import', noteKey: 'preflight.kb.seedNote' },
    });
  }

  // The model is the half that may need the network, so it says so itself.
  if (kb.model) {
    checks.push({ id: 'kbModel', status: 'ok', optional: true, detailKey: 'preflight.kbModel.ok' });
  } else if (kb.modelShipped) {
    checks.push({
      id: 'kbModel',
      status: 'warn',
      optional: true,
      detailKey: 'preflight.kbModel.shipped',
      fix: { kind: 'run-task', labelKey: 'preflight.fix.seedKb', task: 'kb-import', noteKey: 'preflight.kbModel.shippedNote' },
    });
  } else {
    checks.push({
      id: 'kbModel',
      status: 'warn',
      optional: true,
      detailKey: 'preflight.kbModel.missing',
      fix: { kind: 'run-task', labelKey: 'preflight.fix.fetchModel', task: 'kb-model', noteKey: 'preflight.kbModel.note' },
    });
  }
  return checks;
}

/**
 * The headless browser. Optional on purpose: the desk reads levels, calendars,
 * the journal and the chart through TradingView itself without it — only
 * rendered chart images and backtest reports need one.
 */
function chromiumCheck(r: RuntimeFact): PreflightCheck {
  if (r.chromium) return { id: 'chromium', status: 'ok', optional: true, detailKey: 'preflight.chromium.ok' };
  return {
    id: 'chromium',
    status: 'warn',
    optional: true,
    detailKey: 'preflight.chromium.missing',
    fix: { kind: 'run-task', labelKey: 'preflight.fix.installChromium', task: 'install-chromium', noteKey: 'preflight.chromium.note' },
  };
}

/** Does the server actually come up, and with how many tools. */
function bootCheck(cfg: Settings, f: PreflightFacts): PreflightCheck {
  if (!f.boot.ran) return { id: 'tvmcpBoot', status: 'unknown', detailKey: 'preflight.boot.skipped' };
  if (f.boot.ok) {
    return { id: 'tvmcpBoot', status: 'ok', detailKey: 'preflight.boot.ok', vars: { tools: f.boot.tools, toolsets: cfg.toolsets || '—' } };
  }
  // Bundled: re-provisioning is the app's own job and a button. External: the
  // user owns the checkout, so `uv sync` is theirs to run.
  const fix: PreflightFix =
    f.runtime.mode === 'bundled'
      ? { kind: 'run-task', labelKey: 'preflight.fix.provision', task: 'provision-runtime', noteKey: 'preflight.boot.noteBundled' }
      : { kind: 'copy', labelKey: 'preflight.fix.copySync', command: `uv --directory "${f.clone.path}" sync`, noteKey: 'preflight.boot.note' };
  return { id: 'tvmcpBoot', status: 'fail', detailKey: 'preflight.boot.failed', vars: { error: firstLine(f.boot.error) }, fix };
}

/**
 * The official TradingView MCP (mcp.tradingview.com).
 *
 * Always optional: everything the desk does day to day runs on tvmcp, and this
 * server adds news, calendars, screeners and alerts on top. It is registered in
 * Claude Code's own store rather than ours, because that is where the OAuth
 * token lives — see `shared/official-mcp.ts`.
 */
function codexChecks(c: CodexFact): PreflightCheck[] {
  if (!c.found) {
    return [
      {
        id: 'codex',
        status: 'fail',
        detailKey: 'preflight.codex.missing',
        // Installing someone's global toolchain is their call, never the app's.
        fix: { kind: 'copy', labelKey: 'preflight.fix.copyInstall', command: 'npm i -g @openai/codex', noteKey: 'preflight.codex.installNote' },
        docs: 'https://developers.openai.com/codex/cli',
      },
    ];
  }
  const checks: PreflightCheck[] = [{ id: 'codex', status: 'ok', detailKey: 'preflight.codex.ok', vars: { version: c.version || '—' } }];

  if (c.hasAuth) {
    checks.push({ id: 'codexAuth', status: 'ok', detailKey: 'preflight.codexAuth.ok', vars: { home: c.home } });
  } else if (c.userAuth) {
    // The app has a login it could copy. It does not copy it by itself:
    // credentials move only when the user presses the button.
    checks.push({
      id: 'codexAuth',
      status: 'fail',
      detailKey: 'preflight.codexAuth.importable',
      vars: { home: c.home },
      fix: { kind: 'run-task', labelKey: 'preflight.fix.codexAuthImport', task: 'codex-auth-import', noteKey: 'preflight.codexAuth.importNote' },
    });
  } else {
    checks.push({
      id: 'codexAuth',
      status: 'fail',
      detailKey: 'preflight.codexAuth.none',
      vars: { home: c.home },
      // `codex login` opens a browser and waits; it belongs in the user's own
      // terminal, with the app's CODEX_HOME in front of it.
      // PowerShell first (that is the shell this app's users have open), POSIX
      // as the alternative. `set X=Y && …` in cmd.exe is deliberately NOT
      // offered: it swallows the space before `&&` into the value, and codex
      // would quietly build a second home with a trailing space in its name.
      fix: {
        kind: 'copy',
        labelKey: 'preflight.fix.copyLogin',
        command: `$env:CODEX_HOME='${c.home}'; codex login`,
        altCommand: `CODEX_HOME="${c.home}" codex login`,
        noteKey: 'preflight.codexAuth.loginNote',
      },
    });
  }

  checks.push(codexOfficialCheck(c.official));
  return checks;
}

/** The official TradingView MCP inside the app's Codex home — its own OAuth store. */
function codexOfficialCheck(o: OfficialMcpFact): PreflightCheck {
  const base = { id: 'codexOfficialMcp' as const, optional: true };
  if (o.status === 'unknown') return { ...base, status: 'unknown', detailKey: 'preflight.official.unknown' };
  if (!o.registered) {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.codexOfficial.absent',
      fix: { kind: 'run-task', labelKey: 'preflight.fix.officialAdd', task: 'codex-mcp-add', noteKey: 'preflight.codexOfficial.addNote' },
    };
  }
  if (o.status !== 'connected') {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.codexOfficial.needsAuth',
      vars: { raw: o.raw || '—' },
      fix: { kind: 'run-task', labelKey: 'preflight.fix.officialLogin', task: 'codex-mcp-login', noteKey: 'preflight.codexOfficial.loginNote' },
    };
  }
  if (!o.enabled) {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.official.offInSettings',
      vars: { scope: o.scope || '—' },
      fix: { kind: 'set-flag', labelKey: 'preflight.fix.officialEnable', field: 'officialMcp', value: true, noteKey: 'preflight.official.enableNote' },
    };
  }
  return { ...base, status: 'ok', detailKey: 'preflight.codexOfficial.ok' };
}

function officialCheck(o: OfficialMcpFact): PreflightCheck {
  const base = { id: 'officialMcp' as const, optional: true };
  if (o.status === 'unknown') {
    return { ...base, status: 'unknown', detailKey: 'preflight.official.unknown' };
  }
  if (!o.registered) {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.official.absent',
      fix: { kind: 'run-task', labelKey: 'preflight.fix.officialAdd', task: 'official-mcp-add', noteKey: 'preflight.official.addNote' },
    };
  }
  if (o.status !== 'connected') {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.official.needsAuth',
      vars: { raw: o.raw || '—' },
      fix: { kind: 'run-task', labelKey: 'preflight.fix.officialLogin', task: 'official-mcp-login', noteKey: 'preflight.official.loginNote' },
    };
  }
  if (!o.enabled) {
    return {
      ...base,
      status: 'warn',
      detailKey: 'preflight.official.offInSettings',
      vars: { scope: o.scope || '—' },
      fix: { kind: 'set-flag', labelKey: 'preflight.fix.officialEnable', field: 'officialMcp', value: true, noteKey: 'preflight.official.enableNote' },
    };
  }
  return { ...base, status: 'ok', detailKey: 'preflight.official.ok', vars: { scope: o.scope || '—' } };
}

/** TradingView Desktop, the debug port, the workflows and the two day files. */
function chartChecks(cfg: Settings, f: PreflightFacts): PreflightCheck[] {
  const checks: PreflightCheck[] = [];

  // 5. TradingView Desktop.
  const cdp = f.tv.cdp;
  if (cdp.up && cdp.tradingview) {
    checks.push(
      cdp.chartTab
        ? { id: 'tvDesktop', status: 'ok', detailKey: 'preflight.tv.ok', vars: { port: f.tv.port } }
        : { id: 'tvDesktop', status: 'warn', detailKey: 'preflight.tv.noChart', fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.tv.noChartNote' } },
    );
  } else if (f.tv.running && !cdp.up) {
    // Single-instance app: relaunching only focuses the flagless copy. We never
    // close it ourselves — the button explains, the user decides.
    checks.push({
      id: 'tvDesktop',
      status: 'fail',
      detailKey: 'preflight.tv.flagless',
      vars: { port: f.tv.port },
      fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.tv.flaglessNote' },
    });
  } else if (f.tv.running && cdp.up) {
    // TradingView is up and someone else owns the port: same remedy, and the
    // port check below says who it is.
    checks.push({
      id: 'tvDesktop',
      status: 'fail',
      detailKey: 'preflight.tv.otherOwner',
      vars: { port: f.tv.port, owner: cdpOwner(cdp) },
      fix: { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.tv.flaglessNote' },
    });
  } else {
    checks.push({
      id: 'tvDesktop',
      status: 'fail',
      detailKey: 'preflight.tv.notRunning',
      fix: { kind: 'launch-tv', labelKey: 'preflight.fix.launchTv' },
    });
  }

  // 6. who owns the CDP port. A 200 from /json/version proves nothing: any
  // CDP-speaking app can own it, and 9222 usually already belongs to one.
  if (!cdp.up) {
    checks.push({ id: 'cdpPort', status: 'ok', detailKey: 'preflight.port.free', vars: { port: f.tv.port } });
  } else if (cdp.tradingview) {
    checks.push({ id: 'cdpPort', status: 'ok', detailKey: 'preflight.port.tv', vars: { port: f.tv.port } });
  } else {
    checks.push({
      id: 'cdpPort',
      status: 'fail',
      detailKey: 'preflight.port.conflict',
      vars: { port: f.tv.port, owner: cdpOwner(cdp), browser: cdp.browser || '—' },
      fix: f.freePort
        ? { kind: 'set-port', labelKey: 'preflight.fix.setPort', port: f.freePort }
        : { kind: 'explain', labelKey: 'preflight.fix.how', noteKey: 'preflight.port.noFreeNote' },
    });
  }

  // 7. workflows — without one there is no prompt block and no day panel.
  if (!f.workflows.loaded.length) {
    checks.push({ id: 'workflows', status: 'fail', detailKey: 'preflight.workflows.none', vars: { failed: f.workflows.failed.join(', ') || '—' } });
  } else if (f.workflows.failed.length) {
    checks.push({ id: 'workflows', status: 'warn', detailKey: 'preflight.workflows.someFailed', vars: { n: f.workflows.loaded.length, failed: f.workflows.failed.join(', ') } });
  } else {
    checks.push({ id: 'workflows', status: 'ok', detailKey: 'preflight.workflows.ok', vars: { n: f.workflows.loaded.length, list: f.workflows.loaded.join(', ') } });
  }

  // 8-9. the two optional files the day panel reads.
  checks.push(fileCheck('riskRules', f.files.riskRules, 'riskRulesPath'));
  checks.push(fileCheck('journal', f.files.journal, 'journalPath'));

  return checks;
}

function fileCheck(id: 'riskRules' | 'journal', fact: FileFact, field: PathField): PreflightCheck {
  if (!fact.path.trim()) {
    return { id, status: 'warn', optional: true, detailKey: `preflight.${id}.unset`, fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickFile', field } };
  }
  if (!fact.exists) {
    return { id, status: 'fail', optional: true, detailKey: `preflight.${id}.missing`, vars: { path: fact.path }, fix: { kind: 'pick-file', labelKey: 'preflight.fix.pickFile', field } };
  }
  return { id, status: 'ok', optional: true, detailKey: `preflight.${id}.ok`, vars: { path: fact.path } };
}

/** A run is healthy when no required check failed. */
export function resultOf(checks: PreflightCheck[], at = Date.now()): PreflightResult {
  return { checks, at, ok: !checks.some((c) => c.status === 'fail' && !c.optional) };
}

/** Worst status of a set of checks, for a step header or a badge. */
export function worstStatus(checks: PreflightCheck[]): CheckStatus {
  const order: CheckStatus[] = ['fail', 'warn', 'unknown', 'ok'];
  for (const s of order) if (checks.some((c) => c.status === s)) return s;
  return 'ok';
}
