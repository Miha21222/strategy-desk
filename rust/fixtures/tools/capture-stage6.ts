// Golden cases for stage 6 (connections, wizard, settings, schedule) from the
// OLD code: `buildChecks` (src/shared/preflight.ts), the wizard's step and skip
// logic (src/renderer/setup.ts) and the config → settings mapping
// (src/shared/defaults.ts `legacyConfigPatch` — executed — plus store.ts
// `readConfig` / `toSettings` — three lines, transcribed: store.ts imports
// Electron). schedule.json is the one file NOT from the old app: the old app
// had fixed times; it states TZ №21 + CONTRACTS «этап 6» as cases.
//
//   TZ=Europe/Kyiv node capture-stage6.cjs <out_dir>
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import type { Settings } from '../../../src/shared/api';
import { DEFAULT_SETTINGS, legacyConfigPatch } from '../../../src/shared/defaults';
import { emptyOfficialFact } from '../../../src/shared/official-mcp';
import { buildChecks, EMPTY_CDP, resultOf, SCHEDULER_TASKS, type PreflightCheck, type PreflightFacts } from '../../../src/shared/preflight';
import * as W from '../../../src/renderer/setup';
import { kyivNow } from '../../../src/shared/macro';
import { canonLines } from './canon';

const out = path.resolve(process.argv[2]);
mkdirSync(out, { recursive: true });
const put = (n: string, s: string) => writeFileSync(path.join(out, n), s);
const SOURCE = 'rust/fixtures/tools/capture-stage6.ts over the old src/shared/preflight.ts, src/renderer/setup.ts, src/shared/defaults.ts. Regenerate: bash rust/fixtures/tools/capture-golden.sh';
const clone = <T>(x: T): T => JSON.parse(JSON.stringify(x));
type DeepPartial<T> = { [K in keyof T]?: T[K] extends object ? DeepPartial<T[K]> : T[K] };
function merge<T>(base: T, patch: DeepPartial<T>): T {
  const r = clone(base) as Record<string, unknown>;
  for (const [k, v] of Object.entries(patch as Record<string, unknown>)) {
    r[k] = v && typeof v === 'object' && !Array.isArray(v) && r[k] && typeof r[k] === 'object' ? merge(r[k], v as never) : v;
  }
  return r as T;
}

// ------------------------------------------------------------ checks.json

const VAULT = 'C:\\Users\\T\\Documents\\Strategy Desk';
const PY = 'C:\\Users\\T\\AppData\\Roaming\\strategy-desk\\py\\venv\\Scripts\\python.exe';
/** A healthy packaged install on the owner's laptop: every MVP check green. */
const GOOD: PreflightFacts = {
  uv: { found: true, version: 'uv 0.9.2' },
  clone: { path: '', repo: 'https://example.invalid/tradingview-mcp.git', dirExists: false, pyproject: 'missing', projectName: '' },
  runtime: { mode: 'bundled', tvmcpDir: 'C:\\Program Files\\Strategy Desk\\resources\\tvmcp', payloadOk: true, projectName: 'tvmcp', provisioned: true, stale: false, pythonPath: PY, uvPath: 'C:\\Program Files\\Strategy Desk\\resources\\uv.exe', lockHash: '3f9c1a2b', chromium: null },
  kb: { mode: 'off', enabled: false, dir: '', payloadOk: false, seeded: false, stale: false, model: false, modelShipped: false, pages: 0 },
  boot: { ran: true, ok: true, tools: 59, error: '' },
  claude: 'subscription',
  official: emptyOfficialFact(false),
  codex: { found: false, version: '', home: '', hasAuth: false, userAuth: false, official: emptyOfficialFact(false) },
  tv: { running: true, port: 9223, cdp: { up: true, browser: 'Chrome/140.0', chartTab: true, tradingview: true, targets: [{ url: 'https://www.tradingview.com/chart/abc/', title: 'FDAX1! 25 000' }] } },
  freePort: 9224,
  workflows: { loaded: [], failed: [] },
  files: { riskRules: { path: '', exists: false }, journal: { path: '', exists: false } },
  platform: 'win32',
  vault: { path: VAULT, exists: true, nonEmpty: true, hasSkill: true, hasScaleTool: true, hasScaleConfig: true, templateVersion: '2', templateAvailable: true },
  toolsPython: { python: PY, exists: true, checked: true, importOk: true, error: '' },
  tvSession: { set: true, verifiedAt: Date.parse('2026-11-25T07:31:00Z') },
  scheduler: { queried: true, tasks: SCHEDULER_TASKS.map((name) => ({ name, state: 'Ready', arguments: `-File "${VAULT}\\indices-kb\\tools\\macro_schedule.ps1"` })), script: `${VAULT}\\indices-kb\\tools\\macro_schedule.ps1` },
  timezone: 'Europe/Kyiv',
};
const CFG: Settings = { ...DEFAULT_SETTINGS, vaultPath: VAULT };

type Variant = [string, string, DeepPartial<PreflightFacts>, Partial<Settings>?];
const V: Variant[] = [
  ['all_green', 'the owner\'s laptop, everything ready', {}],
  // vault
  ['vault_unset', 'no vault path in the settings', { vault: { path: '' } }],
  ['vault_missing', 'the folder does not exist; the template is there → «Создать vault»', { vault: { exists: false, nonEmpty: false, hasSkill: false, hasScaleTool: false, hasScaleConfig: false, templateVersion: '' } }],
  ['vault_missing_no_template', 'the folder does not exist and the build carries no template → explain', { vault: { exists: false, nonEmpty: false, hasSkill: false, hasScaleTool: false, hasScaleConfig: false, templateVersion: '', templateAvailable: false } }],
  ['vault_empty', 'the folder exists and is empty → create', { vault: { nonEmpty: false, hasSkill: false, hasScaleTool: false, hasScaleConfig: false, templateVersion: '' } }],
  ['vault_partial', 'some template files are there → «Дополнить» (missing files only)', { vault: { hasScaleConfig: false, templateVersion: '' } }],
  ['vault_partial_no_template', 'partial, no template → pick another folder', { vault: { hasScaleConfig: false, templateVersion: '', templateAvailable: false } }],
  ['vault_stranger', 'a folder full of someone else\'s files → pick another (never write there)', { vault: { hasSkill: false, hasScaleTool: false, hasScaleConfig: false, templateVersion: '' } }],
  ['vault_ok_no_template_version', 'a vault made by hand (no template.json)', { vault: { templateVersion: '' } }],
  // claude
  ['claude_none', 'Claude Code not logged in → claude-login task (button)', { claude: 'none' }],
  ['claude_api_key', 'logged in with an API key', { claude: 'api-key' }],
  // runtime / python
  ['runtime_missing', 'packaged, environment not built → provision-runtime (auto on the wizard page)', { runtime: { provisioned: false }, boot: { ran: false, ok: false, tools: 0 }, toolsPython: { exists: false, checked: false, importOk: false } }],
  ['runtime_stale', 'environment built from another uv.lock → warn + provision', { runtime: { provisioned: false, stale: true } }],
  ['runtime_payload_broken', 'installation damaged → explain only', { runtime: { payloadOk: false, provisioned: false, projectName: 'other' }, boot: { ran: false, ok: false, tools: 0 } }],
  ['boot_failed', 'tvmcp does not start', { boot: { ok: false, tools: 0, error: 'ModuleNotFoundError: No module named \'numba\'\nTraceback…' } }],
  ['python_import_failed', 'the vault tools\' imports fail', { toolsPython: { importOk: false, error: 'ImportError: pandas\n…' } }],
  ['python_unchecked', 'interpreter there, not asked', { toolsPython: { checked: false, importOk: false } }],
  ['python_none', 'no interpreter path at all', { toolsPython: { python: '', exists: false, checked: false, importOk: false } }],
  ['external_dev_ok', 'a dev run: uv + a checkout', { runtime: { mode: 'external' }, clone: { path: 'C:\\dev\\tradingview-mcp', dirExists: true, pyproject: 'ok', projectName: 'tvmcp' } }],
  ['external_no_uv', 'dev run, no uv → copy the installer command (never run by the app)', { runtime: { mode: 'external' }, uv: { found: false, version: '' }, clone: { path: 'C:\\dev\\tradingview-mcp', dirExists: true, pyproject: 'ok', projectName: 'tvmcp' } }],
  ['external_no_uv_linux', 'same on Linux: the curl command first', { platform: 'linux', runtime: { mode: 'external' }, uv: { found: false, version: '' }, clone: { path: '/opt/tvmcp', dirExists: true, pyproject: 'ok', projectName: 'tvmcp' } }],
  ['external_clone_unset', 'dev run, no checkout path → pick', { runtime: { mode: 'external' } }],
  ['external_clone_missing', 'checkout path that does not exist → copy `git clone` (never run by the app)', { runtime: { mode: 'external' }, clone: { path: 'C:\\dev\\tvmcp', dirExists: false } }],
  ['external_wrong_name', 'a folder that is another project', { runtime: { mode: 'external' }, clone: { path: 'C:\\dev\\x', dirExists: true, pyproject: 'wrong-name', projectName: 'something' } }],
  ['external_boot_failed', 'dev run, boot fails → copy `uv sync` (the user owns the checkout)', { runtime: { mode: 'external' }, clone: { path: 'C:\\dev\\tradingview-mcp', dirExists: true, pyproject: 'ok', projectName: 'tvmcp' }, boot: { ok: false, error: 'boom' } }],
  // TradingView / CDP
  ['tv_not_running', 'TradingView closed → «Запустить TradingView» (the app launches it on a press)', { tv: { running: false, cdp: clone(EMPTY_CDP) } }],
  ['tv_flagless', 'TradingView running without the debug port → explain (the app never closes it)', { tv: { running: true, cdp: clone(EMPTY_CDP) } }],
  ['tv_no_chart', 'TradingView on the port, no chart tab', { tv: { cdp: { chartTab: false } } }],
  ['port_other_owner', 'another app owns the port, a free one exists → set-port', { tv: { running: true, cdp: { up: true, browser: 'Chrome/140', chartTab: false, tradingview: false, targets: [{ url: 'https://example.com/', title: 'Some Electron app' }] } } }],
  ['port_other_owner_no_free', 'another app owns the port, none free → explain', { freePort: null, tv: { running: false, cdp: { up: true, browser: 'Chrome/140', chartTab: false, tradingview: false, targets: [] } } }],
  // session / scheduler / timezone
  ['session_none', 'no sessionid stored → explain where to copy it', { tvSession: { set: false, verifiedAt: 0 } }],
  ['session_unverified', 'stored, never checked → session-check (auto on the wizard page)', { tvSession: { verifiedAt: 0 } }],
  ['scheduler_unknown', 'PowerShell did not answer', { scheduler: { queried: false, tasks: [] } }],
  ['scheduler_none', 'no jobs, vault has the script → register (button)', { scheduler: { tasks: [] } }],
  ['scheduler_none_no_vault', 'no jobs, no vault tools → nothing to offer', { scheduler: { tasks: [], script: '' } }],
  ['scheduler_partial', 'one job missing', { scheduler: { tasks: [{ name: 'Trading Macro EU', state: 'Ready' }, { name: 'Trading Review', state: 'Ready' }] } }],
  ['scheduler_disabled', 'a job disabled', { scheduler: { tasks: [{ name: 'Trading Macro EU', state: 'Ready' }, { name: 'Trading Macro US', state: 'Disabled' }, { name: 'Trading Review', state: 'Ready' }] } }],
  ['scheduler_stale_asar', 'a job pointing into app.asar (0.2.1)', { scheduler: { tasks: SCHEDULER_TASKS.map((name) => ({ name, state: 'Ready', arguments: '-Claude "C:\\Program Files\\Strategy Desk\\resources\\app.asar\\node_modules\\x\\claude.exe"' })) } }],
  ['timezone_other', 'PC clock on Europe/Berlin', { timezone: 'Europe/Berlin' }],
  ['timezone_windows_name', 'Windows zone name for Kyiv', { timezone: 'FLE Standard Time' }],
  ['timezone_unknown', 'zone not read', { timezone: '' }],
  // whole machines
  ['fresh_pc', 'a new Windows user: nothing set up', {
    vault: { path: VAULT, exists: false, nonEmpty: false, hasSkill: false, hasScaleTool: false, hasScaleConfig: false, templateVersion: '', templateAvailable: true },
    claude: 'none', runtime: { provisioned: false }, boot: { ran: false, ok: false, tools: 0 }, toolsPython: { exists: false, checked: false, importOk: false },
    tv: { running: false, cdp: clone(EMPTY_CDP) }, tvSession: { set: false, verifiedAt: 0 }, scheduler: { tasks: [], script: '' }, timezone: 'Europe/Kyiv',
  }],
];

const MVP_OLD = ['vault', 'claude', 'uv', 'tvmcpClone', 'tvmcpRuntime', 'tvmcpBoot', 'toolsPython', 'tvDesktop', 'cdpPort', 'tvSession', 'scheduler', 'timezone'];
const checkCases = V.map(([name, about, patch, cfgPatch]) => {
  const facts = merge(GOOD, patch);
  const checks = buildChecks({ ...CFG, ...(cfgPatch ?? {}) }, facts);
  return { name, about, facts_patch: patch, settings_patch: cfgPatch ?? {}, checks, ok: resultOf(checks, 0).ok, order: checks.map((c) => c.id), mvp_order: checks.map((c) => c.id).filter((id) => MVP_OLD.includes(id)) };
});
const fixKinds: Record<string, Set<string>> = {};
for (const c of checkCases) for (const ch of c.checks as PreflightCheck[]) if (ch.fix) (fixKinds[ch.fix.kind] ??= new Set()).add(`${ch.id}${'task' in ch.fix ? `:${ch.fix.task}` : ''}`);
put('checks.json', canonLines(
  {
    about: 'buildChecks(settings, facts) of the old app for 42 machines: the all-green laptop and one fact broken at a time, plus a fresh PC. Each case: facts = base_facts deep-merged with facts_patch (objects merge key by key, arrays and scalars replace), settings = base_settings + settings_patch, every check (id, status, detailKey, vars, fix), the order, and the MVP subset. Rust CheckId mapping (CONTRACTS «этап 6»): vault→vault, claude→claude (+ new claude_auth), uv→uv, tvmcpClone|tvmcpRuntime|tvmcpBoot|toolsPython→tvmcp, tvDesktop→tradingview, cdpPort→cdp, tvSession→tv_session, scheduler→scheduler, timezone→timezone; dropped: chromium, knowledgeBase, kbModel, officialMcp, codex*, workflows, riskRules, journal. vars.time of tvSession.ok is the old toLocaleString under TZ=Europe/Kyiv — Rust formats its own.',
    base_facts: GOOD,
    base_settings: CFG,
    fix_kinds_seen: Object.fromEntries(Object.entries(fixKinds).map(([k, v]) => [k, [...v].sort()])),
    acts_by_itself: {
      never: ['copy (text to copy: uv installer, git clone, uv sync)', 'explain (a note)'],
      on_a_press: ['launch-tv', 'set-port', 'pick-file', 'run-task', 'set-flag'],
      auto_on_wizard_page: W.AUTO_TASKS,
      rule: 'CLAUDE.md «Preflight and the setup wizard»: the diagnosis never runs a network or destructive command by itself; a wizard PAGE presses AUTO_TASKS once per wizard session (never a retry loop); claude-login, scheduler-register and anything that removes wait for the button.',
    },
    source: SOURCE,
  },
  { cases: checkCases },
));

// ------------------------------------------------------------ wizard.json

const greenChecks = checkCases[0].checks as PreflightCheck[];
const fresh = checkCases.find((c) => c.name === 'fresh_pc')!.checks as PreflightCheck[];
const stepView = (checks: PreflightCheck[], ids: W.StepId[]) =>
  ids.map((id) => ({ step: id, checks: W.checksOfStep(id, checks).map((c) => c.id), status: W.stepStatus(id, checks), done: W.stepDone(id, checks), tasks: W.stepTaskIds(id, checks) }));
const MVP = W.stepIdsFor({ legacyUi: false });
const states = (o: Record<string, { running: boolean; ok: boolean | null }>) => Object.fromEntries(Object.entries(o).map(([id, s]) => [id, { id, ...s, lines: [], error: '', startedAt: 0, finishedAt: 0 }]));
const autoCases: { about: string; step: W.StepId; machine: string; tasks: Record<string, { running: boolean; ok: boolean | null }>; started: string[]; commands?: Record<string, string> }[] = [
  { about: 'fresh PC, tvmcp page opens: provision-runtime starts by itself', step: 'tvmcp', machine: 'fresh_pc', tasks: {}, started: [] },
  { about: 'already started once in this wizard session: never again (no retry loop)', step: 'tvmcp', machine: 'fresh_pc', tasks: {}, started: ['provision-runtime'] },
  { about: 'something is running: nothing else starts', step: 'tvmcp', machine: 'fresh_pc', tasks: { 'provision-runtime': { running: true, ok: null } }, started: [] },
  { about: 'it finished (failed) in this app run: the user retries, not the page', step: 'tvmcp', machine: 'fresh_pc', tasks: { 'provision-runtime': { running: false, ok: false } }, started: [] },
  { about: 'a dev run without the payload: the plan refused (empty command) → not started', step: 'tvmcp', machine: 'fresh_pc', tasks: {}, started: [], commands: { 'provision-runtime': '' } },
  { about: 'session page with an unverified cookie: session-check starts by itself', step: 'session', machine: 'session_unverified', tasks: {}, started: [] },
  { about: 'vault page: vault-create is NOT automatic (the folder is the owner\'s choice)', step: 'vault', machine: 'fresh_pc', tasks: {}, started: [] },
  { about: 'claude page: claude-login is NOT automatic (opens a browser)', step: 'claude', machine: 'fresh_pc', tasks: {}, started: [] },
  { about: 'schedule page: scheduler-register is NOT automatic', step: 'schedule', machine: 'scheduler_none', tasks: {}, started: [] },
];
const skipSeq: { op: string; skipped_in: string[]; id?: string; on?: boolean; out: unknown }[] = [];
const sk = (skipped: string[], id: W.StepId, on: boolean) => skipSeq.push({ op: 'applySkip', skipped_in: skipped, id, on, out: W.applySkip(skipped, id, on) });
sk([], 'session', true);
sk(['session'], 'vault', true);
sk(['vault', 'session'], 'session', false);
sk(['schedule', 'junk', 'session', 'session'], 'tradingview', true);
sk(['vault'], 'vault', true);
put('wizard.json', canonLines(
  {
    about: 'The wizard of the old app (src/renderer/setup.ts): page order, which checks sit on which page, when a page is done, skip bookkeeping, the «limited because skipped» notes, and which tasks a page starts by itself.',
    step_ids_mvp: MVP,
    step_ids_legacy: W.stepIdsFor({ legacyUi: true }),
    wizard_steps: W.WIZARD_STEPS,
    auto_tasks: W.AUTO_TASKS,
    schedule_times: W.SCHEDULE_TIMES,
    should_open: { fresh: W.shouldOpenWizard({ setupCompletedAt: 0 }), completed: W.shouldOpenWizard({ setupCompletedAt: 1764000000000 }) },
    finish_patch: W.finishPatch(['schedule', 'junk', 'session'], 1764000000000),
    limitation_keys: { none: W.limitationKeys({ setupSkipped: [] }), session_schedule: W.limitationKeys({ setupSkipped: ['schedule', 'session'] }), junk: W.limitationKeys({ setupSkipped: ['nope', 'tvmcp'] }) },
    nav: { next_of_vault: W.nextStep('vault', MVP), next_of_schedule: W.nextStep('schedule', MVP), prev_of_claude: W.prevStep('claude', MVP), next_of_claude_mvp: W.nextStep('claude', MVP) },
    source: SOURCE,
  },
  {
    pages: checkCases.map((c) => ({ machine: c.name, visible: W.visibleStepIds(c.checks as PreflightCheck[], MVP), steps: stepView(c.checks as PreflightCheck[], MVP) })),
    reconcile_skips: [
      { machine: 'all_green', skipped_in: ['session', 'schedule'], out: W.reconcileSkips(['session', 'schedule'], greenChecks) },
      { machine: 'fresh_pc', skipped_in: ['session', 'schedule', 'claude'], out: W.reconcileSkips(['session', 'schedule', 'claude'], fresh) },
      { machine: 'session_unverified', skipped_in: ['session'], out: W.reconcileSkips(['session'], checkCases.find((c) => c.name === 'session_unverified')!.checks as PreflightCheck[]) },
    ],
    apply_skip: skipSeq,
    auto_fixes: autoCases.map((a) => ({ ...a, out: W.autoFixes(a.step, checkCases.find((c) => c.name === a.machine)!.checks as PreflightCheck[], states(a.tasks) as never, new Set(a.started as never[]), a.commands as never).map((f) => f.task) })),
    empty_diagnosis_visible: [{ out: W.visibleStepIds([], MVP) }],
  },
));

// ------------------------------------------------------------ settings_import.json

/** store.ts readConfig + toSettings, transcribed (three lines; store.ts imports Electron). */
const DOCS_VAULT = '<Documents>\\Strategy Desk';
function oldSettings(parsed: unknown): { patch: unknown; settings: Record<string, unknown> } {
  const raw = parsed && typeof parsed === 'object' ? (parsed as Record<string, unknown>) : null;
  const { hasApiKey: _a, hasOauthToken: _b, hasTvSession: _c, ...defaults } = DEFAULT_SETTINGS;
  const cfg: Record<string, unknown> = { ...defaults, ...legacyConfigPatch(raw), ...(raw ?? {}) };
  const { apiKeyEnc, oauthTokenEnc, tvSessionEnc, ...rest } = cfg;
  return {
    patch: legacyConfigPatch(raw),
    settings: { ...rest, vaultPath: (String(rest.vaultPath ?? '') || '').trim() || DOCS_VAULT, hasApiKey: Boolean(apiKeyEnc), hasOauthToken: Boolean(oauthTokenEnc), hasTvSession: Boolean(tvSessionEnc) },
  };
}
const OWNER = {
  engine: 'claude', codexPath: '', authMode: 'subscription', tvmcpPath: 'C:\\Users\\T\\Projects\\tradingview-mcp', tvmcpRepo: 'https://example.invalid/tvmcp.git', cdpPort: 9223, toolsets: 'hybrid,desktop,session',
  officialMcp: false, knowledgeBase: true, gtRagPath: '', model: '', effort: '', permissionPreset: 'standard', theme: 'dark', uiScale: 1.25, answerLang: 'ru', tvDesktopExe: '', sidebar: { lights: true },
  activeWorkflow: 'strategy-lab', shelfWorkflows: {}, riskRulesPath: '', journalPath: '', fieldNotesPath: '', setupCompletedAt: 1760000000000, setupSkipped: ['schedule'], vaultPath: 'C:\\Users\\T\\Documents\\Trading Strategy Assistant',
  legacyUi: false, tvSessionVerifiedAt: 1764050000000, autoCheckUpdates: true,
  apiKeyEnc: 'djEwAAAA-encrypted-by-safeStorage', tvSessionEnc: 'djEwBBBB-encrypted-by-safeStorage',
};
const CONFIGS: [string, string, unknown][] = [
  ['no_file', 'no config.json at all: a fresh install, defaults (macroMcp bundled)', null],
  ['owner_full', 'the owner\'s 0.2.3 config with every field, written before macroMcp existed → macroMcp user; secrets present but encrypted by Chromium (not portable)', OWNER],
  ['owner_with_macro_mcp', 'a config that already says macroMcp bundled', { ...OWNER, macroMcp: 'bundled' }],
  ['empty_object', '{}: an existing but empty file → macroMcp user, knowledgeBase true (the «existing install» meaning)', {}],
  ['minimal', 'only a vault and a port', { vaultPath: 'D:\\vault', cdpPort: 9224 }],
  ['blank_vault', 'vaultPath of spaces → the Documents default', { vaultPath: '   ' }],
  ['junk_types', 'wrong types everywhere: the old reader keeps them as they are (quirk S1)', { cdpPort: 'abc', theme: 5, uiScale: 'big', vaultPath: null, setupSkipped: 'vault', macroMcp: 'yes', knowledgeBase: 'no', tvmcpPath: 42 }],
  ['unknown_fields', 'fields from the future / the removed legacy desk', { ...OWNER, macroMcp: 'user', futureField: { a: 1 }, profile: { callName: 'x' } }],
  ['json_array', 'a JSON array: typeof object, spread into keys "0","1" (quirk S2)', [1, 2]],
  ['json_string', 'a JSON string: not an object → treated as no file', 'hello'],
  ['json_number', 'a JSON number', 42],
];
// What Rust's import_electron should give (CONTRACTS: vaultPath, tvmcpPath, cdpPort, theme, macroMcp, schedule; secrets never).
const rustView = (s: Record<string, unknown>, rawIsFile: boolean) => {
  const num = (x: unknown, d: number) => (typeof x === 'number' && Number.isFinite(x) ? x : d);
  const str = (x: unknown) => (typeof x === 'string' ? x : '');
  return {
    vault: s.vaultPath === DOCS_VAULT ? null : str(s.vaultPath).trim() || null,
    tvmcp_path: str(s.tvmcpPath),
    cdp_port: Number.isInteger(s.cdpPort) && (s.cdpPort as number) > 0 && (s.cdpPort as number) < 65536 ? s.cdpPort : 9223,
    theme: ['system', 'light', 'dark'].includes(s.theme as string) ? s.theme : 'system',
    scale: num(s.uiScale, 1),
    macro_mcp: s.macroMcp === 'bundled' || s.macroMcp === 'user' ? s.macroMcp : rawIsFile ? 'user' : 'bundled',
    setup_skipped: Array.isArray(s.setupSkipped) ? (s.setupSkipped as unknown[]).filter((x) => typeof x === 'string') : [],
    schedule: { eu: '09:30', us: 'auto', review: '20:00', enabled: null },
    has_tv_session: false,
  };
};
put('settings_import.json', canonLines(
  {
    about: 'Old config.json → settings: legacyConfigPatch (executed) + readConfig/toSettings (transcribed). `old_settings` is what the Electron app ran with; `rust` is what import_electron must give: same values where the old type was right, the Rust default where it was junk (decided, quirk S1), vault null when the old app fell back to Documents (desk-app resolves that path), secrets never imported (has_tv_session false: safeStorage blobs are Chromium-bound — TZ «секреты придётся ввести один раз заново»), schedule: the old config had none (fixed 09:30/16:00/20:00 in the Task Scheduler), so the TZ defaults with US «auto» (window_times) and enabled = null (unknown: read the Task Scheduler, not the file).',
    defaults: DEFAULT_SETTINGS,
    source: SOURCE,
  },
  { cases: CONFIGS.map(([name, about, raw]) => ({ name, about, raw, ...oldSettings(raw), rust: rustView(oldSettings(raw).settings, Boolean(raw && typeof raw === 'object')) })) },
));

// ------------------------------------------------------------ schedule.json (TZ, not the old app)

const wall = (tz: string, t: Date) => {
  const p = Object.fromEntries(new Intl.DateTimeFormat('en-CA', { timeZone: tz, year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).formatToParts(t).map((x) => [x.type, x.value]));
  return { date: `${p.year}-${p.month}-${p.day}`, minutes: Number(p.hour) * 60 + Number(p.minute) };
};
const instantOf = (tz: string, date: string, hhmm: string): Date => {
  for (let off = -14; off <= 14; off++) {
    const t = new Date(Date.parse(`${date}T${hhmm}:00Z`) - off * 3_600_000);
    const w = wall(tz, t);
    if (w.date === date && w.minutes === Number(hhmm.slice(0, 2)) * 60 + Number(hhmm.slice(3))) return t;
  }
  throw new Error('no instant');
};
const hm = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
const usAt = (date: string) => kyivNow(instantOf('America/New_York', date, '09:30')).minutes - 30;
const euAt = (date: string) => kyivNow(instantOf('Europe/Berlin', date, '09:00')).minutes - 30;
const weekday = (d: string) => new Date(`${d}T12:00:00Z`).getUTCDay();
const planned = (d: string) => (weekday(d) === 0 || weekday(d) === 6 ? [] : [{ window: 'eu', at_local: hm(euAt(d)) }, { window: 'us', at_local: hm(usAt(d)) }, { window: 'review', at_local: '20:00' }]);
const DATES = ['2026-03-06', '2026-03-09', '2026-03-27', '2026-03-28', '2026-03-30', '2026-10-23', '2026-10-26', '2026-10-30', '2026-10-31', '2026-11-02', '2026-11-27', '2027-03-15', '2027-03-26', '2027-11-01', '2027-11-08'];
const triggers = new Set<string>();
for (let t = Date.parse('2026-01-01T12:00:00Z'); t < Date.parse('2028-01-01T12:00:00Z'); t += 86_400_000) {
  const d = new Date(t).toISOString().slice(0, 10);
  for (const p of planned(d)) triggers.add(`${p.window} ${p.at_local}`);
}
const dueCases: unknown[] = [];
for (const d of ['2026-10-26', '2026-11-27', '2026-03-09', '2027-11-01']) {
  for (const time of ['09:30', '09:31', '09:59', '10:00', '15:00', '15:05', '16:00', '16:05', '20:00', '20:29', '20:30', '12:00', '23:00']) {
    const k = kyivNow(instantOf('Europe/Kyiv', d, time)).minutes;
    const due = weekday(d) === 0 || weekday(d) === 6 ? null : (planned(d).find((p) => { const at = Number(p.at_local.slice(0, 2)) * 60 + Number(p.at_local.slice(3)); return k >= at && k < at + 30; })?.window ?? null);
    dueCases.push({ utc: instantOf('Europe/Kyiv', d, time).toISOString(), kyiv: `${d} ${time}`, due_window: due });
  }
}
put('schedule.json', canonLines(
  {
    about: 'NOT from the old app (it had fixed 09:30 / 16:00 / 20:00). TZ №21 + CONTRACTS «этап 6»: planned_runs(date) = the minute each window really starts that Kyiv date (Mon–Fri; the US one from window_times: 15:00 in DST-mismatch weeks, else 16:00); the Task Scheduler is registered on the UNION of minutes over the year (`triggers`), so on any day one of the two US triggers is not due; due_window(now) = the window whose planned minute is ≤ now < planned + 30 min (QA\'s proposal for the tolerance: a task the PC woke up late for still runs; the other US trigger is 60 min away, so it never is), else None → `run` exits 0 without work.',
    triggers: [...triggers].sort(),
    tolerance_minutes: 30,
    source: SOURCE,
  },
  { planned_runs: DATES.map((d) => ({ date: d, weekday: weekday(d), out: planned(d) })), due_window: dueCases },
));
console.log('stage6 written to', out);
