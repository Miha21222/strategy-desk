# Reference screenshots of the old macro-review screen

The Electron 0.2.3 renderer, run in a browser over `rust/fixtures/golden/vault` by
`scripts/preview/` (unchanged), for TZ stage 3: «снимки egui рядом с снимками нынешнего
preview.sh по тем же шагам steps.json». Regenerate with
`bash rust/fixtures/tools/capture-reference-ui.sh`.

- **Clock:** `NOW=2026-10-26T07:40:00Z` = Monday 26 October 2026, **09:40 Kyiv** (the demo vault's
  «today»; the same Kyiv wall clock the preview steps were written for). It is a DST-mismatch
  week (Europe on winter time, the US not yet), see «Deliberate differences» below.
- **Viewport:** 1380 × 1400, device scale 1, `ru-RU`, `Europe/Kiev`; `light/` = `colorScheme:
  light`, `dark/` = `dark` (the app's theme setting is «system»). The steps file's `full: true` is
  Playwright's fullPage, which the app shell never grows past the viewport, so long screens are
  cut at 1400 px; **the full on-screen text of every step is in `<theme>/text.json`**.
- **Size:** quantized to 256 colours, no downscale needed (81–148 KB each, 34 files);
  `shots.json` lists every file's size.
- `text.json` per step: `day` (header), `tiles` (the three window tiles), `agentStrip`, `cards`
  (card titles in order), `buttons`, `main` (the visible text of the main area), `rawKeys`
  (untranslated i18n keys on screen — empty everywhere). `docs/rust/UI_CHECKLIST.md` turns
  these into the lists the egui screen is checked against.

Steps run in sequence inside one browser session per file, so each step starts from the
screen the previous one left (the owner's answers are kept in memory by the mock).

## `scripts/preview/steps.json`

| Step | Day / window | Does | The screen must show | Gating row (TEST_PLAN §4) |
| --- | --- | --- | --- | --- |
| `10-eu-ask` | 26.10, Европа | open | Question only: «Сначала твой взгляд», four answers, «Показать обзор без ответа». All three tiles hide the needle («сначала твой взгляд» / «после твоего взгляда»). No gauge, plan, factors, memory cards | 1, 10, 16 |
| `12-eu-said` | 26.10, Европа | answers «вниз» | The read card («Твой взгляд: вниз · Шкала: вниз — вы сходитесь»), then «Сегодня помни», «Что изменилось», gauge −41 % (ghost −57 %), «Что делать» with the M15 shot and legend, «Почему так», «Не согласен?» with one argument, «Подробнее». EU tile: «сильный шорт / осторожно» | 3 |
| `14-us-ask` | 26.10, США | opens the US tile | Question only for the US window (16:30–18:30 in the old app) | 1 |
| `14b-us-peek` | 26.10, США | «Показать обзор без ответа» | The US review in full with the question still on top (no peek link any more); the evening tile now shows «2 из 2 верно» because neither window is withheld | 2, 12 |
| `15-review-today` | 26.10, Итог дня | opens the evening tile | «Твой день» form first (two questions + note), then «Как прошёл день», Европа / США cards with «Ты / Шкала / План», the 20:00 shot legend, «О тебе», «Идеи на проверке», «Уроки дня» | — |
| `17-review-saved` | 26.10, Итог дня | picks EU «шорт», «по правилу», US «не брал», types a note | The «по правилу / не по правилу / не уверен» row appears for the region with a trade | — |
| `18-review-saved2` | 26.10, Итог дня | «Сохранить» | The form collapses to «Европа: шорт · по правилу», «США: не брал», the note in quotes, «Изменить»; the answers also appear under «Ты» in each region card | — |
| `21-prev-review` | 24.09, Итог дня | «‹» then the evening tile | A past day with a US scale only (v1, «собирается»), EU and evening «не было»; main: «Итога дня ещё нет — он появится в 20:00.» (old wording even on a past day), agent strip offers «Запустить обзор»; «Сегодня» button visible | 8 |
| `30-chat` | — | tab «Чат» | Empty chat: «Спроси агента», six suggestion chips, «Прикрепить», «Отправить» | — |
| `32-help` | — | tab «Помощь» | «Как пользоваться», «Мастер настройки», the README guide | — |

## `scripts/preview/steps-memory.json` (note 23 §2–3: the memory cards)

| Step | Day / window | Does | The screen must show |
| --- | --- | --- | --- |
| `m10-eu-ask` | 26.10, Европа | open | Same as `10-eu-ask`: «Сегодня помни» is withheld with the gauge |
| `m12-eu-said` | 26.10, Европа | answers «вниз» | «Сегодня помни» (урок / правило / похожий день, each with why and ref) above the gauge, «Что изменилось» (two signals, the first a warning) |
| `m14-us-peek` | 26.10, США | US tile, «Показать обзор без ответа» | US «Сегодня помни» (two lines), no «Что изменилось» (the file has no signals → no card at all) |
| `m15-review-today` | 26.10, Итог дня | evening tile | Ideas with the score «✓3 ✗1» and «подтвердилась 3 раза — правило или доля? решаешь ты»; not a Friday → no «Неделя в пять строк» |
| `m21-friday-review` | 18.09 (Fri), Итог дня | «‹» × 5, evening tile | «Неделя в пять строк · 14.09 – 18.09» (five lines), «О тебе» from the WEEK file, the share proposal line, US «Обзора не было — оценивать нечего.», EU «после обсуждения — пропуск» |
| `m22-friday-eu` | 18.09, Европа | EU tile | A past day: no question, the read card absent (no read that day), «Сегодня помни», gauge «нет перевеса» with ghost −48 % |
| `m32-help` | — | tab «Помощь» | as `32-help` |

## Deliberate differences the egui screen will show on 26.10

2026-10-26 is a DST-mismatch week. The old app's window clock is fixed; the Rust one follows the
exchanges (TZ №21, `desk_core::windows::window_times`). On these references the egui screen
must differ exactly here and nowhere else:

| Where | Old app (these PNGs) | Rust on 2026-10-26 |
| --- | --- | --- |
| US tile time | «США · 16:00» | «США · 15:00» |
| US question | «Куда пойдёт окно открытия США (16:30–18:30)?» (hard-coded in `macro.yourReadAsk.us`) | «… (15:30–17:30)?» |
| US idle strip | «Обзор США приходит сам в 16:00…» (`macro.agentIdle.us`) | «… в 15:00…» |

The old strings with fixed times need parameters in Rust (`{at}`, `{open}–{until}`); the
wording around them stays. To see the three rows match too, capture both sides in a scratch
copy with an ordinary «today»: `GOLDEN_TODAY=2026-11-25 bash rust/fixtures/tools/capture-golden.sh`
then `NOW=2026-11-25T07:40:00Z bash rust/fixtures/tools/capture-reference-ui.sh` (do not commit
that run: the committed golden is pinned to 2026-10-26).
