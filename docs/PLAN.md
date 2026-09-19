# Strategy Desk — план по этапам

## Этап 0 — оболочка чата (текущий)

- [x] Каркас: electron-vite + TypeScript + Preact, vitest, electron-builder (stub), строгий CSP
- [x] Тёмная/светлая тема через CSS-переменные (`prefers-color-scheme` + ручной выбор)
- [x] Хранилище: `config.json` (секреты через `safeStorage`), `chats.json`, `profile.json`, `transcripts/`, атомарная запись
- [x] `mcp.ts`: `tradingview` (stdio через `uv`), опциональный официальный `mcp-tradingview` (http, выкл. по умолчанию)
- [x] `agent.ts`: системный промпт + профиль трейдера, пресеты прав `standard`/`extended`, env по способу входа, поток `chat:event`, `interrupt()`, повтор при падении процесса (Windows access violation)
- [x] `status.ts`: TradingView запущен / порт CDP + вкладка графика / `tvmcp --check` (кэш 60 с) / вход в Claude
- [x] `tv-launch.ts`: запуск TradingView с `--remote-debugging-port`, не убивает запущенный экземпляр
- [x] IPC + typed `window.desk` через `contextBridge`
- [x] Renderer: сайдбар (лампочки, быстрые действия, чаты с полками/закреплёнными/датами, поиск, действия по наведению)
- [x] Чат: потоковый текст, строки инструментов, индикатор «думает», композер (Enter/Shift+Enter), Стоп, счётчик токенов/стоимости, режим Экономный/Полный (`effort: medium`)
- [x] Markdown: абзацы, **жирный**, `код`, блоки кода с языком и «Копировать», карточки-заглушки для `nextsteps|chartcard|...` (парсер в `chat/blocks.ts` + тест)
- [x] Настройки: Claude (способ входа, ключ/токен), tvmcp, порт CDP, наборы инструментов, официальный MCP, модель, effort, права, тема, профиль трейдера
- [x] Помощь: схема, три шага, типичные проблемы
- [x] `README.md`, `CLAUDE.md`, `docs/PLAN.md`
- [x] `npm run typecheck`, `npm test`, `npm run build` проходят; smoke `query()` без MCP отвечает
- [ ] Живая проверка с TradingView + tvmcp (см. чек-лист в отчёте этапа)

## Этап 1 — живая проверка и доводка

## Этап 2 — структурированные карточки

## Этап 3 — рабочие процессы по полкам

## Этап 4 — документы и вложения

## Этап 5 — упаковка и обновления
