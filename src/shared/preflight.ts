// The preflight contract: what a check looks like, what a fix may be, and the
// pure mapping from gathered facts to the ordered check list.
//
// Electron-free on purpose, like `panel.ts`: main builds the list, the renderer
// renders it, vitest feeds it faked facts. The probes themselves live in
// `src/main/preflight.ts`.
//
// Two rules the whole feature keeps:
//   * the app NEVER runs a network or destructive command by itself. `git
//     clone`, the uv installer, `claude /login` and "close TradingView" come
//     back as copyable text with one sentence saying what will happen;
//   * nothing is cached. "Проверить заново" means a real re-probe.
import type { ClaudeAuthStatus, Settings } from './api';

export const CHECK_IDS = ['uv', 'tvmcpClone', 'tvmcpBoot', 'claude', 'tvDesktop', 'cdpPort', 'workflows', 'riskRules', 'journal'] as const;
export type CheckId = (typeof CHECK_IDS)[number];

export type CheckStatus = 'ok' | 'warn' | 'fail' | 'unknown';

/** Settings fields a `pick-file` fix may write. */
export type PathField = 'tvmcpPath' | 'riskRulesPath' | 'journalPath' | 'fieldNotesPath';

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
  | { kind: 'explain'; labelKey: string; noteKey: string };

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

export interface PreflightFacts {
  uv: { found: boolean; version: string };
  clone: { path: string; repo: string; dirExists: boolean; pyproject: 'ok' | 'missing' | 'wrong-name'; projectName: string };
  /** `ran: false` = uv or the clone is broken, so booting was not attempted. */
  boot: { ran: boolean; ok: boolean; tools: number; error: string };
  claude: ClaudeAuthStatus;
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

  // 1. uv — everything tvmcp does starts here.
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

  // 3. does it boot — where a missing dependency or the wrong Python shows up.
  if (!f.boot.ran) {
    checks.push({ id: 'tvmcpBoot', status: 'unknown', detailKey: 'preflight.boot.skipped' });
  } else if (f.boot.ok) {
    checks.push({ id: 'tvmcpBoot', status: 'ok', detailKey: 'preflight.boot.ok', vars: { tools: f.boot.tools, toolsets: cfg.toolsets || '—' } });
  } else {
    checks.push({
      id: 'tvmcpBoot',
      status: 'fail',
      detailKey: 'preflight.boot.failed',
      vars: { error: firstLine(f.boot.error) },
      fix: { kind: 'copy', labelKey: 'preflight.fix.copySync', command: `uv --directory "${c.path}" sync`, noteKey: 'preflight.boot.note' },
    });
  }

  // 4. Claude — the SDK spawns the user's own Claude Code binary.
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
