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
    WriteDayAnswers { date: NaiveDate, regions: RegionAnswers, note: String },
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
