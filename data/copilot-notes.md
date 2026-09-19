# Live field notes (tvmcp desktop toolset)

Things that bit real sessions on the owner's TradingView Desktop. Treat as facts, not guesses.

- The Pine editor does not mount through `bottomWidgetBar.activateScriptEditorTab()` or `showWidget('pine-editor')`; only clicking the footer `[data-name="pine-dialog-button"]` mounts Monaco. If a Pine tool reports "editor not mounted", ask for `tv_desktop_workspace_prepare` (or the footer click) instead of retrying the same call.
- Panel buttons are icon-only and localized: the label lives in `title` (RU "Обновить на графике" / "Добавить на график" / "Сохранить"). Match text, title and aria-label in EN and RU; never assume English UI. Ctrl+Enter compiles only when Monaco is focused.
- Editing the text of a SAVED script is auto-saved to the user's account as a new version within seconds. `tv_desktop_pine_set_source` refuses while a saved script is open unless `overwrite_saved=true`; never pass it without an explicit yes from the user. Prefer working on an agent-owned copy (`pine_save_as` / build tool's verified copy).
- `clear_studies` in `tv_desktop_pine_build_and_backtest` is tri-state: `"none"` keeps everything, `"strategies"` removes only strategy scripts (default for backtests), `"all"` wipes the pane. The user's chart carries their own studies (LuxAlgo Imbalance Detector, Dark Trader session labels, a DXY overlay) — never `"all"` without listing them and getting a yes.
- Zero trades twice in a row after a compile is structural, not bad luck: check `margin_long/margin_short`, the session string, the date window gate, and whether bars are actually loaded (`tv_desktop_scroll_to_date`). Stop re-polling the Strategy Tester and diagnose.
- CDP port: TradingView Desktop is launched on **9223**; 9222 on this machine belongs to the wmux browser panel, and Chromium silently skips a busy port. A red "CDP port" light with 9222 configured means the wrong port, not a dead TradingView.
- TradingView ignores a synthetic Escape keydown. Dialogs close only by clicking their Cancel/close button (`tv_desktop_ui_click` or `dismiss_dialogs`).
- Hidden (eye-toggled-off) studies are unloaded by TradingView: `read_study_plots` returns empty data. Ask the user to make the study visible instead of reporting "no values".
- Protected Pine scripts carry their encrypted source as a multi-KB hidden `text` input; read studies with `compact=true` or the output budget blows up on one study.
- Public `setVisibleRange()` throws "Not implemented" in the desktop build; use `tv_desktop_scroll_to_date` / `tv_desktop_set_visible_range` (bar-index zoom) and report `earliest_loaded` and `clamped` honestly.
