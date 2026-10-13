// Screenshots of the preview site, one per step of a steps file.
//   NOW=2026-09-15T06:40:00Z H=1000 node scripts/preview/shoot.mjs
//   STEPS=<file> OUT=<dir> QUERY='?wizard=1' DARK=1
// A step: {name, click?: sel|sel[], hover?: sel, fill?: [sel, text], type?: [sel, text], scroll?: sel,
//          eval?: js, wait?: ms, full?: bool, fit?: bool}
// `full` is Playwright's fullPage; `fit` resizes the viewport to the scrolling container's content
// height first, which is what actually shows the whole desk (the app shell never grows).
import { chromium } from 'playwright';
import { readFileSync, mkdirSync } from 'node:fs';
import { startServer, BASE, FIT_HEIGHT, RAW_KEYS } from './server.mjs';

const srv = await startServer();
const out = process.env.OUT || 'scripts/preview/out/shots'; mkdirSync(out, { recursive: true });
const W = 1380;
const H = Number(process.env.H || 900);
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 1, colorScheme: process.env.DARK ? 'dark' : 'light', locale: 'ru-RU', timezoneId: 'Europe/Kiev' });
const page = await ctx.newPage();
if (process.env.NOW) { await page.clock.install({ time: new Date(process.env.NOW) }); }
page.on('pageerror', (e) => console.log('PAGEERROR', e.message));
page.on('console', (m) => { if (m.type() === 'error') console.log('CONSOLE', m.text()); });
await page.goto(BASE + (process.env.QUERY || ''));
await page.waitForTimeout(1200);
const steps = JSON.parse(readFileSync(process.env.STEPS || 'scripts/preview/steps.json', 'utf8'));
let failed = 0;
for (const s of steps) {
  if (s.click) { for (const sel of [].concat(s.click)) { await page.locator(sel).first().click(); await page.waitForTimeout(400); } }
  if (s.fill) { await page.locator(s.fill[0]).fill(s.fill[1]); }
  if (s.type) { await page.locator(s.type[0]).first().click(); await page.keyboard.type(s.type[1], { delay: 10 }); }
  if (s.scroll) { await page.locator(s.scroll).first().evaluate((el) => el.scrollIntoView({ block: 'start' })); }
  if (s.hover) { await page.locator(s.hover).first().hover(); }
  if (s.eval) await page.evaluate(s.eval);
  await page.waitForTimeout(s.wait ?? 500);
  if (s.fit) {
    const h = await page.evaluate(FIT_HEIGHT);
    if (h > H) { await page.setViewportSize({ width: W, height: Math.min(Math.ceil(h), 6000) }); await page.waitForTimeout(350); }
  }
  await page.screenshot({ path: `${out}/${s.name}.png`, fullPage: Boolean(s.full) });
  if (s.fit) { await page.setViewportSize({ width: W, height: H }); await page.waitForTimeout(200); }
  const raw = await page.evaluate(RAW_KEYS);
  if (raw.length) { failed++; console.log('RAW-KEYS', s.name, raw.join(' ')); }
  console.log('shot', s.name);
}
await browser.close(); srv.close();
if (failed) { console.log(`${failed} step(s) show raw i18n keys`); process.exit(1); }
