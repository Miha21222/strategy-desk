// How tvmcp is started — the Electron half: where the shipped payload is, where
// the provisioned environment goes, and whether it is currently usable.
//
// READ-ONLY BY CONSTRUCTION. Nothing here runs `uv sync`, downloads a browser
// or touches the network; it only reports. The provisioning itself is a task in
// `tasks.ts`, and a task starts only when the user presses a button. That is
// the same rule the preflight has always kept, narrowed to its real shape: the
// app may act, but never on its own initiative.
import { app } from 'electron';
import { createHash } from 'node:crypto';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import type { Settings } from '@shared/api';
import type { KbFact, RuntimeFact } from '@shared/preflight';
import { binaryCandidates } from './resources';
import {
  EXTERNAL_PATHS,
  gtRagSpawn,
  kbEnabled,
  kbMode,
  kbModelSeedDir,
  kbProjectDir,
  kbSeedDir,
  modeOf,
  projectDir,
  pythonBin,
  runtimeEnv,
  setRuntimePaths,
  tvmcpSpawn,
  type RuntimePaths,
  type SpawnSpec,
} from './runtime-spec';

export type { RuntimePaths, SpawnSpec };

/** The uv we ship, by platform. `npm run dist` puts it here (scripts/stage-tvmcp.mjs). */
const UV_NAME = process.platform === 'win32' ? 'uv.exe' : 'uv';

/** Everything provisioning creates lives under one directory, so "clear it" is one delete. */
export function runtimeRoot(): string {
  return path.join(app.getPath('userData'), 'py');
}

async function exists(p: string): Promise<boolean> {
  if (!p) return false;
  try {
    await fs.access(p);
    return true;
  } catch {
    return false;
  }
}

/** First candidate that exists, or null. Binaries and payload directories only. */
async function firstExisting(rel: string[]): Promise<string | null> {
  for (const p of binaryCandidates(...rel)) if (await exists(p)) return p;
  return null;
}

/**
 * Work out this installation's paths once. In a dev run nothing is found in
 * `resources`, so the result is EXTERNAL_PATHS and the app behaves exactly as
 * it did before bundling: `uv` from PATH, the clone from the settings.
 */
export async function initRuntimePaths(): Promise<RuntimePaths> {
  const payloadDir = await firstExisting(['tvmcp', 'pyproject.toml']);
  const uvBin = await firstExisting([UV_NAME]);
  // The knowledge base is looked up on its own: an installer may carry tvmcp
  // without it (a build with SD_BUNDLE_KB=0), and a dev run may fake one.
  const kbServer = await firstExisting(['gt-rag', 'server.py']);
  const root = runtimeRoot();
  const kb = kbServer
    ? {
        kbPayloadDir: path.dirname(kbServer),
        kbDataDir: path.join(root, 'kb', 'data'),
        kbModelDir: path.join(root, 'kb', 'model'),
        kbMarkerPath: path.join(root, 'kb', 'seeded.json'),
      }
    : { kbPayloadDir: null, kbDataDir: '', kbModelDir: '', kbMarkerPath: '' };

  if (!payloadDir) {
    // No tvmcp payload: a dev run. The knowledge base still reports whatever it
    // found, so a faked kb payload can be exercised without faking tvmcp too.
    const paths = { ...EXTERNAL_PATHS, ...kb };
    setRuntimePaths(paths);
    return paths;
  }
  const paths: RuntimePaths = {
    payloadDir: path.dirname(payloadDir),
    uvBin,
    venvDir: path.join(root, 'venv'),
    cacheDir: path.join(root, 'cache'),
    pythonInstallDir: path.join(root, 'python'),
    numbaCacheDir: path.join(root, 'numba'),
    pycacheDir: path.join(root, 'pycache'),
    browsersDir: path.join(root, 'ms-playwright'),
    markerPath: path.join(root, 'provisioned.json'),
    ...kb,
  };
  setRuntimePaths(paths);
  return paths;
}

/**
 * The shipped vault template (`scripts/stage-vault.mjs` → `resources/vault-template`,
 * shipped as `extraResources` → `<resources>/vault-template`). A real directory
 * on disk, because the task copies out of it with plain fs and the user's
 * Obsidian then opens the result; so it is looked up like the payloads, with
 * the dev checkout's `resources/` as the first candidate. Null when nothing
 * shipped — the wizard then says so instead of offering a button.
 */
export async function vaultTemplateDir(): Promise<string | null> {
  const marker = (await firstExisting(['resources', 'vault-template', 'template.json'])) ?? (await firstExisting(['vault-template', 'template.json']));
  return marker ? path.dirname(marker) : null;
}

/**
 * The Claude Code binary. The Agent SDK ships one per platform and spawns it
 * itself; the MCP tasks need the same one, because a packaged install on a
 * machine with no `claude` on PATH would otherwise report every check as
 * "not installed" when it is merely unreachable.
 */
export function claudeBin(): string {
  const pkg = `@anthropic-ai/claude-agent-sdk-${process.platform}-${process.arch}`;
  const exe = process.platform === 'win32' ? 'claude.exe' : 'claude';
  try {
    // The platform package is asarUnpacked, so this resolves to a real file.
    const entry = require.resolve(`${pkg}/package.json`);
    return path.join(path.dirname(entry), exe);
  } catch {
    return 'claude';
  }
}

interface Marker {
  lockHash: string;
  uv: string;
  at: number;
}

/** sha256 of uv.lock — the one thing that decides whether the venv is current. */
export async function lockHash(payloadDir: string | null): Promise<string> {
  if (!payloadDir) return '';
  try {
    const raw = await fs.readFile(path.join(payloadDir, 'uv.lock'));
    return createHash('sha256').update(raw).digest('hex').slice(0, 16);
  } catch {
    return '';
  }
}

async function readMarker(p: string): Promise<Marker | null> {
  try {
    const raw = await fs.readFile(p, 'utf8');
    const m = JSON.parse(raw) as Marker;
    return typeof m?.lockHash === 'string' ? m : null;
  } catch {
    return null;
  }
}

export async function writeMarker(paths: RuntimePaths, uv: string): Promise<void> {
  const marker: Marker = { lockHash: await lockHash(paths.payloadDir), uv, at: Date.now() };
  await fs.mkdir(path.dirname(paths.markerPath), { recursive: true });
  await fs.writeFile(paths.markerPath, JSON.stringify(marker, null, 2), 'utf8');
}

/** Name of the project in the payload's pyproject.toml — an installation integrity check. */
async function payloadProject(dir: string | null): Promise<string> {
  if (!dir) return '';
  try {
    const raw = await fs.readFile(path.join(dir, 'pyproject.toml'), 'utf8');
    const m = raw.match(/^\s*name\s*=\s*["']([^"']+)["']/m);
    return m ? m[1] : '';
  } catch {
    return '';
  }
}

/**
 * What the preflight needs to know, without running anything: is the payload
 * intact, is the environment there, and was it built from this uv.lock.
 */
export async function runtimeFact(cfg: Settings, paths: RuntimePaths): Promise<RuntimeFact> {
  const mode = modeOf(paths);
  const fact: RuntimeFact = {
    mode,
    tvmcpDir: projectDir(cfg, paths),
    payloadOk: false,
    projectName: '',
    provisioned: false,
    stale: false,
    pythonPath: pythonBin(paths),
    uvPath: paths.uvBin ?? '',
    lockHash: '',
    chromium: null,
  };
  if (mode === 'external') return fact;

  fact.projectName = await payloadProject(paths.payloadDir);
  fact.payloadOk = fact.projectName === 'tvmcp' && Boolean(await exists(path.join(paths.payloadDir ?? '', 'uv.lock')));
  fact.lockHash = await lockHash(paths.payloadDir);

  const marker = await readMarker(paths.markerPath);
  const hasPython = await exists(fact.pythonPath);
  fact.provisioned = Boolean(marker && hasPython && marker.lockHash === fact.lockHash && fact.lockHash);
  // "Built, but from a different lock" is its own answer: the app still runs,
  // it just may be missing a dependency the new payload expects.
  fact.stale = Boolean(marker && hasPython && !fact.provisioned);
  // Only worth answering once there is an environment to install it into.
  fact.chromium = fact.provisioned || fact.stale ? await hasHeadlessShell(paths.browsersDir) : null;
  return fact;
}

// ---------- the knowledge base ----------

interface KbMarker {
  seedHash: string;
  at: number;
}

/**
 * What identifies the shipped store: gt-rag's own `index_state.json`, which is
 * a hash per indexed page. A new app version with re-indexed content gets a new
 * value, and the seeded copy in userData is then stale — exactly the question
 * the marker has to answer, and the one thing that is cheap to read.
 */
export async function kbSeedHash(paths: RuntimePaths): Promise<string> {
  const seed = kbSeedDir(paths);
  if (!seed) return '';
  try {
    const raw = await fs.readFile(path.join(seed, 'index_state.json'));
    return createHash('sha256').update(raw).digest('hex').slice(0, 16);
  } catch {
    return '';
  }
}

export async function writeKbMarker(paths: RuntimePaths): Promise<void> {
  const marker: KbMarker = { seedHash: await kbSeedHash(paths), at: Date.now() };
  await fs.mkdir(path.dirname(paths.kbMarkerPath), { recursive: true });
  await fs.writeFile(paths.kbMarkerPath, JSON.stringify(marker, null, 2), 'utf8');
}

async function readKbMarker(p: string): Promise<KbMarker | null> {
  try {
    const m = JSON.parse(await fs.readFile(p, 'utf8')) as KbMarker;
    return typeof m?.seedHash === 'string' ? m : null;
  } catch {
    return null;
  }
}

/** Does this fastembed cache hold the model, rather than merely exist? */
async function hasEmbeddingModel(dir: string): Promise<boolean> {
  if (!dir) return false;
  try {
    const entries = await fs.readdir(dir);
    // huggingface_hub's layout: models--<org>--<name>. fastembed pulls the
    // model from a qdrant mirror, so match the family, not one repository name.
    return entries.some((name) => name.startsWith('models--') && name.includes('MiniLM'));
  } catch {
    return false;
  }
}

/** What the preflight needs to know about the knowledge base, without running anything. */
export async function kbFact(cfg: Settings, paths: RuntimePaths): Promise<KbFact> {
  const mode = kbMode(cfg, paths);
  const fact: KbFact = {
    mode,
    enabled: kbEnabled(cfg),
    dir: kbProjectDir(cfg, paths),
    payloadOk: false,
    seeded: false,
    stale: false,
    model: false,
    modelShipped: false,
    pages: 0,
  };
  if (mode === 'off') return fact;

  if (mode === 'external') {
    const spawn = gtRagSpawn(cfg, paths);
    fact.payloadOk = Boolean(spawn && (await exists(spawn.command)) && (await exists(spawn.args[0])));
    // A clone brings its own data and its own model cache; nothing of ours to report.
    fact.seeded = fact.payloadOk;
    fact.model = fact.payloadOk;
    return fact;
  }

  const payload = paths.kbPayloadDir ?? '';
  const seedHash = await kbSeedHash(paths);
  fact.payloadOk = Boolean(seedHash) && (await exists(path.join(payload, 'server.py'))) && (await exists(path.join(payload, 'raw')));
  fact.pages = await countPages(path.join(payload, 'raw'));
  fact.modelShipped = await hasEmbeddingModel(kbModelSeedDir(paths));

  const marker = await readKbMarker(paths.kbMarkerPath);
  const hasStore = await exists(path.join(paths.kbDataDir, 'chroma'));
  fact.seeded = Boolean(marker && hasStore && seedHash && marker.seedHash === seedHash);
  fact.stale = Boolean(marker && hasStore && !fact.seeded);
  fact.model = await hasEmbeddingModel(paths.kbModelDir);
  return fact;
}

async function countPages(dir: string): Promise<number> {
  try {
    return (await fs.readdir(dir)).filter((n) => n.endsWith('.md')).length;
  } catch {
    return 0;
  }
}

/**
 * Seeding the store: a copy, not a download, and therefore not a spawn either.
 * The ChromaDB store and (when the installer carried one) the embedding model
 * move from the read-only payload into userData, where chroma may open its
 * sqlite read-write.
 */
export function kbSeedPlan(paths: RuntimePaths): { copies: { from: string; to: string }[]; after: () => Promise<void> } | null {
  const seed = kbSeedDir(paths);
  if (!seed || !paths.kbDataDir) return null;
  const copies = [{ from: seed, to: paths.kbDataDir }];
  const model = kbModelSeedDir(paths);
  if (model && paths.kbModelDir) copies.push({ from: model, to: paths.kbModelDir });
  return { copies, after: () => writeKbMarker(paths) };
}

/**
 * Is the headless browser there?
 *
 * playwright names its download directory after the build it pins
 * (`chromium_headless_shell-1234`), and that number moves with the playwright
 * version, so the presence of ANY such directory is the honest question — a
 * number hardcoded here would go stale the next time tvmcp bumps playwright.
 */
async function hasHeadlessShell(dir: string): Promise<boolean> {
  if (!dir) return false;
  try {
    const entries = await fs.readdir(dir);
    return entries.some((name) => name.startsWith('chromium_headless_shell-'));
  } catch {
    return false;
  }
}

/**
 * The provisioning command, assembled here and nowhere else.
 *
 * `--managed-python` on purpose: without it uv is free to build the environment
 * on whatever interpreter it finds, and on Windows that is often the Microsoft
 * Store stub, whose redirected install directory breaks venvs in ways that
 * surface much later as an import error. A uv-managed download lands in
 * `UV_PYTHON_INSTALL_DIR` — inside userData, with everything else we made.
 */
export function provisionSpec(paths: RuntimePaths): { spec: SpawnSpec; env: Record<string, string> } {
  const uv = paths.uvBin ?? 'uv';
  // ONE environment for both servers. gt-rag's dependencies ride in tvmcp's
  // optional `kb` extra (chromadb, fastembed, rapidfuzz), so the knowledge base
  // costs a bigger `uv sync` and not a second venv — and only when the
  // installer actually carried the knowledge base.
  const extra = paths.kbPayloadDir ? ['--extra', 'kb'] : [];
  return {
    spec: { command: uv, args: ['sync', '--frozen', '--no-dev', ...extra, '--managed-python', '--project', paths.payloadDir ?? ''] },
    env: runtimeEnv(paths),
  };
}

/** Installing the headless browser, once the environment exists. */
export function chromiumSpec(cfg: Settings, paths: RuntimePaths): { spec: SpawnSpec; env: Record<string, string> } {
  const base = tvmcpSpawn(cfg, paths, 'server');
  return {
    spec: { command: base.command, args: ['-m', 'playwright', 'install', 'chromium-headless-shell'] },
    env: runtimeEnv(paths),
  };
}

/**
 * Delete the provisioned environment.
 *
 * The one destructive thing in this file, and the only one the uninstaller
 * cannot do: `deleteAppDataOnUninstall` is false on purpose (chats, journals
 * and config survive a reinstall), so ~600 MB of Python would otherwise sit in
 * userData forever after the app is gone. It is never called on its own — the
 * settings ask first, and the next start offers to provision again.
 */
export async function clearRuntime(): Promise<{ removed: string }> {
  const root = runtimeRoot();
  await fs.rm(root, { recursive: true, force: true });
  return { removed: root };
}
