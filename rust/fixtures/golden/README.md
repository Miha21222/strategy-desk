# Golden fixtures: the old app's answers

TEAM.md rule 4: while Electron 0.2.3 lives, the right answer for reading the vault is what its
`loadDay()` returns. These files are that answer, captured by running the TS code, not written by
hand. A Rust result that differs is a Rust bug unless `docs/rust/TZ.md` changes the behaviour on
purpose (the one such place is marked in `expected/helpers/windows.json`).

Regenerate everything (needs `python3` + Pillow, `node`, and `ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm ci --ignore-scripts`):

```
bash rust/fixtures/tools/capture-golden.sh
```

The capture is reproducible: two runs give byte-identical files. Nothing in it is personal: the
demo vault is synthetic by design (`[демо]` in every text), and the script greps the result for
names and machine paths before it finishes.

## What is here

| Path | What | Made by |
| --- | --- | --- |
| `vault/` | the demo vault: 9 invented September days + «today» = a copy of 14.09 under **2026-10-26** | `scripts/make-demo-vault.py` through `tools/make-vault.py`, which pins the clock (the generator's «today» would otherwise change every day) |
| `expected/index.json` | `listDays(vault)` | `scripts/preview/dump.ts` (unchanged, bundled with esbuild as `preview.sh` does) |
| `expected/day_<D>.json` | `loadDay(vault, D)` for every listed day (10 days) | the same |
| `expected/helpers/*.json` | the pure helpers of `src/shared/macro.ts`, case by case | `tools/capture-helpers.ts` |
| `../edge/<case>/` | hand-made broken vaults + `loadDay()` over them | `tools/make-edge.py` + `tools/dump-edge.ts` |

`dump.ts` also writes `defaults.json` (the settings defaults); it is dropped: it is desk-app's
settings contract, not the day's, and it names the owner's GitHub repo.

### The days of the demo vault

| Day | What it exercises |
| --- | --- |
| 2026-09-14 Mon | a normal good day: both windows, a discussion, review + agent review, the owner's read and answers, review screenshots, memory cards |
| 2026-09-15 Tue | strong long, the needle was wrong |
| 2026-09-16 Wed | no edge (neutral) |
| 2026-09-17 Thu | red light, CPI; US screenshots |
| 2026-09-18 Fri | a discussion flips the needle; `week_<D>.json` (Friday) |
| 2026-09-21 Mon | reviews in progress (half-written: partial windows) |
| 2026-09-22 Tue | very long texts |
| 2026-09-23 Wed | Europe only, a review without the agent's part |
| 2026-09-24 Thu | an old v1 scale (weights, points out of `max`) |
| 2026-10-26 Mon | «today»: a copy of 14.09 without the owner's read and answers (the «Сначала человек» flow); a DST-mismatch week |

## Rules of the canonical JSON (`tools/canon.ts`)

1. **JSON round trip**, exactly what the old app sends over IPC: `undefined` fields are absent,
   `null` fields are present. The Rust `Day` must serialize the same way (skip a field TS leaves
   `undefined`, write `null` where TS writes `null`). The test harness can relax this
   (`Cmp { absent_equals_null: true }`) to look past it while porting, never to pass.
2. **`"$VAULT"`** replaces the absolute vault path (only `index.json` carries it).
3. **Screenshots: `{"$shot": "<vault-relative path>", "bytes": N}` instead of base64.** The old app
   reads a PNG into a `data:image/png;base64,…` string (`readShot`) and ships it over IPC; the Rust
   version hands out the path and the UI decodes the file in the background (TZ «Почему сейчас
   медленно»). Any `data:image/png;base64,…` string met during capture is replaced by this object; the
   path is found by matching the decoded bytes against the PNGs under `vault/indices-kb/shots`, and a
   payload that matches no file fails the capture.
   *Today `loadDay()` itself never embeds pictures* — the day carries `shots[].path` strings — so the
   day files contain no `$shot`. The shape appears in `expected/helpers/shots.json` → `read_shot`:
   `readShot(vault, rel)` for every shot path of the vault (and a few refused ones → `null`). That is
   the contract for the desk-app call that replaces `macro:image`.
4. Pretty-printed with a one-space indent, keys in TS order; the big case tables (`helpers/*.json`)
   keep one case per line so a diff shows which case moved.
5. `note` paths are vault-relative with **`/`**. The old app builds them with `path.join`, so on
   Windows it says `Макро-обзоры\2026-09-14 Европа.md`; the capture runs on Linux and the Rust port
   uses `/` everywhere (like shot paths). Decision recorded in `docs/rust/TEST_PLAN.md`.

## `expected/helpers/`

Every table has an `about` line saying which TS function produced it and what is odd about it.

| File | Tables (cases) |
| --- | --- |
| `zones.json` | `zone_of` (92), `clamp_needle` (12), `zone_segments` (4) |
| `gauge.json` | `needle_angle` (47), `polar` (10), `arc` (10: SVG path + the four corners) |
| `windows.json` | `cases` (529): `windowState` over 12 dates × 28 Kyiv times, presence variants, past / future days, Kyiv midnight. Each case has `ts` (old app) and `tz` (TZ №21) answers; 46 differ, all in DST-mismatch weeks |
| `stats.json` | `parse_csv` (13), `factor_stats` (18, incl. `sort_ties`), `lessons_tail` (7) |
| `names.json` | `state_file` (47), `date_of_state_file` (29), `list_days` (1 list of 28 names), `is_shot_path` (17) |
| `normalize.json` | `cases` (73): every `normalize*` incl. wrong types, plus `visibleIdeas` |
| `prompts.json` | `run_command` (6), `discuss_prompt` (16), `macro_chat_title` (3) |
| `shots.json` | `read_shot` (20, the `$shot` shape), `main_shot` (5), `shot_tab_label` (8), `shots_error_text` (14), `legend_color` (10) |
| `misc.json` | `fmt` (15), `is_friday` (10), `human_hit` (9), `split_factors` (3), `trader_lines` (4), `kyiv_now` (10) |

`shots.json` (except `read_shot`) and `misc.json` (except `kyiv_now`) are UI helpers: the golden
tests of desk-core do not read them; desk-ui's tests should.

## The edge vaults (`../edge/<case>/`)

Each case: `vault/` (the input), `case.json` (`dates`, `compare`, `why`), `expected/` (the old
app's `listDays` + `loadDay` for those dates). `compare: strict` = the same JSON; `compare:
no-crash` = the old app **threw** (`{"$error": …}` recorded) and Rust must load a day instead.

| Case | What |
| --- | --- |
| `truncated-json` | files cut off mid-write: scale, verdict, shots, remember, drift, review, ideas, the end of lessons.md and of the CSV |
| `being-written` | a scale without `result`, empty files, temp files of the temp+rename write (a date with only a temp file is still listed — TS quirk) |
| `unknown-fields` | a newer skill's extra fields everywhere; raw files keep them in the day JSON, normalized ones drop them |
| `empty-state-dir` | `state/` exists and is empty (a `.gitkeep` holds it in git), no journal, no config |
| `no-state-dir` | the wrong folder: `index.ok = false, error = no-state-dir`, the day is empty |
| `old-v1-scale` | v1 scales with and without `max`, an `on: false` factor, `after`; a mixed v2 file; a v3 file without `undecided` |
| `wrong-types` | numbers as strings, strings as arrays, NaN in a scale (Python writes it), a JSON string as verdict, config zones as strings, BOM + CRLF lessons |
| `bom-crlf` | PowerShell-written files: UTF-8 BOM + CRLF everywhere |
| `verdict-fallback` | the verdict file beats the copy in the collect file; `null` file → the copy; notes by exact name |
| `ts-crash` | `null` inside `collect.markets` / config `factors`: the old `loadDay()` throws, Rust must not |
