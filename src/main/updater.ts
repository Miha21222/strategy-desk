// In-app updates: GitHub Releases of this repository through electron-updater.
//
// One state machine, owned here; the renderer reads it on mount
// (`update:state`) and follows `update:event`. Rules the module keeps:
//   * nothing downloads by itself — a check only ASKS GitHub, the download and
//     the install are each a press (the owner's rule for everything the app
//     does for the user, see tasks.ts). The automatic part is the check alone,
//     ~30 s after start and every few hours, switchable in the settings;
//   * never install while the app is busy: a chat that streams or a task that
//     runs may be using `claude.exe` from the install folder (the scheduled
//     review does exactly that at 09:30 / 16:00 / 20:00), and a silent
//     installer replacing a locked binary is a half-updated install;
//   * never install on quit (autoInstallOnAppQuit = false) for the same reason:
//     the owner closes the window while the scheduler's run is still writing;
//   * release notes are not shown: the GitHub feed hands them over as HTML and
//     the renderer would have to inject it. The state carries the release URL.
// A private repository answers 404 without a token; the `ghToken` secret
// (safeStorage) goes into the feed options at check time, never into a file.
import { createRequire } from 'node:module';
import { app } from 'electron';
import { autoUpdater, type ProgressInfo, type UpdateInfo } from 'electron-updater';
import { APP_LINKS, type UpdateInstallResult, type UpdateState } from '@shared/api';
import { anyChatActive } from './agent';
import * as store from './store';
import { anyTaskRunning } from './tasks';

/** First automatic check after start, then the interval between checks. */
export const AUTO_CHECK_DELAY_MS = 30_000;
export const AUTO_CHECK_EVERY_MS = 4 * 60 * 60_000;

const [OWNER, REPO] = APP_LINKS.repo.replace(/^https:\/\/github\.com\//, '').split('/');

let state: UpdateState = { status: app.isPackaged ? 'idle' : 'dev', current: app.getVersion() };
const listeners = new Set<(s: UpdateState) => void>();
let wired = false;
let checking: Promise<UpdateState> | null = null;
let downloading: Promise<UpdateState> | null = null;
let timer: NodeJS.Timeout | null = null;

export function updateState(): UpdateState {
  return state;
}

export function onUpdate(cb: (s: UpdateState) => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

function set(patch: Partial<UpdateState>): UpdateState {
  state = { ...state, ...patch, current: app.getVersion() };
  for (const cb of listeners) cb(state);
  return state;
}

/** The release page of a version: what the «Что нового» link opens. */
export function releaseUrl(version: string): string {
  return `${APP_LINKS.releases}/tag/v${version}`;
}

/** One line of an updater error, safe to show: the first line, trimmed, capped. */
export function shortError(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e ?? '');
  const line = raw.split(/\r?\n/).find((l) => l.trim()) ?? '';
  return line.trim().slice(0, 200) || 'unknown error';
}

function wire(): void {
  if (wired) return;
  wired = true;
  autoUpdater.autoDownload = false;
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.allowPrerelease = false;
  autoUpdater.logger = {
    info: (m: unknown) => console.log('[update]', m),
    warn: (m: unknown) => console.warn('[update]', m),
    error: (m: unknown) => console.error('[update]', m),
    debug: () => undefined,
  };
  autoUpdater.on('checking-for-update', () => set({ status: 'checking', error: undefined }));
  autoUpdater.on('update-available', (info: UpdateInfo) =>
    set({ status: 'available', version: info.version, date: info.releaseDate, url: releaseUrl(info.version), percent: undefined, bytes: undefined, total: undefined, checkedAt: Date.now(), error: undefined }),
  );
  autoUpdater.on('update-not-available', () => set({ status: 'none', version: undefined, date: undefined, url: undefined, checkedAt: Date.now(), error: undefined }));
  autoUpdater.on('download-progress', (p: ProgressInfo) => set({ status: 'downloading', percent: Math.round(p.percent), bytes: p.transferred, total: p.total }));
  autoUpdater.on('update-downloaded', (info: UpdateInfo) => set({ status: 'downloaded', version: info.version, url: releaseUrl(info.version), percent: 100, error: undefined }));
  autoUpdater.on('error', (e: Error) => set({ status: 'error', error: shortError(e) }));
}

/** Point electron-updater at this repository, with the owner's token when there is one (private repository). */
async function feed(): Promise<void> {
  const { ghToken } = await store.getSecrets();
  autoUpdater.setFeedURL(ghToken ? { provider: 'github', owner: OWNER, repo: REPO, private: true, token: ghToken } : { provider: 'github', owner: OWNER, repo: REPO });
}

/** Ask GitHub once. Resolves with the state it left behind; a check already running is joined, not doubled. */
export function checkForUpdates(): Promise<UpdateState> {
  if (state.status === 'dev') return Promise.resolve(state);
  if (checking) return checking;
  if (downloading) return downloading;
  checking = (async () => {
    wire();
    try {
      await feed();
      await autoUpdater.checkForUpdates();
    } catch (e) {
      set({ status: 'error', error: shortError(e) });
    } finally {
      checking = null;
    }
    return state;
  })();
  return checking;
}

/** Fetch the installer of the version the last check found; electron-updater verifies its sha512 against latest.yml. */
export function downloadUpdate(): Promise<UpdateState> {
  if (state.status !== 'available') return Promise.resolve(state);
  if (downloading) return downloading;
  downloading = (async () => {
    wire();
    try {
      set({ status: 'downloading', percent: 0 });
      await autoUpdater.downloadUpdate();
    } catch (e) {
      set({ status: 'error', error: shortError(e) });
    } finally {
      downloading = null;
    }
    return state;
  })();
  return downloading;
}

/**
 * Quit and run the downloaded installer silently, then relaunch. Refused, not
 * failed, while a chat streams or a task runs — the renderer says so and the
 * user presses again later.
 */
export function installUpdate(): UpdateInstallResult {
  if (state.status === 'dev') return { ok: false, reason: 'dev' };
  if (state.status !== 'downloaded') return { ok: false, reason: 'not-downloaded' };
  if (anyChatActive() || anyTaskRunning()) return { ok: false, reason: 'busy' };
  setImmediate(() => autoUpdater.quitAndInstall(true, true));
  return { ok: true };
}

/**
 * The automatic check: only after the wizard was finished, only when the
 * setting allows, and never over a download in flight. Each tick re-reads the
 * settings, so switching the checkbox off takes effect without a restart.
 */
export function startAutoCheck(): void {
  if (state.status === 'dev' || timer) return;
  const tick = async () => {
    try {
      const cfg = await store.getSettings();
      if (!cfg.autoCheckUpdates || !cfg.setupCompletedAt) return;
      if (state.status === 'downloading' || state.status === 'downloaded') return;
      await checkForUpdates();
    } catch (e) {
      console.warn('[update] auto check failed:', shortError(e));
    }
  };
  timer = setTimeout(() => {
    void tick();
    timer = setInterval(() => void tick(), AUTO_CHECK_EVERY_MS);
  }, AUTO_CHECK_DELAY_MS);
}

/**
 * DESK_SMOKE=update (packaged build only): ask GitHub for real, log what came
 * back, and — with DESK_UPDATE_PRETEND=<version> making the running app look
 * older — download the installer so electron-updater checks its sha512 against
 * latest.yml. Never installs. The token comes from the stored secret or, for
 * the smoke only, from GH_TOKEN in the environment.
 */
export async function runUpdateSmoke(): Promise<void> {
  console.log('[update] app', app.getVersion(), `packaged=${app.isPackaged}`, `repo=${OWNER}/${REPO}`);
  if (!app.isPackaged) {
    console.log('[update] not packaged: nothing to check (electron-updater needs resources/app-update.yml)');
    return;
  }
  wire();
  const pretend = (process.env.DESK_UPDATE_PRETEND ?? '').trim();
  if (pretend) {
    // electron-updater compares with ITS semver (7.x); the hoisted top-level
    // semver is another major and its objects fail the instanceof check.
    const require = createRequire(__filename);
    const semver = createRequire(require.resolve('electron-updater'))('semver') as { parse(v: string): unknown };
    const v = semver.parse(pretend);
    if (!v) throw new Error(`DESK_UPDATE_PRETEND is not a version: ${pretend}`);
    (autoUpdater as unknown as { currentVersion: unknown }).currentVersion = v;
    console.log('[update] pretending to be', pretend);
  }
  const envToken = (process.env.GH_TOKEN ?? '').trim();
  const { ghToken } = await store.getSecrets();
  const token = ghToken || envToken;
  console.log('[update] token', ghToken ? 'from settings' : envToken ? 'from GH_TOKEN' : 'none');
  autoUpdater.setFeedURL(token ? { provider: 'github', owner: OWNER, repo: REPO, private: true, token } : { provider: 'github', owner: OWNER, repo: REPO });
  try {
    const r = await autoUpdater.checkForUpdates();
    console.log('[update] check', JSON.stringify({ status: state.status, found: r?.updateInfo?.version ?? null, files: r?.updateInfo?.files?.map((f) => f.url) ?? [] }));
  } catch (e) {
    throw new Error(`check failed: ${shortError(e)}`);
  }
  // `state` is reassigned by the event handlers, which TypeScript cannot see
  // across the awaits above; read it through the accessor.
  const after = updateState();
  if (after.status === 'error') throw new Error(`check failed: ${after.error}`);
  if (after.status !== 'available') {
    console.log('[update] nothing newer than', pretend || app.getVersion());
    return;
  }
  console.log('[update] downloading', after.version, 'from', after.url);
  const files = await autoUpdater.downloadUpdate();
  const done = updateState();
  console.log('[update] downloaded', JSON.stringify(files), `status=${done.status}`);
  if (done.status !== 'downloaded') throw new Error(`download ended in ${done.status}: ${done.error ?? ''}`);
  console.log('[update] ok — sha512 verified by electron-updater; not installing');
}
