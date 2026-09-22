// Stages the course knowledge base (gt-rag) so the installed app can answer
// from the course with nothing installed on the user's side.
//
// Run by `npm run dist` before electron-builder, next to `stage-tvmcp.mjs`.
// The two payloads are deliberately different shapes: tvmcp ships SOURCES that
// a `uv sync` turns into an environment, while gt-rag ships sources AND data,
// and its dependencies ride in tvmcp's optional `kb` extra — one venv serves
// both servers (see src/main/runtime-spec.ts).
//
// What the payload looks like, and why it is split that way:
//
//   gt-rag/server.py, gt_rag/, manifest.json, style-overrides.json   the code
//   gt-rag/raw/       course markdown            read-only, stays in the install dir
//   gt-rag/images/    chart screenshots          read-only, stays in the install dir
//   gt-rag/store/     ChromaDB + index_state     COPIED into userData on first run
//   gt-rag/model/     fastembed ONNX cache       COPIED into userData (optional)
//
// Only the last two are copied per user, because ChromaDB opens its sqlite file
// read-write even to answer a query and huggingface_hub writes lock files into
// the model cache — neither may happen inside an installation directory.
//
// Switches:
//   SD_BUNDLE_KB=0         ship no knowledge base at all (the app then has no
//                          `gt-database` server unless the user names a clone).
//   SD_BUNDLE_KB_MODEL=0   ship the content but not the ~240 MB model; the app
//                          offers a one-press download instead.
//   GT_RAG_SRC=<dir>       where the gt-rag checkout is.
//   FASTEMBED_CACHE=<dir>  where the model cache to stage is.
//
// NOTE ON CONTENT: what lands in `raw/` and `images/` is whatever the checkout
// holds. gt-rag's own README says the course texts are copyrighted material and
// are not redistributable; staging them into a public installer is a decision
// about YOUR rights to that content, not something this script can check.
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, 'build', 'gtrag-payload');

/** The code half: everything the server imports or reads by an absolute path. */
const CODE = ['server.py', 'gt_rag', 'manifest.json', 'style-overrides.json', 'LICENSE'];
const SKIP = new Set(['__pycache__', '.pytest_cache', '.mypy_cache', '.git', '.venv', 'tmp']);

function die(message) {
  console.error(`[stage-gtrag] ${message}`);
  process.exit(1);
}

function off(value) {
  return ['0', 'false', 'no', 'off'].includes(String(value ?? '').trim().toLowerCase());
}

function source() {
  const dir = (process.env.GT_RAG_SRC || '').trim() || path.resolve(root, '..', '..', 'Documents', 'Obsidian Notes', 'Trading', 'Trading Strategy Assistant', 'gt-rag');
  if (!existsSync(path.join(dir, 'server.py')) || !existsSync(path.join(dir, 'gt_rag'))) {
    die(`no gt-rag checkout at ${dir}. Set GT_RAG_SRC to one, or SD_BUNDLE_KB=0 to ship without the knowledge base.`);
  }
  return dir;
}

/** The fastembed cache to stage, or null when there is nothing to stage. */
function modelCache() {
  if (off(process.env.SD_BUNDLE_KB_MODEL)) return null;
  const dir = (process.env.FASTEMBED_CACHE || '').trim() || path.join(os.tmpdir(), 'fastembed_cache');
  if (!existsSync(dir)) return null;
  // fastembed stores models in huggingface_hub's layout. A cache with no model
  // in it is not worth shipping, and shipping the wrong one is worse than
  // shipping none: the app falls back to a download it can explain.
  const models = readdirSync(dir).filter((n) => n.startsWith('models--') && n.includes('MiniLM'));
  return models.length ? dir : null;
}

function copyTree(from, to) {
  cpSync(from, to, { recursive: true, filter: (p) => !p.split(/[\\/]/).some((part) => SKIP.has(part)) });
}

function mbOf(dir) {
  let total = 0;
  const walk = (d) => {
    for (const entry of readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, entry.name);
      if (entry.isDirectory()) walk(p);
      else total += statSync(p).size;
    }
  };
  if (existsSync(dir)) walk(dir);
  return Math.round(total / 1048576);
}

if (off(process.env.SD_BUNDLE_KB)) {
  // The directory still has to exist: `extraResources` names it and a missing
  // source fails the build. An empty one is exactly what the app reads as "no
  // knowledge base shipped" — there is no server.py in it to find.
  rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  writeFileSync(path.join(out, 'STAGED.json'), `${JSON.stringify({ bundled: false, at: new Date().toISOString() }, null, 2)}\n`, 'utf8');
  console.log('[stage-gtrag] SD_BUNDLE_KB=0 — shipping without the knowledge base');
  process.exit(0);
}

const src = source();
const data = path.join(src, 'data');
const raw = path.join(data, 'raw');
const chroma = path.join(data, 'chroma');
const state = path.join(data, 'index_state.json');

// Each of these would otherwise ship as a knowledge base that fails on someone
// else's machine in a way they cannot repair: no content, no index, or an index
// with no state file (gt_health then calls every page stale).
if (!existsSync(raw) || !readdirSync(raw).some((n) => n.endsWith('.md'))) die(`${raw} holds no course pages`);
if (!existsSync(path.join(chroma, 'chroma.sqlite3'))) die(`${chroma} is not a ChromaDB store — run "python ingest.py" in ${src} first`);
if (!existsSync(state)) die(`${state} is missing — the app could not tell a fresh index from a stale one`);

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

for (const item of CODE) {
  const from = path.join(src, item);
  if (!existsSync(from)) {
    if (item === 'LICENSE' || item === 'style-overrides.json') continue;
    die(`${item} is missing from ${src}`);
  }
  copyTree(from, path.join(out, item));
}

copyTree(raw, path.join(out, 'raw'));
const images = path.join(data, 'images');
if (existsSync(images)) copyTree(images, path.join(out, 'images'));
mkdirSync(path.join(out, 'store'), { recursive: true });
copyTree(chroma, path.join(out, 'store', 'chroma'));
cpSync(state, path.join(out, 'store', 'index_state.json'));

const model = modelCache();
if (model) copyTree(model, path.join(out, 'model'));

const pages = readdirSync(path.join(out, 'raw')).filter((n) => n.endsWith('.md')).length;
writeFileSync(
  path.join(out, 'STAGED.json'),
  `${JSON.stringify({ source: src, pages, model: model ?? null, at: new Date().toISOString() }, null, 2)}\n`,
  'utf8',
);

console.log(`[stage-gtrag] payload -> ${out}`);
console.log(`[stage-gtrag] ${pages} pages, store ${mbOf(path.join(out, 'store'))} MB, images ${mbOf(path.join(out, 'images'))} MB, model ${model ? `${mbOf(path.join(out, 'model'))} MB` : 'NOT staged (the app will offer a download)'}`);
