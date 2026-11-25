# Перформанс: как меряем

Методика к таблице «Метрики» из `docs/rust/TEAM.md`. Пишет и меняет QA · перформанс. Инструменты лежат в `rust/crates/desk-bench/` и `scripts/rust/`, отчёты в `docs/rust/perf/`.

## Где цифры честные

Считаются только замеры `scripts/rust/bench.ps1` на ноутбуке владельца. У таких отчётов `"representative": true`. Облачный Linux-бокс делит CPU с другими, у него нет окна и нет Defender, поэтому его отчёт (`perf-sample`, `"representative": false`) показывает только одно: стенд работает. `report.py` и дашборд пишут это крупно.

Окно 16:25–18:30 по Киеву закрыто. `bench.ps1` в нём не стартует, а если прогон до него дотянется, отказывается заранее. Начатый прогон останавливается за 1–2 минуты до 16:25 и пишет в отчёт `ABORTED`. Обойти можно только явным `-Force`. TradingView скрипт не запускает, не закрывает и не считает. Даже если Desk сам запустил его, процесс `TradingView*` исключается из дерева процессов.

## Команды владельца

Сначала закрыть Strategy Desk: при запущенном экземпляре скрипт отказывается работать. Мышь и клавиатуру во время прогона не трогать. Ноутбук на зарядке.

```powershell
# База «до»: установленный Electron 0.2.3 (exe находится сам)
powershell -ExecutionPolicy Bypass -File scripts\rust\bench.ps1 -Kind electron

# «После»: Rust-сборка из этой ветки
cd rust; cargo build --release -p desk-ui -p desk-bench; cd ..
powershell -ExecutionPolicy Bypass -File scripts\rust\bench.ps1 -Kind rust -Exe rust\target\release\strategy-desk.exe

# Сводка «до / после» и summary.json для дашборда
python scripts\rust\report.py docs\rust\perf
```

Полный прогон идёт около 15 минут: 10 холодных стартов по ~45 с, 11 тёплых, 3 простоя по 80 с. Отчёт сохраняется в `docs/rust/perf/<дата>-<commit>.json`, у Electron вместо коммита стоит `electron-<версия>`. Если запустить PowerShell от администратора, холодный старт станет честнее (см. ниже), но это не обязательно.

Полезные ключи: `-Runs 10`, `-IdleRuns 3`, `-IdleSeconds 60`, `-SettleSeconds 10`, `-ColdMethod auto|admin|pressure|none`, `-Installer <путь>`, `-SkipCold/-SkipWarm/-SkipIdle`, `-StreamFrames`, `-SoakHours 24`, `-Out <файл>`. Ключ `-SelfTest` проверяет чистые функции (статистику, окно, разбор кадров, JSON) без Windows и запускается и под `pwsh` на Linux.

## Метрики

| id | Что меряем: от события до события | Повторы | Сравниваем | Цель | Чем |
| --- | --- | --- | --- | --- | --- |
| `cold_start_ms` | `Process.Start` → первый кадр; перед каждым запуском вытесняется файловый кэш | 10 | медиана | ≤ 300 | bench.ps1 |
| `warm_start_ms` | то же сразу после прошлого запуска; первый запуск выброшен как прогрев | 10 (+1) | медиана | ≤ 150 | bench.ps1 |
| `idle_working_set_mb` | сумма Working Set по дереву процессов через 10 с успокоения + 60 с покоя | 3 | медиана | ≤ 80 | bench.ps1 |
| `idle_private_mb` | то же, Private Bytes (`PrivateMemorySize64`) | 3 | медиана | ≤ 80 | bench.ps1 |
| `idle_cpu_pct` | CPU дерева за те же 60 с / время / число логических CPU, как в Диспетчере задач | 3 | медиана | ≤ 1 % | bench.ps1 |
| `idle_frames_per_min` | кадры из `DESK_FRAME_STATS` за минуту покоя (в идеале около 0) | 3 | медиана | — | bench.ps1, только Rust |
| `frame_p50/p95/p99_ms` | время кадра по окнам `DESK_FRAME_STATS` по 10 с, пока fake-claude стримит 10 тыс. событий | все окна | p50: медиана окон; p95, p99: худшее окно | ≤ 8 / 16 / 33 | bench.ps1 `-StreamFrames` |
| `day_load_ms` | чтение ~24 файлов дня из тёплого кэша + разбор | 30 (+3) | медиана | ≤ 30 | criterion `day_load`, perf-sample |
| `day_load_real_size_ms` | то же, `macro_*.json` по ~2000 строк | 30 | медиана | ≤ 30 | то же |
| `vault_x10_parse_ms`, `vault_x10_list_days_ms` | все 501 день синтетики ×10 / список дней | 5 / 30 | медиана | — (стресс) | то же |
| `stream_parse_10k_ms` | 10 тыс. строк stream-json → `serde_json::Value` | 20 | медиана | — (нижняя граница) | criterion `stream_parse` |
| `stream_pipe_10k_ms` | запуск fake-claude, все 10 тыс. строк через трубу | 10 | медиана | — | perf-sample |
| `watch_latency_ms` | `done` от slow-writer → новый снимок в desk-app | 20 | p95 | ≤ 350 | хук desk-app, см. ниже |
| `switch_ms` | команда «другой день / экран» → кадр с ним | 20 | p95 | ≤ 100 | хук desk-ui, см. ниже |
| `exe_mb`, `installer_mb` | размер файла | 1 | — | ≤ 15 / 20 | bench.ps1, perf-sample |
| `install_dir_mb` | все файлы папки установки (у Electron 669 МБ) | 1 | — | — | bench.ps1 |
| `soak_private_growth_pct` | Private Bytes дерева раз в 5 мин; рост от отметки 10 мин до последней | 1 | — | ≤ 5 % | bench.ps1 `-SoakHours 24` |

Перцентили считаются по ближайшему рангу, без интерполяции. Из 10 запусков p95 — это самый долгий из них. Так считают и Rust (`stats.rs`), и `bench.ps1`, и `report.py`. Медиана чётного числа замеров — среднее двух средних. Статус `pass`, если значение ≤ цели, `fail`, если больше. `skip` — не мерили, причина в `note`. `info` — цели нет или сравнение бессмысленно (например, у заглушки без окна).

## Старт: первый кадр

**Часы снаружи.** Секундомер в `bench.ps1` (C#-хелпер, `Stopwatch`, `timeBeginPeriod(1)`) стартует прямо перед `Process.Start`. Поэтому в замер входят загрузчик Windows, DLL, проверка Defender и всё, что приложение делает до `main`. Сам процесс этого о себе не знает.

- **Rust:** `strategy-desk --bench-first-frame` печатает одну строку `{"first_frame_ms": ..}` после первого показанного кадра и выходит. Первичное значение — время от `Process.Start` до прихода этой строки в stdout (через событие, без опроса). Число, которое процесс сообщает о себе сам, хранится рядом в `extra.self_reported_ms`.
- **Electron:** первый видимый top-level-окно любого процесса из дерева. Окна перечисляются через `EnumWindows` каждую 1 мс, учитываются только видимые, без владельца и больше 50×50. Окно Electron создаётся с `show: false` и показывается по `ready-to-show`, то есть после первой отрисовки рендерера (`src/main/index.ts`). «Окно видно» здесь означает «первый кадр нарисован».
- Для сравнения яблок с яблоками у Rust тем же опросом снимается `extra.window_ms`. Если Rust покажет окно до первого кадра, это будет видно как разница.

**Тёплый старт.** Тот же exe запускается сразу после предыдущего запуска (между ними пауза 1,5 с). Перед серией один запуск выбрасывается.

**Холодный старт.** Это первый запуск после перезагрузки, когда файлов приложения нет в кэше ОС. Совсем честно его даёт только перезагрузка, а 10 перезагрузок никто делать не будет. Поэтому перед каждым из 10 запусков кэш вытесняется одним из способов, и способ пишется в отчёт (`harness.cold_method`, `extra.flush` по каждому запуску):

| `-ColdMethod` | Что делает | Честность |
| --- | --- | --- |
| `admin` (по умолчанию, если PowerShell запущен от администратора) | `NtSetSystemInformation(MemoryPurgeStandbyList)` — то же, что «Empty Standby List» в RAMMap: весь файловый кэш ОС выброшен | как после перезагрузки для файлов. Не сбрасывает кэш Defender и Prefetch, они живут и после перезагрузки |
| `pressure` (по умолчанию без админа) | выделяет и трогает память, пока доступной не останется 256 МБ, потом отдаёт. Windows забирает под неё standby-страницы, а с ними кэшированные exe и DLL | почти как `admin`, но гарантии нет: страницы с высоким приоритетом могут уцелеть. Остальные программы на минуту выгружаются в файл подкачки |
| `none` | ничего | это ещё один тёплый старт; пометка в отчёте обязательна |

Если холодная медиана не больше тёплой, вытеснение не сработало, и цифре верить нельзя. `report.py` показывает обе рядом. Системные DLL (user32, d3d11 и т.п.) в любом способе остаются в памяти, как и после настоящей перезагрузки: их держит сама Windows.

## Покой: RAM и CPU

Последовательность: запуск, первое видимое окно, 10 с на успокоение (стартовые выделения памяти в замер не идут), 60 с покоя. Во время покоя CPU каждую секунду суммируется по дереву процессов, поэтому процесс, который стартовал или умер внутри окна, учитывается за время, пока жил. В конце покоя снимается память. Дерево строится через Toolhelp32 от корневого PID, процессы `TradingView*` и их потомки исключаются.

У Electron в дерево попадают GPU-процесс, рендерер, utility и `claude.exe` от prewarm (`chat.prewarm` стартует его сразу). Это честная цена Electron-версии, поэтому сумма считается по всем. У Rust prewarm нет по ТЗ, так что лишний `claude.exe` в покое был бы багом. Число процессов пишется в `extra.processes`.

Rust запускается с `DESK_FRAME_STATS=1`. Его строки в stderr, попавшие в окно покоя по времени, дают `idle_frames_per_min`: сколько кадров рисуется, когда ничего не происходит.

## Кадры при стриме Claude

Протокол desk-ui: при `DESK_FRAME_STATS=1` раз в 10 с в stderr выводится строка с p50/p95/p99. Разбор терпимый: годится любая строка, где после `p50`, `p95`, `p99` идёт число, а число кадров берётся из `frames` / `n` / `count`. Например:

```
{"frame_stats":{"frames":600,"p50_ms":4.2,"p95_ms":9.1,"p99_ms":15.5}}
```

Чтобы окно стримило без человека, нужны два хука. Это **запрос к desk-app и desk-ui**, сейчас их нет:

- `DESK_CLAUDE_BIN=<путь>` — desk-app запускает этот exe вместо `claude`. Сюда подставляется `fake-claude.exe`, а он сам читает `FAKE_CLAUDE_EVENTS` / `FAKE_CLAUDE_RATE` из окружения.
- `DESK_BENCH_AUTOCHAT=1` — desk-ui после первого кадра открывает «+ Новый чат» и отправляет одно сообщение.

`bench.ps1 -StreamFrames` ставит оба (по умолчанию 10 тыс. событий по 200 в секунду, около 50 с) и собирает строки статистики. Пока хуков нет, метрики будут `skip` с подсказкой, чего не хватает.

## Загрузка дня, vault ×10, разбор stream-json

`desk_bench::day::day_files(date)` — это ровно те файлы, которые открывает `loadDay()` из `src/main/macro-state.ts`: по 8 на регион (scale, verdict, collect, shots, human_read, remember, drift, shots review), 5 на день (review, review_agent, day_answers, week, ideas), плюс `lessons.md`, CSV оценок и `config/scale.json`, всего 24. Отсутствующие файлы пропускаются, как `readJson → null`.

Пока у desk-core нет `day`, замер даёт **нижнюю границу**: `serde_json::Value` для каждого JSON и разбор CSV по строкам. Если потом `desk_core::day` окажется намного медленнее, значит, время уходит на нормализацию, а не на JSON. Переключение: `--features core-day` в desk-bench. Место отмечено `HOOK(desk-core)` в `src/day.rs`, сигнатуру нужно сверить с тем, что desk-core реально экспортирует. Для stream-json хук `HOOK(desk-app)` стоит в `benches/stream_parse.rs`.

Файлы читаются из тёплого кэша: так бывает при переключении дня. Холодный диск уже входит в холодный старт.

```
cd rust
cargo bench -p desk-bench                                   # day_load + stream_parse (criterion)
cargo run --release -p desk-bench --bin perf-sample -- --out ../docs/rust/perf/<файл>.json [--exe target/release/strategy-desk]
```

## Перерисовка после записи в `state/`

Цель ≤ 350 мс при дебаунсе 300 мс, сравнивается p95 из 20 записей. Часы общие, unix ms одной машины:

1. `slow-writer <vault>/indices-kb/state/scale_<D>_eu.json --mode rename` пишет файл и печатает `{"event":"done","t_unix_ms":…}`.
2. **Запрос к desk-app:** при `DESK_BENCH_WATCH=1` выводить в stderr при каждом новом снимке `{"snapshot_rev":N,"changed":"<имя файла>","t_unix_ms":…}`. Если хочется мерить до кадра, а не до снимка, desk-ui может добавить в ту же строку `frame_t_unix_ms`.
3. Задержка = `t_unix_ms` снимка минус `t_unix_ms` события `done`.

Стрессовые режимы того же инструмента: `--mode chunks --delay-ms 50` (читатель видит обрезанный JSON), `--mode truncate` (файл так и остаётся битым), `--mode truncate --then-complete-ms 400`, `--repeat 10 --interval-ms 50` (пачка записей на один дебаунс), `--bom` (как пишет PowerShell 5.1). Watcher обязан пережить все режимы без паники и в итоге показать последнюю целую версию.

## Переключение экрана / дня

**Запрос к desk-ui:** `strategy-desk --bench-switch <N> --vault <путь>` открывает vault, N раз переключает день (по кругу из последних дней) и экраны, затем печатает `{"switch_ms":[…]}` (от отправки `Command` до кадра с новым `Snapshot.rev`) и выходит. Пока такого ключа нет, метрика `skip`. Когда он появится, в `bench.ps1` добавится его вызов на vault от `gen-vault --scale 10`.

## Размеры и сутки работы

`exe_mb` — размер exe. У Electron это только `Strategy Desk.exe`, поэтому рядом всегда стоит `install_dir_mb`: вся папка установки. `installer_mb` берётся из `-Installer`, а для Electron скрипт сам ищет `release\Strategy-Desk-<версия>-*.exe` в репозитории.

Сутки работы: `bench.ps1 -Kind rust -SoakHours 24`. Запускается один экземпляр без других замеров, Private Bytes дерева снимается раз в 5 минут, база — первый замер после 10 минут работы. Окно 16:25 прогон не пересекает, поэтому настоящие 24 часа не получатся: прогон, начатый в 18:30, закончится сам около 16:23 следующего дня (≈ 22 ч). Фактическая длительность пишется в `extra.hours` и в `note`. Ряд замеров лежит в `extra.series`, по нему видно, растёт ли память линейно (утечка) или выходит на плато.

## Стресс

| Случай из TEAM.md | Чем создаём |
| --- | --- |
| vault ×10 (2 года) | `gen-vault <dir> --scale 10 --real-size` (501 день, ~10 тыс. файлов, ~110 МБ) |
| 10 тыс. событий подряд | `fake-claude --events 10000 [--rate 0]` |
| чтение во время записи | `slow-writer --mode chunks` |
| битый и обрезанный JSON | `slow-writer --mode truncate`, `fake-claude --garbage-every M` (5 видов: не JSON, полстроки, неизвестный `type`, не UTF-8, пустая строка) |
| обрыв `claude` посреди ответа | `fake-claude --die-after K`: K строк, потом полстроки без перевода строки и код выхода 3 |
| зависание и продолжение | `fake-claude --stall-ms 1500 [--stall-at K]` |
| нет сети | у fake-claude сети нет; для настоящего `claude` — отключить Wi-Fi вручную (скрипт сеть не трогает) |
| 4K и масштаб 150 % | вручную: настройки экрана Windows, затем `bench.ps1 -SkipCold`; масштаб записывается в `label` |

Правило TEAM.md «краш или зависание дольше 1 с = провал» проверяет QA · функционал в e2e-тестах на этих же инструментах. `perf-sample` пишет в отчёт раздел `stress` с самопроверками стенда (`"scope": "harness"`): инструмент действительно создаёт тот случай, который заявлен.

## Отчёт

Файл: `docs/rust/perf/<YYYY-MM-DD>-<commit>.json`, схема `desk-perf/1`, машинное описание — `docs/rust/perf/report.schema.json`. Пишут три места, и менять их нужно вместе: `desk_bench::report` (Rust), `bench.ps1`, `report.py`.

```json
{
  "schema": "desk-perf/1",
  "representative": true,
  "label": "rust 0.3.0-dev",
  "created_at": "2026-12-01T19:02:11Z",
  "date": "2026-12-01",
  "machine": { "os": "Microsoft Windows 11 Pro 10.0.26100 (build 26100)", "cpu": "…", "logical_cpus": 16,
               "ram_gb": 31.7, "power": "ac", "power_plan": "Сбалансированная",
               "noise": ["cpu busy before start: 2.1 %"] },
  "build": { "kind": "rust", "version": "0.3.0", "commit": "abc1234", "exe": "strategy-desk.exe", "profile": "release" },
  "harness": { "script": "scripts/rust/bench.ps1 v1", "runs": 10, "cold_method": "pressure",
               "idle_seconds": 60, "settle_seconds": 10 },
  "metrics": [
    { "id": "cold_start_ms", "label": "Cold start to first frame", "unit": "ms",
      "samples": [212.4, 230.1], "median": 221.3, "p95": 230.1, "min": 212.4, "max": 230.1,
      "target": 300, "stat": "median", "value": 221.3, "status": "pass",
      "method": "Stopwatch from Process.Start to the {\"first_frame_ms\"} line …",
      "extra": { "window_ms": [205.0, 224.9], "self_reported_ms": [140.2, 151.0], "timeouts": 0,
                 "flush": ["pressure 27392 MB in 11.4 s"] } }
  ],
  "stress": [ { "id": "fake_claude_garbage", "status": "pass", "scope": "harness", "detail": "…" } ],
  "notes": []
}
```

В отчёте нет имени машины, имени пользователя и путей: у `build.exe` только имя файла.

`python scripts/rust/report.py [файлы или папки]` печатает таблицу «метрика · до · после · цель · статус» и пишет `docs/rust/perf/summary.json` (`desk-perf-summary/1`): `before` и `after` (какие отчёты взяты), по каждой метрике `before` / `after` / `target` / `status` / `delta_pct` / `trend` (все Rust-отчёты по времени), `failed` с причиной, `stress`. «До» — самый новый Electron-отчёт, «после» — самый новый Rust-отчёт, причём репрезентативные идут раньше прочих. Ключи `--before` / `--after` задают их явно. Дашборд-артефакт читает только `summary.json`.

## База Electron 0.2.3

- **Exe:** сначала ключ удаления в `HKCU\…\Uninstall` с `DisplayName` «Strategy Desk…» и его `InstallLocation`, затем поиск `Strategy Desk.exe` в `%LOCALAPPDATA%\Programs\*` (установщик NSIS per-user кладёт его в `%LOCALAPPDATA%\Programs\strategy-desk\`). `-Exe` перекрывает оба способа.
- **Первый кадр** — первое видимое окно дерева процессов (см. выше).
- **RAM** — сумма по всем процессам Electron и их потомкам, включая `claude.exe` от prewarm.
- **Кадры, загрузка дня, watcher, переключение** у Electron не меряются (`skip`, «нет счётчика кадров»): у старого приложения нет таких хуков, а добавлять их в 0.2.3 ради базы — значит менять базу.
- Electron работает на настоящих настройках и vault владельца: при старте он читает vault и ставит watcher, пишет в vault только по нажатию кнопки. Закрывается он через `CloseMainWindow` с 5 с на корректный выход, только потом принудительно.

## Пределы метода

- Один ноутбук и одна сессия Windows. Фоновые задачи (обновления, индексация, OneDrive) добавляют шум. Скрипт пишет загрузку CPU за 3 с до старта (`machine.noise`) и предупреждает, если она выше 10 %. Сравнивать честно можно только отчёты одной машины в одном режиме питания.
- Холодный старт без админа — это вытеснение давлением памяти, гарантии нет. Defender и Prefetch не сбрасываются ни одним способом. Запуск exe, который только что пересобрали, медленнее из-за первой проверки Defender. Поэтому холодную серию лучше запускать вторым прогоном после сборки, а первый выбросить.
- Опрос окна раз в 1 мс даёт погрешность около 1–2 мс. `timeBeginPeriod(1)` на время замера меняет разрешение таймера и у самого процесса стенда.
- Первый кадр Rust меряется по строке в stdout, Electron — по видимому окну. Это разные события; честное сравнение — `extra.window_ms` у обоих.
- Простой на 60 с не ловит медленные таймеры (раз в 5 минут и реже) — их видно только в суточном прогоне.
- Загрузка дня пока меряет нижнюю границу на `serde_json`, а не настоящий парсер. Синтетика повторяет форму demo-vault, не реальный vault: числа и тексты другие, а `--real-size` дописывает выдуманные заголовки.
- `frame_*`, `watch_latency_ms`, `switch_ms` зависят от хуков, которых ещё нет (см. запросы выше). До них эти метрики `skip`.
- Linux-отчёт `docs/rust/perf/sample-linux.json` нерепрезентативен по определению: облачный CPU, нет GPU, нет окна.

## Инструменты

| Что | Где | Запуск |
| --- | --- | --- |
| синтетический vault | `rust/crates/desk-bench/src/vault_gen.rs`, bin `gen-vault` | `gen-vault <dir> [--scale N \| --days D] [--seed S] [--end D] [--real-size] [--png WxH\|none]`; ×1 = 10 дней, ×10 = 501, между ними `10·N^1.7`. Шаблоны — один полный demo-день из `make-demo-vault.py`, обновляются `scripts/rust/refresh-bench-templates.sh`. В чужую непустую папку не пишет |
| поддельный claude | bin `fake-claude` | флаги claude игнорирует; `--events --rate --seed --tool-every --die-after --garbage-every --stall-ms --stall-at` или `FAKE_CLAUDE_*`; с `--input-format stream-json` — один ход на каждую строку `{"type":"user"}` в stdin. Формы строк — по `sdk.d.ts` из `@anthropic-ai/claude-agent-sdk@0.3.278` и `translate()` в `src/main/drivers/claude.ts`. Ключи JSON идут по алфавиту: порядок полей у настоящего CLI не контракт |
| медленная запись | bin `slow-writer` | см. «Перерисовка после записи» |
| бенчи | `benches/day_load.rs`, `benches/stream_parse.rs` | `cargo bench -p desk-bench` |
| отчёт без окна | bin `perf-sample` | `--out --label --exe --installer --runs --quick --representative` |
| Windows-стенд | `scripts/rust/bench.ps1` | см. «Команды владельца» |
| сводка | `scripts/rust/report.py` | `report.py [пути]`, `--self-test` |
