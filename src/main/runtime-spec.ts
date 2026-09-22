// How tvmcp is started — the pure half.
//
// Strategy Desk ships tvmcp with the installer (`extraResources`) and provisions
// its Python environment into `userData` on first run. That is the BUNDLED mode.
// A checkout on the developer's disk, driven by a `uv` from PATH, is the
// EXTERNAL mode; it stays, because in a dev run there is no payload in
// `resources` and because a user with their own clone must keep working.
//
// Everything Electron-shaped (userData, resourcesPath, fs) lives in
// `runtime.ts`. This file only turns a `RuntimePaths` into an environment and a
// spawn spec, so `mcp.ts`, `status.ts` and `tv-cli.ts` can all be unit-tested
// without booting an app — and so all three spawn tvmcp exactly alike.
import path from 'node:path';
import type { Settings } from '@shared/api';
import { DEFAULT_CDP_PORT, DEFAULT_TOOLSETS } from '@shared/defaults';

export type RuntimeMode = 'bundled' | 'external';

export interface RuntimePaths {
  /** The shipped tvmcp payload (pyproject + uv.lock + src); null in a dev run. */
  payloadDir: string | null;
  /** The shipped uv binary; null in a dev run, where uv comes from PATH. */
  uvBin: string | null;
  /** UV_PROJECT_ENVIRONMENT — the venv we create and own. */
  venvDir: string;
  /** UV_CACHE_DIR. */
  cacheDir: string;
  /** UV_PYTHON_INSTALL_DIR — where uv puts the managed interpreter. */
  pythonInstallDir: string;
  /** NUMBA_CACHE_DIR — numba writes next to its code otherwise, i.e. into resources. */
  numbaCacheDir: string;
  /** PYTHONPYCACHEPREFIX — same reason, for .pyc files. */
  pycacheDir: string;
  /** PLAYWRIGHT_BROWSERS_PATH — the headless shell, installed on demand. */
  browsersDir: string;
  /** Where the "provisioned, with this lock" marker is written. */
  markerPath: string;
  /**
   * The shipped gt-rag payload: `server.py`, the `gt_rag` package and the course
   * content (`raw/`, `images/`, `store/`). Null when the installer was built
   * without the knowledge base — the app then has no `gt-database` server and
   * everything else works exactly as before.
   */
  kbPayloadDir: string | null;
  /** GT_RAG_DATA — the writable half: ChromaDB opens its sqlite read-write even to answer a query. */
  kbDataDir: string;
  /** FASTEMBED_CACHE_PATH — the embedding model (~240 MB), seeded from the payload or fetched once. */
  kbModelDir: string;
  /** Where the "store seeded, from this payload" marker is written. */
  kbMarkerPath: string;
}

/** A dev run: no payload, no bundled uv, and no directories of our own. */
export const EXTERNAL_PATHS: RuntimePaths = {
  payloadDir: null,
  uvBin: null,
  venvDir: '',
  cacheDir: '',
  pythonInstallDir: '',
  numbaCacheDir: '',
  pycacheDir: '',
  browsersDir: '',
  markerPath: '',
  kbPayloadDir: null,
  kbDataDir: '',
  kbModelDir: '',
  kbMarkerPath: '',
};

export function modeOf(paths: RuntimePaths): RuntimeMode {
  return paths.payloadDir ? 'bundled' : 'external';
}

// The one set of paths this process runs with. `runtime.ts` fills it once at
// boot, from Electron's anchors; everything else reads it. Kept here, in the
// Electron-free half, so `mcp.ts`, `status.ts` and `tv-cli.ts` never import
// electron and stay unit-testable — a test run simply sees EXTERNAL_PATHS,
// which is exactly how a developer's machine behaves.
let CURRENT: RuntimePaths = EXTERNAL_PATHS;

export function setRuntimePaths(paths: RuntimePaths): void {
  CURRENT = paths;
}

export function runtimePaths(): RuntimePaths {
  return CURRENT;
}

/** The project directory tvmcp is run from, whichever mode we are in. */
export function projectDir(cfg: Settings, paths: RuntimePaths): string {
  return paths.payloadDir ?? (cfg.tvmcpPath || '').trim();
}

/** Windows puts a venv's executables in Scripts/, everyone else in bin/. */
export function pythonBin(paths: RuntimePaths, platform: string = process.platform): string {
  if (!paths.venvDir) return '';
  return platform === 'win32' ? path.join(paths.venvDir, 'Scripts', 'python.exe') : path.join(paths.venvDir, 'bin', 'python');
}

/**
 * The environment tvmcp runs with: the app's own, plus the two knobs the
 * settings own, plus — in bundled mode — every cache location that would
 * otherwise be written inside the installation directory.
 *
 * `TV_SESSIONID` (the opt-in account-cookie feed) and `TV_SENTINEL_DIR` are
 * inherited from the environment when the user set them — deliberately not app
 * settings, because the cookie must not sit in config.json.
 */
export function buildTvEnv(cfg: Settings, baseEnv: NodeJS.ProcessEnv, paths: RuntimePaths): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(baseEnv)) if (typeof v === 'string') env[k] = v;
  env.TV_TOOLSETS = cfg.toolsets?.trim() || DEFAULT_TOOLSETS;
  env.TV_CDP_URL = `http://127.0.0.1:${cfg.cdpPort || DEFAULT_CDP_PORT}`;
  if (modeOf(paths) === 'bundled') Object.assign(env, runtimeEnv(paths));
  return env;
}

/** The cache and install locations, shared by the server, the CLI and `uv sync`. */
export function runtimeEnv(paths: RuntimePaths): Record<string, string> {
  const env: Record<string, string> = {};
  if (paths.venvDir) env.UV_PROJECT_ENVIRONMENT = paths.venvDir;
  if (paths.cacheDir) env.UV_CACHE_DIR = paths.cacheDir;
  if (paths.pythonInstallDir) env.UV_PYTHON_INSTALL_DIR = paths.pythonInstallDir;
  if (paths.numbaCacheDir) env.NUMBA_CACHE_DIR = paths.numbaCacheDir;
  if (paths.pycacheDir) env.PYTHONPYCACHEPREFIX = paths.pycacheDir;
  if (paths.browsersDir) env.PLAYWRIGHT_BROWSERS_PATH = paths.browsersDir;
  return env;
}

export interface SpawnSpec {
  command: string;
  args: string[];
}

/**
 * How to run tvmcp: `server` is the MCP server the SDK talks to, `cli` is the
 * `tv` entry point the day panel polls with.
 *
 * Bundled mode calls the venv's own interpreter, so nothing depends on `uv`
 * being on PATH after the first provisioning. `python -m tvmcp.cli` is used
 * rather than the generated `tv.exe` shim: the module has a `__main__` guard,
 * and one spawn shape is easier to keep honest than two.
 */
export function tvmcpSpawn(cfg: Settings, paths: RuntimePaths, kind: 'server' | 'cli', platform: string = process.platform): SpawnSpec {
  const module = kind === 'server' ? 'tvmcp' : 'tvmcp.cli';
  if (modeOf(paths) === 'bundled') {
    return { command: pythonBin(paths, platform), args: ['-m', module] };
  }
  return { command: 'uv', args: ['--directory', projectDir(cfg, paths), 'run', 'python', '-m', module] };
}

// ---------- the knowledge base (gt-rag) ----------
//
// The course knowledge base is a SECOND MCP server over the SAME Python
// environment: tvmcp's `uv.lock` carries gt-rag's dependencies in the optional
// `kb` extra, so provisioning builds one venv for both. That is why gt-rag's
// own `server.py` prefers fastmcp — tvmcp pins `mcp<2` through fastmcp, and two
// environments for two servers would double ~600 MB for no gain.
//
// Where its three directories live is decided here and nowhere else:
//   * the CODE and the read-only content stay in the payload (installation dir);
//   * the ChromaDB store is copied into userData, because chroma opens sqlite
//     read-write even for a query and the installation directory may be
//     read-only (and is wiped by the uninstaller);
//   * the embedding model lives in userData too, for the same reason.

export type KbMode = 'bundled' | 'external' | 'off';

/**
 * Bundled = the installer carried gt-rag. External = a developer's own clone,
 * named in the settings and using the venv that clone already has. Off = no
 * payload and no clone: the app runs without a `gt-database` server.
 */
export function kbMode(cfg: Settings, paths: RuntimePaths): KbMode {
  if (!kbEnabled(cfg)) return 'off';
  if (paths.kbPayloadDir) return 'bundled';
  return (cfg.gtRagPath || '').trim() ? 'external' : 'off';
}

/** The switch in the settings; absent in an old config file means "on". */
export function kbEnabled(cfg: Settings): boolean {
  return cfg.knowledgeBase !== false;
}

/** The gt-rag checkout a dev run talks to, or '' when there is none. */
export function kbProjectDir(cfg: Settings, paths: RuntimePaths): string {
  return paths.kbPayloadDir ?? (cfg.gtRagPath || '').trim();
}

/**
 * How to run the knowledge base server: `<python> <dir>/server.py`.
 *
 * Bundled mode uses the interpreter the app provisioned (the one that has the
 * `kb` extra); external mode uses the `.venv` of the clone, because a developer
 * who has a gt-rag checkout also has its environment, and gt-rag is not a uv
 * project — there is no `uv run` to fall back on.
 */
export function gtRagSpawn(cfg: Settings, paths: RuntimePaths, platform: string = process.platform): SpawnSpec | null {
  const dir = kbProjectDir(cfg, paths);
  if (!dir) return null;
  const script = path.join(dir, 'server.py');
  if (paths.kbPayloadDir) {
    const python = pythonBin(paths, platform);
    return python ? { command: python, args: [script] } : null;
  }
  const venvPython = platform === 'win32' ? path.join(dir, '.venv', 'Scripts', 'python.exe') : path.join(dir, '.venv', 'bin', 'python');
  return { command: venvPython, args: [script] };
}

/**
 * The environment the knowledge base server runs with.
 *
 * `PYTHONIOENCODING` is not cosmetic: the content is Russian, the server writes
 * JSON to stdout, and a Windows console default of cp1251 turns the first «→»
 * in a hit into a UnicodeEncodeError — i.e. a server that dies mid-answer.
 */
export function buildKbEnv(cfg: Settings, baseEnv: NodeJS.ProcessEnv, paths: RuntimePaths): Record<string, string> {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(baseEnv)) if (typeof v === 'string') env[k] = v;
  env.PYTHONIOENCODING = 'utf-8';
  // The app promises no telemetry, and both libraries under the knowledge base
  // opt IN by default: chromadb's `anonymized_telemetry` is `True` and posts
  // client events to PostHog on every PersistentClient, and huggingface_hub
  // sends its own on a model load. Set in every mode, including a developer's
  // clone — the promise is not about which payload shipped.
  env.ANONYMIZED_TELEMETRY = 'False';
  env.HF_HUB_DISABLE_TELEMETRY = '1';
  const payload = paths.kbPayloadDir;
  if (!payload) {
    // A developer's clone brings its own data and its own venv; all it needs
    // from us is to be importable whatever directory the app was started from.
    const clone = (cfg.gtRagPath || '').trim();
    if (clone) env.PYTHONPATH = [clone, baseEnv.PYTHONPATH].filter(Boolean).join(path.delimiter);
    return env;
  }
  Object.assign(env, runtimeEnv(paths));
  // The package is imported from the payload; nothing is installed into the venv.
  env.PYTHONPATH = [payload, baseEnv.PYTHONPATH].filter(Boolean).join(path.delimiter);
  env.GT_RAG_DATA = paths.kbDataDir;
  env.GT_RAG_RAW = path.join(payload, 'raw');
  env.GT_RAG_IMAGES = path.join(payload, 'images');
  if (paths.kbModelDir) env.FASTEMBED_CACHE_PATH = paths.kbModelDir;
  return env;
}

/** Where the store is seeded FROM, inside the payload. */
export function kbSeedDir(paths: RuntimePaths): string {
  return paths.kbPayloadDir ? path.join(paths.kbPayloadDir, 'store') : '';
}

/** Where the embedding model is seeded FROM; the payload may ship without one. */
export function kbModelSeedDir(paths: RuntimePaths): string {
  return paths.kbPayloadDir ? path.join(paths.kbPayloadDir, 'model') : '';
}

/**
 * The knowledge base's own boot probe: import the server and ask the store how
 * many chunks it holds. It exercises exactly what a real session needs — the
 * `kb` extra in the venv, the seeded store, the model cache and the env split —
 * without an MCP handshake, and it writes nothing.
 */
export function kbCheckSpec(cfg: Settings, paths: RuntimePaths, platform: string = process.platform): { spec: SpawnSpec; env: Record<string, string> } | null {
  const base = gtRagSpawn(cfg, paths, platform);
  if (!base) return null;
  return {
    spec: { command: base.command, args: ['-c', 'import json, server; print("chunks", json.loads(server.gt_stats())["total_chunks"], "pages", json.loads(server.gt_stats())["pages_on_disk"])'] },
    env: buildKbEnv(cfg, process.env, paths),
  };
}

/**
 * Downloading the embedding model, when the installer did not carry one.
 * fastembed caches it under FASTEMBED_CACHE_PATH; constructing the model is
 * what fetches it, and the same call proves it loads.
 */
export function kbModelSpec(cfg: Settings, paths: RuntimePaths, platform: string = process.platform): { spec: SpawnSpec; env: Record<string, string> } | null {
  const python = pythonBin(paths, platform);
  if (!python || !paths.kbPayloadDir) return null;
  return {
    spec: {
      command: python,
      args: ['-c', 'from gt_rag.common import EMBED_MODEL; from fastembed import TextEmbedding; TextEmbedding(model_name=EMBED_MODEL); print("model ready", EMBED_MODEL)'],
    },
    env: buildKbEnv(cfg, process.env, paths),
  };
}
