// Bundles chat-fold.ts with the Electron-bound main modules replaced by the
// stubs next to this file (see chat-fold.ts for what is real and what is not).
//   node build.mjs <out.cjs>
import { build } from 'esbuild';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../../..');
const stub = (f) => path.join(here, 'stubs', f);
// Specifier (as written in src/main/*) → stub. Anything else resolves normally.
const MAP = {
  electron: stub('electron.ts'),
  './store': stub('store.ts'),
  '../store': stub('store.ts'),
  './runtime': stub('runtime.ts'),
  '../runtime': stub('runtime.ts'),
  '../session': stub('misc.ts'),
  './tasks': stub('misc.ts'),
  './resources': stub('misc.ts'),
  './workflows': stub('misc.ts'),
  './prompt': stub('misc.ts'),
  './mcp': stub('misc.ts'),
  './drivers': stub('drivers.ts'),
};
await build({
  entryPoints: [path.join(here, 'chat-fold.ts')],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  outfile: process.argv[2],
  logLevel: 'warning',
  alias: { '@shared': path.join(root, 'src/shared'), '@renderer': path.join(root, 'src/renderer') },
  external: ['@anthropic-ai/claude-agent-sdk'],
  plugins: [{
    name: 'stub-main',
    setup(b) {
      b.onResolve({ filter: /^(electron|\.\.?\/(store|runtime|session|tasks|resources|workflows|prompt|mcp|drivers))$/ }, (a) => {
        const inMain = a.importer.includes(`${path.sep}src${path.sep}main${path.sep}`);
        if (a.path === 'electron' || inMain) return { path: MAP[a.path] };
        return undefined;
      });
    },
  }],
});
