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
src/shared/defaults.ts      default settings / profile
src/main/index.ts           BrowserWindow, app lifecycle
src/main/ipc.ts             ipcMain.handle/on registration (status, tv, settings, profile, chats, chat, open)
src/main/store.ts           JSON files in userData (config/chats/profile/transcripts), safeStorage secrets, atomic writes
src/main/agent.ts           query() wrapper: system prompt, env, permission presets, SDKMessage → ChatEvent, retry on crash
src/main/mcp.ts             mcpServers map (tradingview stdio; optional official http server)
src/main/status.ts          lights: TradingView process, CDP /json/version + chart tab, tvmcp --check (cached 60 s), Claude auth
src/main/tv-launch.ts       launch TradingView with --remote-debugging-port, never kills a running instance
src/preload/index.ts        contextBridge → window.desk
src/renderer/app.tsx        shell: sidebar + tabs, owns state and the single chat:event subscription
src/renderer/chat/          blocks.ts (fenced-block parser + test), Markdown.tsx, ChatView.tsx
src/renderer/components/    Sidebar, SettingsTab, HelpTab
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
- Commit messages end with `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.

## Stage roadmap

0. Chat shell: SDK bridge, lights, quick actions, chats/shelves, settings, help (this stage).
1. Live verification with TradingView + tvmcp, polish streaming edge cases, `canUseTool` scoping for the extended preset.
2. Structured cards: render `nextsteps/chartcard/stressreport/prodata/briefreport/healthdata/workflowstate` natively.
3. Workflows per shelf: strategy builder, indicator (Pine) builder, performance review, journal, IB day.
4. Document import (pdf-parse), attachments in chat, exports.
5. Packaging: signed installers, electron-updater, official MCP auth flow.
