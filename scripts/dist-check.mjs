// Launches the PACKAGED binary headlessly (DESK_SMOKE=pack) and fails loudly
// if it does not start, or starts without the files it is supposed to ship
// with. This is the only check that can catch a missing `files:` entry in
// electron-builder.yml: in dev every resource happens to sit in the working
// directory, so the app finds it either way.
//
//   npm run dist:check                 # release/win-unpacked
//   npm run dist:check -- "<path>"     # some other install, e.g. %LOCALAPPDATA%
//
// Pass --rename-project to move workflows/ and data/ out of the way for the
// duration of the run (and back afterwards, even on failure): that is the real
// acceptance test, because an app that reads them out of the project directory
// passes without it.
import { spawn } from 'node:child_process';
import { existsSync, renameSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const renameProject = args.includes('--rename-project');
const target = args.find((a) => !a.startsWith('--'));

const EXE = {
  win32: join('win-unpacked', 'Strategy Desk.exe'),
  darwin: join('mac', 'Strategy Desk.app', 'Contents', 'MacOS', 'Strategy Desk'),
  linux: join('linux-unpacked', 'strategy-desk'),
}[process.platform];

const binary = target ? resolve(target) : join(ROOT, 'release', EXE ?? '');
if (!existsSync(binary)) {
  console.error(`dist:check: no packaged binary at ${binary}\n  run \`npm run dist\` (or \`npm run dist:dir\`) first`);
  process.exit(1);
}

// The files that must NOT be what the packaged app reads.
const MOVED = ['workflows', 'data'];
const moved = [];
function hideProject() {
  if (!renameProject) return;
  for (const name of MOVED) {
    const from = join(ROOT, name);
    const to = join(ROOT, `${name}.dist-check-hidden`);
    if (!existsSync(from)) continue;
    renameSync(from, to);
    moved.push([to, from]);
  }
  console.log(`dist:check: moved ${moved.map(([, f]) => f).join(', ')} out of the way`);
}
function restoreProject() {
  while (moved.length) {
    const [to, from] = moved.pop();
    try {
      renameSync(to, from);
    } catch (e) {
      console.error(`dist:check: COULD NOT RESTORE ${from} — it is at ${to} (${e.message})`);
    }
  }
}

// The lines a packaged build must print. The macro-review MVP's own resources
// are asserted by name, so a build that lost the appendices or the vault
// template fails here even if `[pack] ok` were ever printed by mistake.
const REQUIRED = ['[pack] ok', '[main] renderer loaded', '[pack] prompt/macro-review.md', '[pack] prompt/ask.md', '[pack] vault-template files'];
const TIMEOUT_MS = 90_000;

function run() {
  return new Promise((done) => {
    const child = spawn(binary, [], {
      // cwd on purpose NOT the project: a packaged app must not depend on it.
      cwd: process.platform === 'win32' ? process.env.SystemRoot ?? ROOT : '/',
      env: { ...process.env, DESK_SMOKE: 'pack', ELECTRON_ENABLE_LOGGING: '1' },
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });
    let out = '';
    const kill = setTimeout(() => {
      console.error(`dist:check: the app did not finish within ${TIMEOUT_MS / 1000} s`);
      child.kill();
    }, TIMEOUT_MS);
    child.stdout.on('data', (d) => {
      out += d;
      process.stdout.write(d);
    });
    child.stderr.on('data', (d) => {
      out += d;
      process.stderr.write(d);
    });
    child.on('error', (e) => {
      clearTimeout(kill);
      console.error(`dist:check: could not launch ${binary}: ${e.message}`);
      done(1);
    });
    child.on('close', (code) => {
      clearTimeout(kill);
      const missing = REQUIRED.filter((line) => !out.includes(line));
      if (out.includes('[pack] FAILED') || out.includes('[pack] vault-template NOT FOUND') || out.includes('[pack] vault-template files MISSING')) {
        console.error('dist:check: FAILED — the packaged app could not find what it ships with (see the [pack] lines above)');
        done(1);
      } else if (missing.length) {
        console.error(`dist:check: FAILED — the packaged app never printed: ${missing.join(' | ')}`);
        done(1);
      } else if (code !== 0) {
        console.error(`dist:check: FAILED — the app exited with code ${code}`);
        done(1);
      } else {
        console.log(`dist:check: ok — ${binary}`);
        done(0);
      }
    });
  });
}

try {
  hideProject();
  process.exitCode = await run();
} finally {
  restoreProject();
}
