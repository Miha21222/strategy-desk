# Этап 6 — два движка: Claude и Codex

План принят 2026-09-23. Цель: одно приложение работает и на подписке Claude (Agent SDK), и на подписке
ChatGPT (Codex CLI). Движок выбирается в настройках; всё, что ниже `tv:call` — панель «День», `tv` CLI,
analytics, workflows, карточки, preflight — от движка не зависит и не переписывается.

## Что уже проверено на этой машине (2026-09-23, codex-cli 0.154.0)

| Факт | Как проверено |
| --- | --- |
| `@openai/codex-sdk` спавнит `codex exec --experimental-json` + `resume` **на каждый ход** | распакован `dist/index.js` пакета 0.156.1 |
| значит на `exec`/SDK tvmcp и gt-database поднимаются заново на каждое сообщение (~3.5 с + база знаний) | следует из предыдущего; `session.ts` существует именно ради этого |
| `-c base_instructions=...` не принимается | `Error loading config.toml: unknown configuration field 'base_instructions' in -c/--config override` |
| `codex app-server` — постоянный процесс, JSON-RPC по stdio, есть `generate-ts` и `generate-json-schema` | `codex app-server --help`, схема сгенерирована локально |
| `ThreadStartParams` = `{baseInstructions, developerInstructions, config, cwd, model, modelProvider, personality, sandbox, approvalPolicy, ephemeral, threadSource, serviceName, serviceTier, sessionStartSource, approvalsReviewer}`; те же поля у `thread/resume` и `thread/fork` | сгенерированная схема `codex_app_server_protocol.v2.schemas.json` |
| в конфиге Codex есть `mcp_servers` (stdio command/args/env), `streamable_http` + `bearer_token`/oauth, `enabled_tools`/`disabled_tools`, `startup_timeout_sec`, `tool_timeout_sec` | строки в `codex.exe`; `codex mcp add/get/list/login` в CLI |
| личный конфиг владельца протёк бы в сессии приложения: `notify` → `codex-computer-use.exe`, `personality=friendly`, `memories=true`, `sandbox_mode=workspace-write`, плюс `~/.codex/AGENTS.md` | `~/.codex/config.toml` |

Вывод: драйвер Codex строится на **`codex app-server`**. Это единственный режим с паритетом по трём
пунктам сразу: живой процесс (MCP не перезагружается между ходами), настоящий системный промпт
(`baseInstructions` у `thread/start`) и MCP-карта из конфигурации (`mcp_servers`, задаётся процессу).
Режим `exec` остаётся запасным (этап 6.6).

## 6.0 Шов драйверов (рефакторинг без изменения поведения)

Renderer уже не видит `SDKMessage`: `agent.ts` переводит всё в `ChatEvent`. Шов ставится по этой границе.

- `src/shared/engine.ts` — `EngineId = 'claude' | 'codex'` и таблица возможностей
  (`persistentSession`, `systemPrompt`, `toolDenyList`, `costReported`, `thinkingStream`). UI спрашивает
  таблицу, а не `if (engine === ...)`.
- `src/main/drivers/types.ts` — интерфейс `EngineDriver`: `prewarm(spec, resume)`, `acquire(chatKey, spec, resume)`,
  `runTurn(session, text, emit)`, `stop(chatKey)`, `closeAll()`, где `emit` отдаёт уже `ChatEvent` без `chatKey`.
- `src/main/drivers/claude.ts` — нынешний `handle()` из `agent.ts` + `session.ts` как есть, без правок логики.
- `agent.ts` остаётся оркестратором: транскрипт, накопление токенов/стоимости, ретрай после краша,
  карта `active`, персист чата. Он больше не знает про SDK.
- `TurnSpec` — общий описатель хода: `{ engine, systemPrompt, model, effort, cwd, mcpServers, permissionPreset, env }`.
  Драйвер переводит его в свои опции. `fingerprintOptions` хеширует **и `engine`**, иначе тёплая сессия
  Claude будет отдана ходу Codex.
- Тесты: существующие `agent.test.ts` / `session.test.ts` переезжают на драйвер Claude без изменения
  ожиданий; новый тест — фингерпринт различает движки, `dropWarm()` при смене движка.

Критерий приёмки этапа: `npm test` и `npm run typecheck` зелёные, поведение на Claude неотличимо.

## 6.1 Драйвер Codex (`src/main/drivers/codex.ts`)

1. **Протокол.** `scripts/gen-codex-proto.mjs` вызывает `codex app-server generate-ts` и кладёт типы в
   `src/main/drivers/codex-proto.ts`. Сгенерированное **коммитится** (app-server помечен experimental,
   сборка не должна зависеть от наличия codex на машине сборщика).
2. **Процесс.** Один `codex app-server` на приложение: MCP-карта задаётся **на поток** через свободное
   поле `config` у `thread/start` (проверено живьём — сервер, переданный там, поднялся именно для этого
   потока), поэтому смена настроек не требует перезапуска процесса. Флаги: `--stdio`; `--ignore-user-config`
   у `app-server` НЕТ (он есть только у `codex exec`), изоляция делается своим `CODEX_HOME` — см. 6.2.
   Следствие, которого нет у Claude: все чаты делят один процесс движка; параллельные ходы из разных чатов
   проверить отдельно (открытый пункт 6.5).
3. **Поток на чат.** `thread/start` с `baseInstructions` = наш собранный системный промпт (тот же
   `composePrompt`), `cwd` = `store.chatWorkspaceDir()`, `model`/`modelReasoningEffort` из настроек,
   `sandbox`/`approvalPolicy` из пресета прав, `config.mcp_servers` = перевод `buildMcpServers()`.
   `chat.sessionId` хранит `threadId`, перезапуск приложения делает `thread/resume`.
   Ход — `turn/start` (`input`, `model`, `effort`, `cwd`, `approvalPolicy`, `sandboxPolicy`, `outputSchema`),
   прерывание — `turn/interrupt`, догон текста в идущий ход — `turn/steer` (проверено по схеме).
4. **Перевод MCP-карты.** `src/main/drivers/codex/servers.ts`: наш `StdioServer` → `{command, args, env}`,
   `HttpServer` → `url`. Передаётся в `thread/start` как `config.mcp_servers`. `alwaysLoad` аналога не
   имеет: схемы всех тулов лежат в каждом ходу (~24k токенов на 64+8 тулов) — это цена движка.
   Ход нельзя слать, пока серверы не сообщили `ready`/`failed`, иначе модель отвечает «инструмент
   недоступен» (поймано живьём).
5. **Перевод событий → `ChatEvent`.** Таблица соответствий в одном месте, с тестом на записанном
   JSON-RPC логе (фикстура в `src/main/drivers/__fixtures__`): agent message deltas → `text`,
   reasoning → `thinking`, MCP tool call → `tool` + `tool-result`, command execution → `tool`
   (имя `shell`), turn completed → `done` с usage, ошибки → `error`.

## 6.2 Изоляция и права

- **Личный конфиг владельца в сессии приложения не участвует: у приложения свой `CODEX_HOME`**
  (`<userData>/codex-home`). Проверено: `-c mcp_servers={}` личные серверы не гасит, `--ignore-user-config`
  у `app-server` нет, ключа против `AGENTS.md` в бинарнике нет. Свой `CODEX_HOME` даёт чистый поток:
  `instructionSources` = только `AGENTS.md` рабочей папки чата. Плата — он стартует разлогиненным;
  владелец выбрал перенос своего `auth.json` по кнопке (задача `codex-auth-import`), альтернатива —
  `codex login` с нашим `CODEX_HOME` копируемой командой. `config.toml` пишется один раз, потому что
  `codex mcp add` дописывает в тот же файл.
- cwd чата не git-репозиторий → `--skip-git-repo-check` (в CLI) / соответствующий параметр потока.
- Пресеты прав переводятся так: `standard` → `sandbox: read-only`, `approvalPolicy: never`;
  `extended` → `workspace-write` с рабочей папкой чата в `additionalDirectories`.
- **Честная разница, её надо показать в UI:** у Codex shell-тул не снимается, только запирается
  песочницей. `disallowedTools: ['Bash', 'Write', ...]` на Codex не воспроизводится; в стандартном
  пресете модель всё равно может выполнять read-only команды. Строка об этом — в настройках рядом с
  выбором движка и в справке.
- Сужать набор тулов tvmcp при необходимости можно через `enabled_tools`/`disabled_tools` у сервера.

## 6.3 Настройки, промпт, UI

- `Settings`: `engine: EngineId`; `model`/`effort` становятся пер-движковыми
  (`models: Partial<Record<EngineId, string>>`, `efforts: ...`), миграция старых значений в ключ `claude`.
  Добавить `codexPath` (как `tvDesktopExe` — путь, а не команда).
- Смена движка = `dropWarm()` + закрытие сессий (механизм уже есть при смене настроек).
- `ChatMeta.engine`: чат помнит, каким движком начат. Смешивать нельзя — `threadId` и `sessionId`
  не переносятся. В шапке чата — значок движка; старые чаты читаются как `claude`.
- Промпт общий. Контракт блоков (`nextsteps`, `chartcard`, `stressreport`, `prodata`, `briefreport`,
  `healthdata`, `workflowstate`) промпто-зависимый, значит переносимый, но GPT будет дрейфовать по
  формату: если дрейф подтвердится на живых ходах — добавить `engineSuffix('codex')` с жёстким примером
  блока, по той же схеме, что `langSuffix`/`modeSuffix` (суффикс после замороженного префикса).
- Стоимость: на подписке Codex не сообщает USD → `costUsd = null`, в UI прочерк, счётчик токенов из
  `usage` хода. Не выдумывать цену.

## 6.4 Preflight, задачи, упаковка

- Новые `CHECK_IDS`: `codex` (наличие и версия), `codexAuth` (`~/.codex/auth.json` + `codex doctor`),
  `codexOfficialMcp` (официальный TradingView MCP зарегистрирован в Codex — **отдельный OAuth**, не тот,
  что у Claude). Показывать шаг мастера только для выбранного движка.
- Фиксы честные, как договорено: `copy` с `npm i -g @openai/codex` и `codex login`; приложение ничего
  не ставит и не логинит само.
- Новые `TASK_IDS`: `codex-official-mcp-add` (`codex mcp add`), `codex-official-mcp-login`
  (`codex mcp login`). Renderer по-прежнему называет id задачи, не команду.
- **Codex не бандлим.** `@openai/codex` тащит бинарь ~350 МБ (на этой машине `codex.exe` — 353 МБ);
  на 650-мегабайтный установщик это плюс треть. Codex — обнаруживаемая внешняя зависимость, как
  TradingView Desktop. Claude-сборка остаётся ровно такой, какая есть.

## 6.5 Живая проверка (критерий готовности этапа)

1. Один и тот же промпт («вердикт дня», «прочитай мой график», «собери план») на обоих движках, сверка
   карточек и вызовов тулов один в один.
2. Панель «День» не трогается (она на CLI); проверить, что ход по `askModelOn` уходит в выбранный движок.
3. Замер: время боота `app-server` + tvmcp, время первого и второго хода — второй ход должен быть без
   перезагрузки MCP (это и есть доказательство, что маршрут выбран правильно).
4. Прогон `DESK_SMOKE` и `npm run dist:check` на собранной сборке с `engine=codex`.

## 6.6 Запасной драйвер `exec` (опционально, после 6.5)

Если `app-server` сломается на обновлении Codex: драйвер поверх `@openai/codex-sdk` (`CodexOptions.config`
пропускает `-c` оверрайды, значит MCP-карта переносится), системный промпт — через `AGENTS.md` в cwd чата
плюс префикс в первое сообщение. Цена: MCP-серверы поднимаются на каждый ход, prewarm невозможен.
Драйвер должен уметь сказать это пользователю строкой в чате, а не молча тормозить.

## Открытые вопросы (закрыть до кода в 6.1)

- Кадрирование JSON-RPC у `app-server` (LSP-style `Content-Length` или JSONL) — читать из
  сгенерированных TS-биндингов.
- Принимает ли `ThreadStartParams.config` ключ `mcp_servers` (тогда MCP-карта живёт на потоке и процесс
  не надо перезапускать при смене настроек).
- Как изолировать личный `~/.codex` — `-c`-оверрайды или свой `CODEX_HOME`, и что в каждом случае
  происходит с `auth.json` и `AGENTS.md` владельца.
- Ведут ли себя параллельные ходы из двух чатов корректно на одном общем tvmcp.
- Поведение при лимитах подписки ChatGPT на длинных сессиях: в каждом ходу висят схемы всех тулов.
- Стабильность experimental-протокола между версиями codex: закрепить минимальную проверенную версию в
  `codexVersion` preflight-чека и деградировать понятной ошибкой, а не стеком.

## Состояние на 2026-09-23 (вечер)

Сделано и проверено живьём:

- **6.0 шов драйверов** — `src/shared/engine.ts` (EngineId + таблица возможностей), `src/main/drivers/`
  (`types.ts`, `claude.ts`, реестр), `agent.ts` стал движко-нейтральным оркестратором. Поведение Claude не
  изменилось, `ChatEvent` разделён на `ChatEventBody` + `chatKey`, `AgentInit.claudeCodeVersion` →
  `engineVersion`.
- **6.1 драйвер Codex** — `src/main/drivers/codex/`: `protocol.ts` (JSON-RPC по строкам), `home.ts`
  (приватный CODEX_HOME), `servers.ts`, `translate.ts`. Живой прогон через собственный CLI приложения
  (`DESK_SMOKE=codex`): ход 1 — 21.7 с с вызовом `mcp__tradingview__tv_data_providers_status` и ответом по
  делу, ход 2 — 2.9 с без перезагрузки MCP. `gt-database` и `tradingview` поднялись, официальный
  `mcp-tradingview` честно сообщил `mcp:mcp-tradingview:failed` (у него свой OAuth в Codex).
- **6.2 изоляция и права** — приватный CODEX_HOME (в личном `~/.codex` в поток затекали девять MCP-серверов
  владельца, его `AGENTS.md`, `notify`, personality и memories); песочница `read-only` / `workspace-write`
  по пресету; approval-callback как аналог deny-листа: MCP-вызовы разрешаются только для серверов, которые
  зарегистрировало само приложение, shell отклоняется в обоих пресетах.
- **6.3 настройки** — выбор движка и путь к `codex` в настройках, чат помнит свой движок, смена настроек
  закрывает сессии всех движков.
- **6.4 preflight и задачи** — проверки `codex` / `codexAuth` / `codexOfficialMcp`, свой шаг мастера
  (шаг неактивного движка не показывается вовсе), задачи `codex-auth-import`, `codex-mcp-add`,
  `codex-mcp-login`. Прогон `DESK_SMOKE=preflight` с `engine=codex` даёт три зелёно-жёлтых строки по делу.

335 тестов, typecheck и сборка зелёные.

Закрытые вопросы плана: кадрирование — одна JSON-строка на сообщение; прерывание — `turn/interrupt`
(плюс `turn/steer`); `thread/start.config` **принимает** `mcp_servers` на поток, поэтому один процесс
обслуживает чаты с разными наборами тулов без перезапуска; изоляция — только свой CODEX_HOME.

Осталось (6.5–6.6): живой торговый день на Codex и сверка карточек с Claude один в один; проверка дрейфа
формата fenced-блоков у GPT; запасной драйвер на `exec`; параллельные ходы из двух чатов на одном
app-server.

Новый открытый вопрос (в чек-лист 6.5): применяет ли `thread/resume` к уже живому потоку новые
`baseInstructions`. Если нет, то переключение «Экономный/Полный» и смена языка ответа не дойдут до
существующего потока Codex (у `turn/start` поля для системного промпта нет) — проверяется одним ходом:
переключить режим в середине чата и посмотреть, изменилось ли поведение.

Ещё один пойманный живьём landmine: ход нельзя отправлять сразу после `thread/start` — список тулов
модель получает один раз, и слишком ранний ход отвечает «инструмент недоступен», не вызвав его. Драйвер
ждёт, пока каждый зарегистрированный MCP-сервер сообщит `ready` или `failed` (потолок 30 с).
