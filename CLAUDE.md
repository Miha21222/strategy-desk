# CLAUDE.md — Strategy Desk

Agent onboarding for this repo. UI strings are Russian, code/comments/prompts are English.

## What it is

Electron desktop chat shell over the `tradingview` MCP server (tvmcp, Python, `C:\Users\Admin\Projects\tradingview-mcp`)
using `@anthropic-ai/claude-agent-sdk`. The SDK spawns the user's Claude Code binary and reuses the login in `~/.claude`.
No telemetry, no paywall, no bundled Claude binary.

## Layout

```
electron.vite.config.ts     main/preload = CJS bundles, renderer = Preact via Vite
electron-builder.yml        packaging stub (NSIS / dmg / AppImage)
prompt/system-prompt.md     base system prompt (English, short)
src/shared/api.ts           IPC contract + all types shared by main/preload/renderer (window.desk)
src/shared/panel.ts         the `panel` contract: schema, tool allowlist, spec assembly, poll scheduler, event->turn rule
src/shared/workflow-schema.ts  the shape of workflows/<id>/workflow.yaml (electron-free, so it can be tested)
src/shared/defaults.ts      default settings / profile
src/main/index.ts           BrowserWindow, app lifecycle
src/main/ipc.ts             ipcMain.handle/on registration (status, tv:launch, tv:call, panel, settings, profile, chats, chat, open)
src/main/store.ts           JSON files in userData (config/chats/profile/panel/transcripts), safeStorage secrets, atomic writes
src/main/agent.ts           query() wrapper: system prompt, env, permission presets, SDKMessage → ChatEvent, retry on crash
src/main/mcp.ts             mcpServers map (tradingview stdio; optional official http server) + buildTvEnv()
src/main/tv-cli.ts          callTool(): deterministic tvmcp calls through `uv ... run tv <tool>`, no model turn
src/main/workflows.ts       workflow loader (yaml + promptBlock), cached
src/main/status.ts          lights: TradingView process, CDP /json/version + chart tab, tvmcp --check (cached 60 s), Claude auth
src/main/tv-launch.ts       launch TradingView with --remote-debugging-port, never kills a running instance
src/preload/index.ts        contextBridge → window.desk
src/renderer/app.tsx        shell: sidebar + tabs, owns state and the single chat:event subscription
src/renderer/chat/          blocks.ts (fenced-block parser + test), Markdown.tsx, ChatView.tsx
src/renderer/components/    Sidebar, DayPanel, SettingsTab, WorkflowsTab, HelpTab
src/renderer/panel/cards.tsx   presentational cards for the «День» tab (levels, calendar, risk, journal, event feed)
workflows/<id>/             workflow.yaml + prompt.md (+ risk-rules.example.json): every strategy number lives here
src/renderer/i18n/          ru.ts (source of truth), en.ts (stub), t()
src/renderer/styles.css     hand-written CSS, theme via CSS variables
docs/PLAN.md                stage roadmap and checklists
```

## Commands

```
npm run dev         electron-vite dev (HMR)
npm run typecheck   tsc for tsconfig.node.json + tsconfig.web.json
npm test            vitest (src/**/*.test.ts)
npm run build       electron-vite build → out/
npm run dist        build + electron-builder --dir → release/

DESK_SMOKE=status npm run dev   headless self-test: logs getStatus() and quits (dev only)
DESK_SMOKE=blocks npm run dev   + assembles the prompt, loads the workflows, parses one of every block
DESK_SMOKE=panel  npm run dev   + walks the day panel's deterministic path and checks the tv:call allowlist
DESK_SMOKE=query  npm run dev   same + one trivial SDK query() without MCP (costs a few cents)
```

Gotcha for agents on Windows: when writing source through a Git Bash heredoc, `\\` collapses to `\`; write files that
contain backslash paths with the editor tool, not heredocs.

## Conventions

- Add a new IPC call in three places: `src/shared/api.ts` (type), `src/preload/index.ts` (bridge), `src/main/ipc.ts` (handler).
- All user-visible strings go through `t('section.key')` from `src/renderer/i18n`; add keys to `ru.ts` first.
- Use only documented Agent SDK options (see `node_modules/@anthropic-ai/claude-agent-sdk/sdk.d.ts`); the SDK is ESM-only
  and is loaded with dynamic `import()` from the CJS main bundle.
- Chat events are the union `ChatEvent` in `api.ts`; the renderer treats the trailing assistant message with `at === -1` as live.
- Secrets never cross IPC; the renderer sees `hasApiKey` / `hasOauthToken` only.
- CSP is `default-src 'self'; style-src 'self' 'unsafe-inline'` (meta tag in `index.html`). No external scripts/styles.
- Keep `tv-launch.ts` small: it will move into the tvmcp tool `tv_desktop_launch`.
- Commit messages end with the attribution line the session's system reminder gives.

## The `panel` contract (stage 4)

A workflow may declare a `panel` in its `workflow.yaml`. The app then drives a whole
trading day **deterministically**, with no model turn, through the tvmcp CLI
(`uv --directory <tvmcp> run tv <tool> '<json>'`). MCP tools are called by the model;
the CLI is how the app calls the same implementations itself. A call costs ~2.5 s of
local CPU (Python + fastmcp import) and zero tokens.

```yaml
panel:
  kind: session-watch | risk-desk
  sentinel: {...}        # tv_sentinel_start spec, minus `day` and `levels` (the app fills both)
  levels: {...}          # tv_scan_levels args; symbol/provider default to the sentinel's
  levelNames: [...]      # which computed level names the watch gets; empty = all
  clocks: [{name, at, tz}]
  askModelOn: [RANGE_CLOSED, BREAK_CONFIRMED, SWEEP, CLOCK]   # the ONLY events that buy a turn
  calendar: {...}        # tv_calendar_check args (the app adds `date`)
  risk: {...}            # tv_risk_guard args (the app adds `today`, `journal`, `rules_path`)
  journal: {...}         # tv_journal_load args (the app adds `path_or_name`)
  prompts: {event, verdict, review}   # templates; {event_json} {events_json} {state_json} {date} ...
  pollFloorSeconds: 5
```

Rules the code enforces, and that a new workflow must not have to restate:

- **The sections are permissive records** passed to the tool almost verbatim, so a second
  workflow declares another instrument, session or rules file with **no code change**. The
  tool validates them (pydantic, `extra="forbid"`); a bad section shows as a call error in
  the panel, not a crash at load.
- **No strategy number ever lives in TypeScript.** Sessions, ranges, thresholds, prop limits
  and prompt text belong in the yaml, the prompt block or the user's own rules JSON.
- **Polling**: the interval is whatever the run's `next_poll_after_s` asks for, clamped to
  [5 s, 15 min]; a stopped, finished, session-closed or replayed run is never polled again;
  the loop is chained, so one run never has two polls in flight.
- **One turn per poll at most**, for the newest event whose type is in `askModelOn`; the
  prompt carries every qualifying event of that poll. A replay sends no turns at all.
- The run id is `desk-<workflowId>-<date>`, so a restart **reattaches** (an "already exists"
  refusal from `tv_sentinel_start` is the signal to attach, not an error to show).

## The `tv:call` allowlist

The renderer names a tool, never a command. `src/shared/panel.ts` holds the list and
`src/main/ipc.ts` enforces it before anything is spawned:

```
tv_sentinel_start  tv_sentinel_poll  tv_sentinel_status  tv_sentinel_stop
tv_scan_levels     tv_calendar_check tv_risk_guard       tv_journal_load
tv_desktop_status
```

Read-and-poll only. Nothing here draws on the user's chart, writes a journal row, saves a
Pine script or places anything. Adding a tool to this list is a security decision: it must
be read-only or write nothing but its own run file, and the reason goes in the commit.

## Stage roadmap

0. Chat shell: SDK bridge, lights, quick actions, chats/shelves, settings, help (this stage).
1. Live verification with TradingView + tvmcp, polish streaming edge cases, `canUseTool` scoping for the extended preset.
2. Structured cards: render `nextsteps/chartcard/stressreport/prodata/briefreport/healthdata/workflowstate` natively.
3. Workflows per shelf: strategy builder, indicator (Pine) builder, performance review, journal, IB day.
4. The day panel: deterministic CLI polling, the `panel` contract, the IB-day and V1B3 workflows.
5. Document import (pdf-parse), attachments in chat, exports, packaging: signed installers,
   electron-updater, official MCP auth flow.
