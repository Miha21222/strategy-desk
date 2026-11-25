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
    WriteHumanRead { date: NaiveDate, region: Region, read: HumanRead },
    WriteDayAnswers { date: NaiveDate, answers: DayAnswers },
    Chat(ChatCommand),          // Send { chat_id, text } | Stop { chat_id } | New { kind }
    CheckStatus,
    LaunchTradingView,
    RunTask(TaskId),            // ids, never commands (rule of the old app)
}

pub struct Snapshot {           // cheap to clone: Arc inside
    pub day: Option<Arc<desk_core::day::Day>>,
    pub days_index: Arc<Vec<DayMark>>,
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
