# Контракты между крейтами

Меняет только тимлид. Агент, которому нужно другое, пишет это в отчёте.

## desk-core (чисто, без I/O)

Модули и что в них переезжает из TS:

| Модуль | Из TS | Публичное |
| --- | --- | --- |
| `vault` | типы `src/shared/macro.ts` | структуры файлов `state/*` (`ScaleFile`, `MacroFile`, `ReviewFile`, `HumanRead`, `DayAnswers`, `Remember`, `Drift`, `Week`, `Ideas`, `Shots`, …) с `serde`, неизвестные поля не роняют разбор |
| `normalize` | `normalize*` | `fn normalize_<kind>(serde_json::Value) -> <Type>` — та же терпимость к старым версиям и пустым полям, что в TS |
| `day` | `loadDay()` из `src/main/macro-state.ts` (чистая часть) | `DayInput` (имя файла → байты) → `Day` (то, что отдаёт `loadDay`). `Day` сериализуется в тот же JSON, что `scripts/preview/dump.ts` — это и есть golden-тест |
| `zones`, `gauge` | зоны ±15/±40 %, геометрия спидометра | чистые функции от `ScaleConfig` |
| `windows` | состояния окон 09:30/16:00/20:00, ТЗ №21 | `fn window_state(region, now: DateTime<Utc>, day: &Day) -> WindowState`; часы окон из `chrono-tz` (Europe/Berlin, America/New_York) |
| `stats` | CSV-статистика | `fn factor_stats(csv: &str) -> FactorStats` |
| `names` | построение имён файлов | `fn state_file(kind, date: NaiveDate, region) -> String` — только из проверенной даты |

Время: `desk-core` никогда не вызывает `Utc::now()`. `now` приходит аргументом.

## desk-app ↔ desk-ui

UI не ждёт ничего на кадре. Он отправляет `Command` и рисует последний `Snapshot`.

```rust
// desk-app
pub enum Command {
    OpenDay(NaiveDate),
    Today,
    WriteHumanRead { date: NaiveDate, region: Region, side: HumanSide, before_scale: bool },
    WriteDayAnswers { date: NaiveDate, regions: desk_core::vault::DayAnswerRegions, note: String },
    LoadShot(String),            // vault-relative; bytes land in Snapshot.shots
    Chat(ChatCommand),          // Send { chat_id, text } | Stop { chat_id } | New { kind }
    CheckStatus,
    LaunchTradingView,
    RunTask(TaskId),            // ids, never commands (rule of the old app)
}

pub struct Snapshot {           // cheap to clone: Arc inside
    pub day: Option<Arc<desk_core::day::Day>>,
    pub days_index: Arc<Vec<DayMark>>,
    pub index: Arc<desk_core::day::MacroIndex>,   // carries no-vault / no-state-dir
    pub shots: Arc<HashMap<String, ShotState>>,   // Ready(LoadedShot) | Refused(reason)
    pub chats: Arc<ChatsView>,
    pub status: Arc<StatusLights>,
    pub tasks: Arc<TasksView>,
    pub errors: Arc<Vec<UserError>>,   // i18n key + vars, never a sentence
    pub rev: u64,                      // bumps on every change
}

pub struct AppHandle { /* send(Command); snapshot() -> Snapshot; set_repaint(Box<dyn Fn() + Send + Sync>) */ }
```

`desk-app` будит UI через колбэк `set_repaint` (в UI — `ctx.request_repaint()`), только когда `rev` изменился.

## Драйвер Claude (desk-app::claude)

Процесс: `claude -p --input-format stream-json --output-format stream-json --verbose [--resume <id>]`, cwd = vault, инструменты `MACRO_ALLOWED`, приложение `prompt/macro-review.md` или `prompt/ask.md`, окружение из `macro_env` (порт `src/main/macro-env.ts`).

```rust
pub enum DriverEvent {
    SessionStarted { session_id: String },
    TextDelta { text: String },
    ToolCall { id: String, name: String, summary: String },
    ToolResult { id: String, ok: bool },
    Usage { input: u64, output: u64, cost_usd: Option<f64> },
    Done { stop: StopReason },
    Error { key: String, detail: String },
}
```

Строка stream-json, которую драйвер не понял, — `tracing::warn` и пропуск, не падение. Обрыв процесса = `Error` + `Done`.

## Запись в vault

Ровно два файла владельца (`human_read_<D>_<region>.json`, `day_answers_<D>.json`): временный файл рядом + rename. Больше Desk в vault ничего не пишет.

## Хуки для замеров (QA · перформанс, docs/rust/PERF.md)

| Хук | Где | Что делает |
| --- | --- | --- |
| `--bench-first-frame` | desk-ui | одна строка JSON `{"first_frame_ms", "rss_mb"}` после первого кадра, выход 0 |
| `DESK_FRAME_STATS=1` | desk-ui | p50/p95/p99/max времени кадра в stderr каждые 10 с |
| `--bench-switch N` | desk-ui | N переключений дня/экрана, `{"switch_ms":[…]}`, выход 0 |
| `DESK_BENCH_AUTOCHAT=1` | desk-ui | после первого кадра сам отправляет одно сообщение в чат |
| `DESK_CLAUDE_BIN=<exe>` | desk-app | запускать этот exe вместо `claude` (для `fake-claude`) |
| `DESK_BENCH_WATCH=1` | desk-app | строка в stderr с `t_unix_ms` на каждый новый `rev` |

## Решения тимлида (25.11)

- Проверка года 1000–9999 для имён файлов владельца переезжает в `desk_core::names`; в desk-app второй копии нет.
- Метки полосы дней (`DayMark`) пока строит desk-app из `MacroIndex`.
- Точный отрезок окна США в недели рассинхрона часов: 15:00 / 15:30 / 17:30 по Киеву (как в golden `windows.json`, ответ `tz`).

## Чат (этап 5)

```rust
pub enum ChatCommand {
    New { kind: ChatKind },                       // Ask → «+ Новый чат»
    OpenMacro { date: NaiveDate },                // один макро-чат на день, title = desk_core::prompts::macro_chat_title
    Open { chat_id: ChatId },                     // любой чат из списка
    SendMacro { date: NaiveDate, text: String },  // в макро-чат дня (например, discuss_prompt)
    Send { chat_id: ChatId, text: String },
    Run { window: WindowId, date: NaiveDate },    // «Запустить / Пересобрать обзор»: OpenMacro + Send(run_command)
    Stop { chat_id: ChatId },
    Rename { chat_id: ChatId, title: String },
    Delete { chat_id: ChatId },                   // только из списка чатов, с подтверждением в UI
}

pub struct ChatsView {
    pub list: Vec<ChatMeta>,                      // новые сверху
    pub open: Option<ChatId>,
    pub transcripts: HashMap<ChatId, Arc<Transcript>>,   // только открытые/последние, не все
}
pub struct ChatMeta { id, kind, title, date: Option<NaiveDate>, created_at, updated_at, session_id: Option<String>, busy: bool, cost_usd: f64 }
pub struct Transcript { pub items: Vec<ChatItem> }       // User{text,at} | Assistant{text,at,live:bool} | Tool{id,name,summary,ok:Option<bool>} | Notice{key,vars}
```

- Хранение: `%APPDATA%\strategy-desk-rs\chats\<id>.json` (meta + transcript), атомарно; чтение старых чатов Electron — не нужно.
- Один процесс `claude` на чат, `--resume <session_id>` при следующем запуске Desk; `busy` чат не принимает `Send` (UI блокирует).
- Макро-чат и «Новый чат» — `macro_env` + `prompt/macro-review.md` / `prompt/ask.md` (через `include_str!`), cwd = vault, инструменты `MACRO_ALLOWED`.
- Где взять `claude`: `DESK_CLAUDE_BIN`, затем `claude` в PATH; не нашли — `Notice{key:"chat.error.no_claude"}` (мастер настройки — этап 6).
- Стрим: `TextDelta` дописывается в последний `Assistant{live:true}`; снимок обновляется не чаще 30 раз в секунду.

## Подключения, мастер, настройки, расписание (этап 6)

Порядок источников правды — старое приложение: `src/shared/preflight.ts` (проверки и их порядок), `src/main/preflight.ts` (пробы),
`src/shared/tasks.ts` + `src/main/tasks.ts` (задачи), `src/renderer/setup.ts` (страницы мастера), `src/main/status.ts`, `src/main/tv-launch.ts`,
`src/main/runtime-spec.ts`, `src/main/vault-template.ts`, `src/shared/defaults.ts` (настройки), `src/main/macro-env.ts`.
Из ТЗ: пункты 16–20 и 23. Без базы курса (gt-rag), без официального MCP, без Codex, без legacy.

desk-core (чисто):
- `settings`: `Settings { vault, claude_bin, tvmcp_path, cdp_port, theme, scale, macro_mcp, schedule: { eu, us, review, enabled } , setup_skipped }`,
  `serde` с дефолтами и терпимостью к старым полям; `import_electron(old_config_json) -> Settings` (vaultPath, tvmcpPath, cdpPort, theme, macroMcp, расписание; секреты не переносятся).
- `preflight`: `CheckId` (vault, claude, claude_auth, uv, tvmcp, tradingview, cdp, tv_session, scheduler, timezone), `Facts` → `Vec<Check { id, state: Ok|Warn|Fail|Skipped, detail_key, vars, fix: Option<Fix> }>`;
  `Fix` = `Task(TaskId)` | `Copy { text, explain_key }` | `LaunchTradingView` | `PickFolder` | `SetCdpPort(u16)` — тот же список «что делает само», что в старом CLAUDE.md.
- `schedule`: `fn planned_runs(date, settings) -> Vec<PlannedRun { window, at_local }>` и `fn due_window(now, settings) -> Option<WindowId>` — Планировщик будит Desk в обе возможные минуты (например 15:00 и 16:00 для США), `run` сам решает по `windows::window_times`, его ли это окно, иначе выходит с кодом 0 без работы.
- `wizard`: страницы `vault → claude → tvmcp → tradingview → session → schedule`, правила пропуска и «что ослабнет без шага».

desk-app (I/O):
- `settings_store`: `%APPDATA%\strategy-desk-rs\settings.json` атомарно; при первом старте — `import_electron` из `%APPDATA%\strategy-desk\config.json`, если он есть.
- `secrets`: sessionid TradingView в Диспетчере учётных данных (`keyring`, сервис `strategy-desk`, ключ `tv_session`); наружу только `has_tv_session`.
- `status`: лампочки `StatusLights { tradingview, cdp, claude, vault }` по событию (старт, фокус окна, за 5 мин до окна) и по кнопке; TradingView через `sysinfo`, CDP — один GET `/json/version` с таймаутом 1,5 с, Claude — `claude --version` + проверка входа.
- `tv_launch`: запуск TradingView Desktop (MSIX) с `--remote-debugging-port`, никогда не убивает запущенный.
- `tasks`: `TaskId` (claude_install, claude_login, provision_runtime, vault_create, session_check, scheduler_register, scheduler_unregister); UI называет id, не команду; вывод построчно в снимок; ввод в stdin только для claude_login.
- `runtime`: среда Python для tvmcp и скриптов vault — `uv sync --frozen` в `%APPDATA%\strategy-desk-rs\py` (внешний режим: готовый checkout + uv в PATH — как `preferExternal`).
- `vault_template`: создание vault из шаблона, только недостающие файлы, ничего не перезаписывать; шаблон вшивается `include_dir` из `resources/vault-template`, если он подготовлен (`scripts/stage-vault.mjs`), иначе пустой — это этап 7.
- `scheduler`: задачи Планировщика Windows через `schtasks`/PowerShell на `strategy-desk.exe run --window <eu|us|review>` в минуты из `planned_runs`; cookie в реестре больше не пишется.
- `run`: безоконный режим `strategy-desk run --window eu|us|review [--date D]`: настройки + sessionid из keyring + `macro_env`, `due_window`, затем `claude -p` с `run_command`, лог в `%APPDATA%\strategy-desk-rs\logs\run-<D>-<w>.log`, код выхода 0/1.

desk-ui: лампочки в боковой панели с «Запустить TradingView», страница «Настройки» (vault с выбором папки, тема, масштаб, sessionid — поле + «Проверить», macroMcp, расписание, «Очистить рантайм»), мастер первого запуска (6 страниц, у каждой ошибки одна кнопка «Исправить»), «Диагностика» — тот же список проверок, вывод задач.
