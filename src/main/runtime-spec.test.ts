// How tvmcp is spawned in each mode, and what environment it gets.
//
// This is the one place where a bundled install and a developer's checkout are
// allowed to differ, so it is worth pinning: the MCP server, the `--check`
// probe and the deterministic CLI all go through `tvmcpSpawn`, and a change
// that only fixed one of the three would show up here.
import { describe, expect, it } from 'vitest';
import type { Settings } from '@shared/api';
import { DEFAULT_SETTINGS } from '@shared/defaults';
import { buildKbEnv, buildTvEnv, EXTERNAL_PATHS, gtRagSpawn, kbMode, modeOf, pythonBin, runtimeEnv, tvmcpSpawn, type RuntimePaths } from './runtime-spec';

const cfg: Settings = { ...DEFAULT_SETTINGS, tvmcpPath: 'C:\\Projects\\tradingview-mcp', toolsets: 'hybrid,desktop', cdpPort: 9223 };

const bundled: RuntimePaths = {
  payloadDir: 'C:\\app\\resources\\tvmcp',
  uvBin: 'C:\\app\\resources\\uv.exe',
  venvDir: 'C:\\userData\\py\\venv',
  cacheDir: 'C:\\userData\\py\\cache',
  pythonInstallDir: 'C:\\userData\\py\\python',
  numbaCacheDir: 'C:\\userData\\py\\numba',
  pycacheDir: 'C:\\userData\\py\\pycache',
  browsersDir: 'C:\\userData\\py\\ms-playwright',
  markerPath: 'C:\\userData\\py\\provisioned.json',
  kbPayloadDir: 'C:\\app\\resources\\gt-rag',
  kbDataDir: 'C:\\userData\\py\\kb\\data',
  kbModelDir: 'C:\\userData\\py\\kb\\model',
  kbMarkerPath: 'C:\\userData\\py\\kb\\seeded.json',
};

describe('which mode we are in', () => {
  it('is decided by the payload, not by a setting the user could mistype', () => {
    expect(modeOf(EXTERNAL_PATHS)).toBe('external');
    expect(modeOf(bundled)).toBe('bundled');
  });
});

describe('the spawn spec', () => {
  it('runs the venv interpreter in bundled mode, so uv is not needed afterwards', () => {
    expect(tvmcpSpawn(cfg, bundled, 'server', 'win32')).toEqual({
      command: 'C:\\userData\\py\\venv\\Scripts\\python.exe',
      args: ['-m', 'tvmcp'],
    });
    expect(tvmcpSpawn(cfg, bundled, 'cli', 'win32').args).toEqual(['-m', 'tvmcp.cli']);
  });

  it('falls back to uv and the configured checkout in a dev run', () => {
    expect(tvmcpSpawn(cfg, EXTERNAL_PATHS, 'server')).toEqual({
      command: 'uv',
      args: ['--directory', 'C:\\Projects\\tradingview-mcp', 'run', 'python', '-m', 'tvmcp'],
    });
  });

  it('knows where a venv keeps its interpreter on each platform', () => {
    expect(pythonBin(bundled, 'win32')).toContain('Scripts');
    expect(pythonBin(bundled, 'darwin')).toContain('bin');
    expect(pythonBin(EXTERNAL_PATHS, 'win32')).toBe('');
  });
});

describe('the environment', () => {
  it('always carries the two settings knobs', () => {
    const env = buildTvEnv(cfg, {}, EXTERNAL_PATHS);
    expect(env.TV_TOOLSETS).toBe('hybrid,desktop');
    expect(env.TV_CDP_URL).toBe('http://127.0.0.1:9223');
  });

  // numba and Python write caches next to the code they load unless told
  // otherwise, and that code lives in the installation directory — read-only on
  // a per-machine install, and wiped by the uninstaller on any install.
  it('redirects every cache into userData in bundled mode, and only there', () => {
    const env = buildTvEnv(cfg, {}, bundled);
    expect(env.UV_PROJECT_ENVIRONMENT).toBe(bundled.venvDir);
    expect(env.NUMBA_CACHE_DIR).toBe(bundled.numbaCacheDir);
    expect(env.PYTHONPYCACHEPREFIX).toBe(bundled.pycacheDir);
    expect(env.PLAYWRIGHT_BROWSERS_PATH).toBe(bundled.browsersDir);
    expect(buildTvEnv(cfg, {}, EXTERNAL_PATHS).UV_PROJECT_ENVIRONMENT).toBeUndefined();
  });

  it('inherits the process environment, so TV_SESSIONID keeps working', () => {
    const env = buildTvEnv(cfg, { TV_SESSIONID: 'cookie', TV_SENTINEL_DIR: 'C:\\tmp' }, bundled);
    expect(env.TV_SESSIONID).toBe('cookie');
    expect(env.TV_SENTINEL_DIR).toBe('C:\\tmp');
  });

  it('omits an unset directory instead of exporting an empty variable', () => {
    expect(runtimeEnv({ ...bundled, browsersDir: '' }).PLAYWRIGHT_BROWSERS_PATH).toBeUndefined();
  });
});

// The knowledge base rides in the same environment as tvmcp, but its three
// directories are split on purpose: the code and the read-only content stay in
// the installation directory, the ChromaDB store and the embedding model go to
// userData. Getting that split wrong is a server that cannot open its own
// sqlite file on someone else's machine, which no dev run would ever show.
describe('the knowledge base server', () => {
  it('runs server.py with the interpreter the app provisioned', () => {
    const spec = gtRagSpawn(cfg, bundled, 'win32');
    expect(spec?.command).toBe(pythonBin(bundled, 'win32'));
    expect(spec?.args).toEqual(['C:\\app\\resources\\gt-rag\\server.py']);
  });

  it('falls back to the clone and ITS venv in a dev run, because gt-rag is not a uv project', () => {
    const dev: Settings = { ...cfg, gtRagPath: 'C:\\Projects\\gt-rag' };
    const spec = gtRagSpawn(dev, EXTERNAL_PATHS, 'win32');
    expect(spec?.command).toBe('C:\\Projects\\gt-rag\\.venv\\Scripts\\python.exe');
    expect(spec?.args).toEqual(['C:\\Projects\\gt-rag\\server.py']);
    expect(gtRagSpawn(cfg, EXTERNAL_PATHS, 'win32')).toBeNull();
  });

  it('keeps the writable half in userData and the read-only half in the payload', () => {
    const env = buildKbEnv(cfg, {}, bundled);
    expect(env.GT_RAG_DATA).toBe(bundled.kbDataDir);
    expect(env.FASTEMBED_CACHE_PATH).toBe(bundled.kbModelDir);
    expect(env.GT_RAG_RAW).toBe('C:\\app\\resources\\gt-rag\\raw');
    expect(env.GT_RAG_IMAGES).toBe('C:\\app\\resources\\gt-rag\\images');
    expect(env.PYTHONPATH).toBe('C:\\app\\resources\\gt-rag');
  });

  // Two things every mode gets, whatever shipped.
  //
  // utf-8: the content is Russian and the server answers in JSON on stdout, so a
  // Windows console default of cp1251 turns the first «→» in a hit into a
  // UnicodeEncodeError — a server that dies mid-answer.
  //
  // Telemetry: chromadb and huggingface_hub both opt IN by default, and the app
  // promises none. A developer's own clone is not an exception to that promise.
  it('pins stdout to utf-8 and switches both libraries telemetry off, bundled or not', () => {
    for (const paths of [bundled, EXTERNAL_PATHS]) {
      const env = buildKbEnv(cfg, {}, paths);
      expect(env.PYTHONIOENCODING).toBe('utf-8');
      expect(env.ANONYMIZED_TELEMETRY).toBe('False');
      expect(env.HF_HUB_DISABLE_TELEMETRY).toBe('1');
    }
  });

  it('is off when the switch is off, whatever shipped', () => {
    expect(kbMode({ ...cfg, knowledgeBase: false }, bundled)).toBe('off');
    expect(kbMode(cfg, bundled)).toBe('bundled');
    expect(kbMode({ ...cfg, gtRagPath: 'C:\\Projects\\gt-rag' }, EXTERNAL_PATHS)).toBe('external');
    expect(kbMode(cfg, EXTERNAL_PATHS)).toBe('off');
  });
});
