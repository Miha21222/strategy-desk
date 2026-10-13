// Stages the «vault template»: the minimal Obsidian-vault folder a trader needs for the
// macro-review cycle (skill macro-review + indices-kb tools + configs), taken from the
// owner's vault and scrubbed of everything personal. The installer ships the result and
// the app copies it into the trader's vault on first start.
//
//   node scripts/stage-vault.mjs [--src <vault>]
//
// Source: --src, else env SD_VAULT_SRC, else the owner's vault path below. Output:
// resources/vault-template/ (deleted and rebuilt every run; it is build output and is
// gitignored — the ignore entry lives in .gitignore, not here). Deliberately strict:
// a missing source file or a scrub hit (owner's paths, account names, secrets, Notion
// links) fails the build, because the alternative is a stranger's installer carrying
// the owner's data. Node 22, ESM, no dependencies beyond node:*.
import { execFileSync } from 'node:child_process';
import {
  cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const TEMPLATE_VERSION = '2'; // 2: + .claude/skills/humanizer (2026-10-13)
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'resources', 'vault-template');
const DEFAULT_SRC = 'C:\\Users\\Admin\\Documents\\Obsidian Notes\\Trading\\Trading Strategy Assistant';

const SKILL = '.claude/skills/macro-review/SKILL.md';
/** The humanizer skill (MIT, github.com/blader/humanizer) the macro-review skill sends every paragraph through, with its license and the Russian notes. */
const HUMANIZER = ['.claude/skills/humanizer/SKILL.md', '.claude/skills/humanizer/LICENSE', '.claude/skills/humanizer/ru.md', '.claude/skills/humanizer/glossary.md'];
/** Notes the skill cites by number; copied verbatim (resolved by their numeric prefix). */
const NOTE_PREFIXES = ['20 - ', '22 - ', '23 - '];
const TOOLS = 'indices-kb/tools';
/** macro_review.py and what it reaches, transitively (grep `^from|^import` in the tools). */
const TOOL_FILES = [
  'common.py', 'macro_review.py', 'calendar_check.py', 'regime.py', 'scale.py', 'levels.py',
  'structure.py', 'plan_build.py', 'bias.py', 'macro_shots.py', 'tv_plan_shots.py',
  'macro_review.ps1', 'macro_schedule.ps1', 'README.md',
];
/** Local modules that may be imported by the tools; anything imported but not shipped fails the build. */
const TOOL_MODULES_NOT_SHIPPED = new Set([
  'ib_sentinel', 'journal', 'account_check', 'tv_replay', 'tv_study', 'tv_view', 'tv_price', 'tv_shot', 'brief', 'smt',
]);
const CONFIG = 'indices-kb/config';
const CONFIG_FILES = ['scale.json', 'rules.json', 'day_rules.json', 'earnings.json'];
const EMPTY_DIRS = ['indices-kb/state', 'indices-kb/journal', 'indices-kb/shots', 'indices-kb/inbox', 'indices-kb/cache', 'Макро-обзоры'];
/** Journal markdown files: header only (text up to the first blank line of the real file). */
const JOURNAL_HEADERS = ['indices-kb/journal/lessons.md', 'indices-kb/journal/ideas.md'];

/**
 * Exact-text rewrites applied to copied files so verbatim copies pass the scrub. Each entry
 * must match, or the build fails: a source change that drops the phrase must be looked at.
 */
const REWRITES = {
  'indices-kb/tools/common.py': [
    ['(checkout of Miha21222/tradingview-mcp)', '(checkout of tradingview-mcp)'],
  ],
  'indices-kb/tools/plan_build.py': [
    ['(SKILL.md ib-es, section «План в Notion»)', '(SKILL.md ib-es, the plan section)'],
  ],
  'indices-kb/tools/README.md': [
    ['→ агент смотрит PNG → Notion. Скилл `.claude/skills/ib-plan/SKILL.md`.', '→ агент смотрит PNG. Скилл `.claude/skills/ib-plan/SKILL.md` (в шаблоне его нет).'],
    ['разделы «План в Notion» и «Терминология».', 'раздел «Терминология».'],
  ],
};

/** Scrub: a hit in any output file fails the build (path:line is printed). */
const SCRUB = [
  { name: 'owner home path', re: /C:\\+Users\\+Admin/i },
  { name: 'owner account name', re: /Kompotus/i },
  { name: 'owner github name', re: /Miha21222/i },
  { name: 'TV_SESSIONID with a value', re: /TV_SESSIONID\s*=\s*["']?[A-Za-z0-9]{6,}/ },
  { name: 'Notion', re: /notion/i },
  { name: 'collection:// link', re: /collection:\/\//i },
  { name: 'hive-mind', re: /хайв|hive-mind|trading-hive/i },
  { name: 'wmux', re: /wmux/i },
  { name: 'gt-rag', re: /gt-rag|gt-database/i },
];
const SCRUB_SKIP_EXT = new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp', '.ico']);

function die(message) {
  console.error(`[stage-vault] ${message}`);
  process.exit(1);
}

function sourceDir() {
  const i = process.argv.indexOf('--src');
  const given = i >= 0 ? process.argv[i + 1] : '';
  const dir = path.resolve((given || process.env.SD_VAULT_SRC || '').trim() || DEFAULT_SRC);
  if (!existsSync(path.join(dir, SKILL))) die(`no vault at ${dir} (expected ${SKILL}). Pass --src <vault> or set SD_VAULT_SRC.`);
  return dir;
}

function must(src, rel) {
  const p = path.join(src, rel);
  if (!existsSync(p)) die(`required source file is missing: ${rel}`);
  return p;
}

function noteByPrefix(src, prefix) {
  const names = readdirSync(src).filter((n) => n.startsWith(prefix) && n.endsWith('.md'));
  if (names.length !== 1) die(`expected exactly one note "${prefix}…md" in ${src}, found ${names.length}`);
  return names[0];
}

function sourceStamp(src) {
  try {
    return execFileSync('git', ['-C', src, 'log', '-1', '--format=%H %cI'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();
  } catch {
    const latest = [SKILL, ...TOOL_FILES.map((f) => `${TOOLS}/${f}`)]
      .map((f) => statSync(path.join(src, f)).mtime)
      .reduce((a, b) => (b > a ? b : a), new Date(0));
    return `mtime ${latest.toISOString()}`;
  }
}

/** All `from X import` / `import X` module roots of a tool file, top-level or indented. */
function importedModules(text) {
  const mods = new Set();
  for (const m of text.matchAll(/^\s*(?:from\s+([\w.]+)\s+import|import\s+([\w.]+))/gm)) {
    mods.add((m[1] || m[2]).split('.')[0]);
  }
  return mods;
}

function checkToolImports(src) {
  const shipped = new Set(TOOL_FILES.filter((f) => f.endsWith('.py')).map((f) => f.slice(0, -3)));
  for (const f of TOOL_FILES) {
    if (!f.endsWith('.py')) continue;
    const text = readFileSync(path.join(src, TOOLS, f), 'utf8');
    for (const mod of importedModules(text)) {
      if (TOOL_MODULES_NOT_SHIPPED.has(mod) || (existsSync(path.join(src, TOOLS, `${mod}.py`)) && !shipped.has(mod))) {
        die(`${TOOLS}/${f} imports local module "${mod}" that the template does not ship; add it to TOOL_FILES`);
      }
    }
  }
}

function headerOf(text) {
  const norm = text.replace(/^\uFEFF/, '').replace(/\r\n/g, '\n');
  const i = norm.indexOf('\n\n');
  const head = i >= 0 ? norm.slice(0, i) : norm;
  // keep the title and the explanatory paragraph that follows it (ends at the next blank line)
  const rest = i >= 0 ? norm.slice(i + 2) : '';
  const j = rest.indexOf('\n\n');
  const para = j >= 0 ? rest.slice(0, j) : rest;
  return `${head}\n\n${para.trim()}\n\n`;
}

/** Section «## 2. …» of the owner's AGENTS.md, without the hive-mind cross-reference. */
function styleSection(src) {
  const text = readFileSync(must(src, 'AGENTS.md'), 'utf8').replace(/\r\n/g, '\n');
  const m = /^## 2\. [^\n]*\n([\s\S]*?)(?=^## )/m.exec(text);
  if (!m) die('AGENTS.md: section "## 2." not found');
  let body = m[1].trim();
  body = body.replace(/\s*Таблица конфликтов:[^\n]*/g, '');
  return body;
}

const files = [];
function record(rel) { files.push(rel.replace(/\\/g, '/')); }

function copyFile(src, rel, destRel = rel) {
  const from = must(src, rel);
  const to = path.join(out, destRel);
  mkdirSync(path.dirname(to), { recursive: true });
  let text = readFileSync(from);
  const rules = REWRITES[destRel];
  if (rules) {
    let s = text.toString('utf8');
    for (const [a, b] of rules) {
      if (!s.includes(a)) die(`${rel}: expected text for rewrite not found: ${JSON.stringify(a)}`);
      s = s.split(a).join(b);
    }
    text = Buffer.from(s, 'utf8');
  }
  writeFileSync(to, text);
  record(destRel);
}

function writeText(rel, text) {
  const to = path.join(out, rel);
  mkdirSync(path.dirname(to), { recursive: true });
  writeFileSync(to, text, 'utf8');
  record(rel);
}

function settingsTemplate() {
  // Neutral values = the defaults in common.py settings(); the trader fills in his own.
  return `${JSON.stringify({
    _comment: 'Настройки инструментов. Заполни под себя: symbol_live — твой живой фид (CFD/фьючерс у твоего брокера), prop — твой счёт: name (брокер/проп), account (размер счёта), risk_pct (риск на сделку, %), daily_loss_limit_pct (дневной лимит, %). Баланс и номера счетов сюда не писать.',
    symbol_ref: 'CME_MINI:ES1!',
    symbol_live: 'PEPPERSTONE:US500',
    symbol_smt: 'CME_MINI:NQ1!',
    prop: { name: '', instrument: 'SPX500', contract_size: 50, account: 10000, risk_pct: 1.0, daily_loss_limit_pct: 2.0 },
    adr_days: 5,
    stale_seconds_live: 180,
  }, null, 2)}\n`;
}

function agentsMd(style, notes) {
  return `# AGENTS.md — онбординг агента (Claude Code и любой другой)

Это папка трейдера в его Obsidian-vault. Здесь живут заметки, настройки и скрипты макро-обзора перед открытием Европы и США и итога дня. Ты — ассистент по анализу. **Сделки не открываешь, не ведёшь, не закрываешь.** Выход агента — анализ, стрелка, план сессии, уроки, код.

Прочитай этот файл целиком, затем скилл \`.claude/skills/macro-review/SKILL.md\` — он описывает весь цикл.

## 1. Что здесь

| Что | Где | Зачем |
|---|---|---|
| Скилл macro-review | \`.claude/skills/macro-review/SKILL.md\` | обзор 09:30 (Европа: FDAX) и 16:00 (США: ES) по Киеву, итог дня 20:00 |
| Правила IB-сетапов | \`${notes[0]}\` | что торгуем и когда не торгуем |
| Шкала LONG / SHORT | \`${notes[1]}\` | как считается стрелка, доли факторов, цикл обратной связи |
| Strategy Desk | \`${notes[2]}\` | как приложение и агент дополняют трейдера |
| Инструменты | \`indices-kb/tools/\` (\`README.md\` там) | сбор фактов, шкала, фаза рынка, снимки уровней |
| Настройки | \`indices-kb/config/\` | \`settings.json\` — счёт и фиды трейдера, \`scale.json\` — доли факторов, \`rules.json\`, \`day_rules.json\`, \`earnings.json\` |
| Состояние дня | \`indices-kb/state/\` | JSON-файлы обзоров (\`macro_*\`, \`scale_*\`, \`regime_*\`, \`review_*\`) — их читает Strategy Desk |
| Журнал | \`indices-kb/journal/\` | \`lessons.md\` — уроки вечерних разборов, \`ideas.md\` — идеи на проверке, CSV со счётом факторов |
| Снимки | \`indices-kb/shots/\` | картинки уровней с TradingView по дням |
| Кэш баров | \`indices-kb/cache/\` | CSV, накапливается сам; можно удалить — скачается заново |
| Заметки обзоров | \`Макро-обзоры/\` | \`<дата> Европа.md\`, \`<дата> США.md\`, \`<дата> Итог дня.md\` |

## 2. Язык и стиль

${style}

## 3. Окружение

- Windows, PowerShell. Python для скриптов: \`MACRO_PYTHON\`, если задан (Strategy Desk ставит его на свой venv с pandas, smartmoneyconcepts, tvmcp), иначе \`python\` с PATH.
- Env \`TV_SESSIONID\` (cookie TradingView) — выставляет Strategy Desk. **Значение никуда не писать.**
- TradingView Desktop с CDP: \`TV_CDP_URL\` (по умолчанию \`http://127.0.0.1:9223\`). Нет TradingView — обзор идёт без картинок.
- Все скрипты запускаются из \`indices-kb/tools/\`: \`python macro_review.py collect --region eu\` и так далее по скиллу.
- Запуск по расписанию — Планировщик Windows («Trading Macro EU», «Trading Macro US», «Trading Review»), регистрирует Strategy Desk или \`macro_schedule.ps1\`.

## 4. Скиллы

- \`macro-review\` — единственный скилл в этой папке: \`/macro-review eu\`, \`/macro-review us\`, \`/macro-review review\`.

## 5. Жёсткие правила

1. Не торговать и не имитировать торговлю. Анализ и вердикт.
2. Числа в вердиктах и планах — только из state-файлов и баров. Не из головы.
3. Секреты (cookie, токены, номера счетов, баланс) не писать в файлы, логи, коммиты, чаты.
4. Не выдумывать уровни — только из данных; не перерисовывать то, что уже показывают индикаторы трейдера.
5. Доли в \`config/scale.json\` не менять без явного «да» трейдера.
6. Решение по сделке — всегда за трейдером. Агент говорит, что видит, и почему; не уговаривает.
`;
}

function claudeMd() {
  return `# CLAUDE.md — Strategy Desk vault

Общий онбординг — \`AGENTS.md\` (читать первым). Здесь только специфика Claude Code.

- Скилл \`macro-review\` в \`.claude/skills/macro-review/SKILL.md\`: \`/macro-review eu\` (FDAX, 09:30), \`/macro-review us\` (ES, 16:00), \`/macro-review review\` (итог дня 20:00). Заметки — в \`Макро-обзоры/\`, состояние — \`indices-kb/state/\`, уроки — \`indices-kb/journal/lessons.md\`.
- Python для скриптов: \`MACRO_PYTHON\` (его ставит Strategy Desk), иначе \`python\` с PATH. Env \`TV_SESSIONID\` и \`TV_CDP_URL\` — тоже от Strategy Desk; их значения никуда не писать.
- Содержательные тексты для трейдера — простой русский по правилам \`AGENTS.md\` §2: сначала вывод, потом одна причина.
`;
}

function readmeMd() {
  return `# Папка Strategy Desk

Это рабочая папка твоего макро-обзора. Её создал Strategy Desk; Obsidian открывает её как vault, Claude Code — как проект (\`CLAUDE.md\`, \`AGENTS.md\`).

## Что здесь

- \`Макро-обзоры/\` — сюда ложатся обзоры: \`<дата> Европа.md\` (09:30), \`<дата> США.md\` (16:00), \`<дата> Итог дня.md\` (20:00). Читай их здесь или в приложении.
- \`indices-kb/journal/\` — \`lessons.md\` (уроки вечерних разборов), \`ideas.md\` (идеи на проверке) и CSV со счётом факторов. Это память цикла — не удаляй.
- \`indices-kb/config/\` — настройки. \`settings.json\` заполни под себя (фид, размер счёта, риск). \`scale.json\` — доли факторов шкалы: менять по итогам месяца, не на эмоциях.
- \`indices-kb/state/\` — машинные файлы обзоров, их читает приложение. Не редактируй руками.
- \`indices-kb/cache/\`, \`indices-kb/shots/\` — кэш баров и картинки уровней. Можно чистить, восстановятся.
- \`indices-kb/tools/\` — скрипты обзора. Не трогай, обновляет приложение.
- \`.claude/skills/macro-review/\` — инструкция агенту. Заметки \`20 - …\`, \`22 - …\`, \`23 - …\` — правила и устройство шкалы, на них ссылается скилл.

## Что не удалять

\`indices-kb/journal/\`, \`indices-kb/config/\`, \`indices-kb/tools/\`, \`.claude/\`, заметки 20 / 22 / 23 и \`AGENTS.md\` / \`CLAUDE.md\`.

## Секреты

Cookie TradingView (\`TV_SESSIONID\`) и всё прочее приложение держит у себя и передаёт скриптам через окружение. В файлы этой папки их не писать.
`;
}

function scrub() {
  const hits = [];
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) { walk(p); continue; }
      if (SCRUB_SKIP_EXT.has(path.extname(e.name).toLowerCase())) continue;
      const lines = readFileSync(p, 'utf8').split(/\r?\n/);
      lines.forEach((line, i) => {
        for (const s of SCRUB) if (s.re.test(line)) hits.push(`${path.relative(out, p)}:${i + 1}: ${s.name}: ${line.trim().slice(0, 120)}`);
      });
    }
  };
  walk(out);
  if (hits.length) {
    for (const h of hits) console.error(`[stage-vault] scrub hit ${h}`);
    die(`${hits.length} scrub hit(s); the template was left in place for inspection`);
  }
}

function main() {
  const src = sourceDir();
  console.log(`[stage-vault] source ${src}`);
  checkToolImports(src);
  const notes = NOTE_PREFIXES.map((p) => noteByPrefix(src, p));
  for (const f of [SKILL, ...HUMANIZER, ...notes, ...TOOL_FILES.map((f) => `${TOOLS}/${f}`), ...CONFIG_FILES.map((f) => `${CONFIG}/${f}`),
    `${CONFIG}/settings.json`, ...JOURNAL_HEADERS, 'AGENTS.md']) must(src, f);

  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });

  copyFile(src, SKILL);
  for (const f of HUMANIZER) copyFile(src, f);
  for (const n of notes) copyFile(src, n);
  for (const f of TOOL_FILES) copyFile(src, `${TOOLS}/${f}`);
  for (const f of CONFIG_FILES) copyFile(src, `${CONFIG}/${f}`);
  writeText(`${CONFIG}/settings.json`, settingsTemplate());
  for (const d of EMPTY_DIRS) writeText(`${d}/.keep`, '');
  for (const j of JOURNAL_HEADERS) writeText(j, headerOf(readFileSync(must(src, j), 'utf8')));
  writeText('AGENTS.md', agentsMd(styleSection(src), notes));
  writeText('CLAUDE.md', claudeMd());
  writeText('README.md', readmeMd());

  scrub();

  const manifest = {
    version: TEMPLATE_VERSION,
    built_at: new Date().toISOString(),
    source_commit_or_date: sourceStamp(src),
    files: files.sort(),
  };
  writeText('template.json', `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`[stage-vault] ${files.length} files -> ${out}`);
}

main();
