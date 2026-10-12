// The environment a macro session (and a scheduled run) gets: the six
// variables of the vault contract, the venv in front of PATH, and the
// interpreter resolved per runtime mode. Pure, so the exact shape is pinned.
import { describe, expect, it } from 'vitest';
import { DEFAULT_SETTINGS } from '@shared/defaults';
import { looksLikeSessionId, macroEnv, macroEnvVars, macroPython, SESSION_CHECK_CODE } from './macro-env';
import { EXTERNAL_PATHS, type RuntimePaths } from './runtime-spec';

const bundled: RuntimePaths = { ...EXTERNAL_PATHS, payloadDir: 'C:\\app\\resources\\tvmcp', uvBin: 'C:\\app\\resources\\uv.exe', venvDir: 'C:\\userData\\py\\venv' };

describe('macroEnvVars', () => {
  it('spells the whole contract, and leaves out what is not resolvable', () => {
    const full = macroEnvVars({ vault: 'C:\\Users\\T\\Documents\\Strategy Desk', python: 'C:\\userData\\py\\venv\\Scripts\\python.exe', claude: 'C:\\app\\claude.exe', sessionId: 'abc', cdpPort: 9223 });
    expect(full).toEqual({
      MACRO_VAULT: 'C:\\Users\\T\\Documents\\Strategy Desk',
      MACRO_PYTHON: 'C:\\userData\\py\\venv\\Scripts\\python.exe',
      MACRO_CLAUDE: 'C:\\app\\claude.exe',
      TV_SESSIONID: 'abc',
      TV_CDP_URL: 'http://127.0.0.1:9223',
      PYTHONIOENCODING: 'utf-8',
    });
    const bare = macroEnvVars({ vault: 'C:\\v', python: '', claude: '', sessionId: null, cdpPort: 0 });
    expect(Object.keys(bare).sort()).toEqual(['MACRO_VAULT', 'PYTHONIOENCODING', 'TV_CDP_URL']);
    expect(bare.TV_CDP_URL).toBe('http://127.0.0.1:9223');
  });
});

describe('macroEnv', () => {
  it('prepends the venv Scripts directory to the PATH spelling the base already has', () => {
    const env = macroEnv({ vault: 'C:\\v', python: 'C:\\py\\venv\\Scripts\\python.exe', claude: '', sessionId: 's', cdpPort: 9223, platform: 'win32' }, { Path: 'C:\\Windows', HOME: 'x' });
    expect(env.Path).toBe('C:\\py\\venv\\Scripts;C:\\Windows');
    expect(env.PATH).toBeUndefined();
    expect(env.HOME).toBe('x');
    expect(env.TV_SESSIONID).toBe('s');
  });

  it('uses a colon on POSIX and creates the variable when the base has none', () => {
    const env = macroEnv({ vault: '/v', python: '/v/.venv/bin/python', claude: '', sessionId: null, cdpPort: 9223, platform: 'linux' }, { PATH: '/usr/bin' });
    expect(env.PATH).toBe('/v/.venv/bin:/usr/bin');
    const none = macroEnv({ vault: '/v', python: '/v/.venv/bin/python', claude: '', sessionId: null, cdpPort: 9223, platform: 'linux' }, {});
    expect(none.PATH).toBe('/v/.venv/bin');
  });

  it('never carries a cookie it was not given', () => {
    const env = macroEnv({ vault: '/v', python: '', claude: '', sessionId: null, cdpPort: 9223, platform: 'linux' }, { TV_SESSIONID: 'inherited' });
    // Inheritance from the process environment is the existing behaviour
    // (runtime-spec.ts): the contract only ADDS the stored one.
    expect(env.TV_SESSIONID).toBe('inherited');
  });
});

describe('macroPython', () => {
  it('is the provisioned venv in bundled mode and the checkout venv in a dev run', () => {
    expect(macroPython(DEFAULT_SETTINGS, bundled, 'win32')).toBe('C:\\userData\\py\\venv\\Scripts\\python.exe');
    expect(macroPython({ ...DEFAULT_SETTINGS, tvmcpPath: 'C:\\tvmcp' }, EXTERNAL_PATHS, 'win32')).toBe('C:\\tvmcp\\.venv\\Scripts\\python.exe');
    expect(macroPython({ ...DEFAULT_SETTINGS, tvmcpPath: '/tvmcp' }, EXTERNAL_PATHS, 'linux')).toBe('/tvmcp/.venv/bin/python');
    expect(macroPython({ ...DEFAULT_SETTINGS, tvmcpPath: '' }, EXTERNAL_PATHS, 'win32')).toBe('');
  });
});

describe('the session check', () => {
  it('reads the cookie from the environment, never from the command line', () => {
    expect(SESSION_CHECK_CODE).toContain('os.environ["TV_SESSIONID"]');
    expect(SESSION_CHECK_CODE).toContain('fetch_auth_token');
  });

  it('knows the cookie shape', () => {
    expect(looksLikeSessionId('a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4')).toBe(true);
    expect(looksLikeSessionId(' short ')).toBe(false);
    expect(looksLikeSessionId('has spaces in it and is long enough')).toBe(false);
  });
});
