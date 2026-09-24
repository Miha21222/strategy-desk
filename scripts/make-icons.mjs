// Regenerate the original bull + bear + chart icon with npm run icons.
// Edit build/icon-src/icon.svg (64px+) or icon-small.svg (16/24/32/48px).
// Electron draws each SVG into an exact-size canvas in one hidden offscreen
// window; render-icons.cjs packs those PNGs into the existing ICO/ICNS files.
// No new dependencies, app launch, or visible window. The old pixel painter
// and hand-written PNG encoder are replaced by Chromium's SVG rasterizer.
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const env = { ...process.env };
// Some Electron-hosted terminals export this; the renderer needs Electron mode.
delete env.ELECTRON_RUN_AS_NODE;
const child = spawnSync(require('electron'), [join(dirname(fileURLToPath(import.meta.url)), 'render-icons.cjs')], {
  env, stdio: 'inherit', windowsHide: true, timeout: 60000,
});
if (child.error) console.error(`Could not run the offscreen icon renderer: ${child.error.message}`);
process.exitCode = child.status ?? 1;
