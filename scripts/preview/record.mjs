// Animated GIFs of the preview site for the user guide: one Playwright video per scene,
// converted with ffmpeg (two-pass palette). Needs the site built by preview.sh
// (SHOOT=0 bash scripts/preview/preview.sh) and ffmpeg on PATH.
//   node scripts/preview/record.mjs                      # every scene of scripts/preview/guide/gifs.json
//   SCENES=scripts/preview/guide/gifs.json ONLY=wizard,days OUT=docs/guide/gif node scripts/preview/record.mjs
//
// A scene: {name, now, query?, h?, pre?: Action[], actions: Action[]} — `pre` runs before the
// recording "starts" (that part of the video is cut), `actions` is what the GIF shows.
// Action: {click: sel} | {hover: sel} | {type: [sel, text]} | {wheel: dy} | {scroll: sel} |
//         {key: name} | {wait: ms} | {eval: js}. Clicks and hovers glide the fake cursor to the
// element first, so the motion is visible at 10 fps; Playwright's own video shows no cursor.
import { chromium } from 'playwright';
import { readFileSync, mkdirSync, rmSync, statSync, existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { startServer, BASE, CURSOR_SCRIPT, RAW_KEYS } from './server.mjs';

const out = process.env.OUT || 'docs/guide/gif';
const tmp = 'scripts/preview/out/video';
const W = 1380;
const FPS = Number(process.env.FPS || 10);
const GIF_W = Number(process.env.GIF_W || 1100);
const scenes = JSON.parse(readFileSync(process.env.SCENES || 'scripts/preview/guide/gifs.json', 'utf8'));
const only = process.env.ONLY ? process.env.ONLY.split(',') : null;
mkdirSync(out, { recursive: true });
rmSync(tmp, { recursive: true, force: true });
mkdirSync(tmp, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const srv = await startServer();
const browser = await chromium.launch();
let failed = 0;

for (const sc of scenes) {
  if (only && !only.includes(sc.name)) continue;
  const H = sc.h || 1000;
  const ctx = await browser.newContext({
    viewport: { width: W, height: H },
    deviceScaleFactor: 1,
    colorScheme: 'light',
    locale: 'ru-RU',
    timezoneId: 'Europe/Kiev',
    recordVideo: { dir: tmp, size: { width: W, height: H } },
  });
  const page = await ctx.newPage();
  const born = Date.now();
  if (sc.now) await page.clock.install({ time: new Date(sc.now) });
  await page.addInitScript(CURSOR_SCRIPT);
  page.on('pageerror', (e) => console.log('PAGEERROR', sc.name, e.message));
  await page.goto(BASE + (sc.query || ''));
  await page.waitForTimeout(1200);

  let mouse = { x: W / 2, y: H / 2 };
  await page.mouse.move(mouse.x, mouse.y);

  /** Move the cursor in small steps so the path is visible, not a jump. */
  async function glide(x, y, ms = 420) {
    const n = Math.max(8, Math.round(ms / 30));
    const from = { ...mouse };
    for (let i = 1; i <= n; i++) {
      const t = i / n;
      const e = t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t; // ease in-out
      await page.mouse.move(from.x + (x - from.x) * e, from.y + (y - from.y) * e);
      await sleep(ms / n);
    }
    mouse = { x, y };
  }
  async function center(sel) {
    const loc = page.locator(sel).first();
    await loc.scrollIntoViewIfNeeded();
    const b = await loc.boundingBox();
    if (!b) throw new Error(`no box for ${sel}`);
    return { x: b.x + Math.min(b.width / 2, 120), y: b.y + b.height / 2 };
  }
  async function act(a, fast) {
    if (a.click) {
      for (const sel of [].concat(a.click)) {
        const c = await center(sel);
        if (fast) await page.mouse.move(c.x, c.y); else await glide(c.x, c.y);
        mouse = c;
        await page.mouse.down(); await sleep(fast ? 20 : 120); await page.mouse.up();
        await sleep(fast ? 250 : a.pause ?? 800);
      }
    }
    if (a.hover) { const c = await center(a.hover); await glide(c.x, c.y); await sleep(a.pause ?? 900); }
    if (a.type) {
      const c = await center(a.type[0]);
      if (fast) await page.mouse.move(c.x, c.y); else await glide(c.x, c.y);
      mouse = c;
      await page.mouse.click(c.x, c.y);
      await page.keyboard.type(a.type[1], { delay: fast ? 5 : 45 });
      await sleep(fast ? 100 : 500);
    }
    if (a.wheel) {
      const n = fast ? 1 : 6;
      for (let i = 0; i < n; i++) { await page.mouse.wheel(0, a.wheel / n); await sleep(fast ? 50 : 70); }
      await sleep(fast ? 150 : 500);
    }
    if (a.scroll) { await page.locator(a.scroll).first().evaluate((el) => el.scrollIntoView({ block: 'start', behavior: 'instant' })); await sleep(300); }
    if (a.key) { await page.keyboard.press(a.key); await sleep(fast ? 100 : 600); }
    if (a.eval) { await page.evaluate(a.eval); await sleep(100); }
    if (a.wait) await sleep(a.wait);
  }

  for (const a of sc.pre || []) await act(a, true);
  await sleep(400);
  const start = (Date.now() - born) / 1000;
  for (const a of sc.actions) await act(a, false);
  await sleep(sc.tail ?? 900);
  const raw = await page.evaluate(RAW_KEYS);
  if (raw.length) { failed++; console.log('RAW-KEYS', sc.name, raw.join(' ')); }
  const end = (Date.now() - born) / 1000;

  const video = page.video();
  await ctx.close();
  const webm = await video.path();
  const gif = path.join(out, `${sc.name}.gif`);
  // No dithering on purpose: the shell's flat panels compress to nothing without it, and the
  // soft gradients band only slightly. The ladder steps down until the file fits 3 MB.
  const ladder = sc.ladder || [
    { fps: FPS, w: GIF_W, colors: 128 },
    { fps: FPS, w: GIF_W, colors: 96 },
    { fps: FPS, w: 1000, colors: 96 },
    { fps: 8, w: 1000, colors: 96 },
  ];
  let kb = Infinity;
  for (const L of ladder) {
    const vf = `fps=${L.fps},scale=${L.w}:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=${L.colors}:stats_mode=diff[p];[s1][p]paletteuse=dither=none`;
    const args = ['-y', '-loglevel', 'error', '-ss', start.toFixed(2), '-to', end.toFixed(2), '-i', webm, '-vf', vf, '-loop', '0', gif];
    const r = spawnSync('ffmpeg', args, { stdio: 'inherit' });
    if (r.status !== 0) { failed++; console.log('FFMPEG failed', sc.name); break; }
    kb = Math.round(statSync(gif).size / 1024);
    console.log(`gif ${sc.name}: ${(end - start).toFixed(1)} s, ${kb} KB (fps ${L.fps}, ${L.w} px, ${L.colors} colours)`);
    if (kb <= 3000) break;
  }
  if (kb > 3000) { failed++; console.log('TOO BIG', sc.name); }
  if (end - start > 10.5) { failed++; console.log('TOO LONG', sc.name); }
}
await browser.close();
srv.close();
if (failed) { console.log(`${failed} problem(s)`); process.exit(1); }
