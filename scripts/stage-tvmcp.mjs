// Stages everything the installer must carry so that tvmcp needs nothing from
// the user's machine: the tvmcp sources, its lock file, and a `uv` binary.
//
// Run by `npm run dist` before electron-builder. It is deliberately picky and
// fails loudly, because every failure it catches here would otherwise become a
// broken install on someone else's computer:
//
//   * the lock file must agree with pyproject.toml. `uv sync --frozen` is what
//     the app runs on first start, and --frozen refuses to resolve anything: a
//     drift shipped in the installer is a first run that cannot succeed;
//   * `uv` must be found. We copy the one on this machine rather than
//     downloading a pinned build: a download needs a URL and a checksum that
//     someone has to keep true, and a wrong one would be discovered by a user,
//     not by us. `uv --version` is recorded in the payload for support.
//
// What is NOT staged: .venv, .git, tests, docs, dist and the caches. The
// environment is built on the user's machine (a Windows venv is not
// relocatable — Scripts/python.exe is a stub that reads `home=` out of
// pyvenv.cfg and points at the interpreter that made it).
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'build', 'tvmcp-payload');
const uvOut = path.join(root, 'build', process.platform === 'win32' ? 'uv.exe' : 'uv');

/** Everything tvmcp reads at runtime, plus what `uv sync` needs to install it. */
const ITEMS = [
  'pyproject.toml',
  'uv.lock',
  // pyproject's `readme = "README.md"`; the build backend fails without it.
  'README.md',
  'LICENSE',
  'src',
  // doctor.py reaches for scripts/start-tv-desktop.ps1 two directories up.
  'scripts',
];

const SKIP = new Set(['__pycache__', '.pytest_cache', '.mypy_cache']);

function die(message) {
  console.error(`[stage-tvmcp] ${message}`);
  process.exit(1);
}

function source() {
  const dir = (process.env.TVMCP_SRC || '').trim() || path.resolve(root, '..', 'tradingview-mcp');
  if (!existsSync(path.join(dir, 'pyproject.toml'))) {
    die(`no tvmcp checkout at ${dir}. Set TVMCP_SRC to one.`);
  }
  const name = /^\s*name\s*=\s*["']([^"']+)["']/m.exec(readFileSync(path.join(dir, 'pyproject.toml'), 'utf8'))?.[1];
  if (name !== 'tvmcp') die(`${dir} holds the project "${name ?? '?'}", not tvmcp.`);
  return dir;
}

function uvBinary() {
  const probe = process.platform === 'win32' ? ['where', 'uv'] : ['which', 'uv'];
  try {
    const found = execFileSync(probe[0], [probe[1]], { encoding: 'utf8' }).split(/\r?\n/).map((l) => l.trim()).filter(Boolean)[0];
    if (found && existsSync(found)) return found;
  } catch {
    /* fall through to the message below */
  }
  die('uv was not found on PATH, and the installer has to carry one. Install uv (https://astral.sh/uv) and run again.');
}

function checkLock(dir, uv) {
  try {
    execFileSync(uv, ['lock', '--check', '--project', dir], { stdio: 'pipe' });
  } catch (e) {
    const detail = String(e?.stderr ?? e?.message ?? e).trim().split('\n').slice(-3).join('\n');
    die(`uv.lock does not match pyproject.toml in ${dir}. Run "uv lock" there and commit the result.\n${detail}`);
  }
}

const src = source();
const uv = uvBinary();
checkLock(src, uv);
const uvVersion = execFileSync(uv, ['--version'], { encoding: 'utf8' }).trim();

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const item of ITEMS) {
  const from = path.join(src, item);
  if (!existsSync(from)) {
    if (item === 'LICENSE') continue;
    die(`${item} is missing from ${src}`);
  }
  cpSync(from, path.join(out, item), {
    recursive: true,
    filter: (p) => !p.split(/[\\/]/).some((part) => SKIP.has(part)),
  });
}

cpSync(uv, uvOut);
writeFileSync(
  path.join(out, 'STAGED.json'),
  `${JSON.stringify({ source: src, uv: uvVersion, at: new Date().toISOString() }, null, 2)}\n`,
  'utf8',
);

console.log(`[stage-tvmcp] payload -> ${out}`);
console.log(`[stage-tvmcp] ${uvVersion} -> ${uvOut}`);
