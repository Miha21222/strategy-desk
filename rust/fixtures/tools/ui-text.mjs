// Replays a steps file the way scripts/preview/shoot.mjs does and records, per
// step, what is ON SCREEN as text: the visible innerText of the window tiles,
// the agent strip and the main area, plus which cards are present. This is the
// objective half of the reference screenshots — UI_CHECKLIST.md compares egui
// against these strings, not against pixels.
//
//   NOW=… STEPS=… OUT=<file.json> [DARK=1] node rust/fixtures/tools/ui-text.mjs
//
// Must run from the repo root after scripts/preview/preview.sh (SHOOT=0) built
// scripts/preview/out/site. Step semantics are copied from shoot.mjs (click,
// fill, type, scroll, hover, eval, wait); screenshots are not taken here.
import { chromium } from 'playwright';
import { readFileSync, writeFileSync } from 'node:fs';
import { startServer, BASE, RAW_KEYS } from '../../../scripts/preview/server.mjs';

const TEXT = `(() => {
  const vis = (el) => !!el && el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden';
  const txt = (sel) => [...document.querySelectorAll(sel)].filter(vis).map((e) => e.innerText.replace(/\\s+\\n/g, '\\n').trim()).filter(Boolean);
  const heads = txt('.main .card-head');
  return {
    tab: txt('.tabs button.on')[0] ?? null,
    day: txt('.macro-day')[0] ?? null,
    tiles: txt('.macro-windows .wtile'),
    activeTile: txt('.macro-windows .wtile.active')[0] ?? null,
    agentStrip: txt('.agent-strip')[0] ?? null,
    cards: heads,
    buttons: [...new Set(txt('.main button'))],
    main: txt('.macro-main')[0] ?? txt('.main')[0] ?? null,
  };
})()`;

const srv = await startServer();
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1380, height: Number(process.env.H || 900) }, deviceScaleFactor: 1, colorScheme: process.env.DARK ? 'dark' : 'light', locale: 'ru-RU', timezoneId: 'Europe/Kiev' });
const page = await ctx.newPage();
if (process.env.NOW) await page.clock.install({ time: new Date(process.env.NOW) });
await page.goto(BASE + (process.env.QUERY || ''));
await page.waitForTimeout(1200);
const steps = JSON.parse(readFileSync(process.env.STEPS, 'utf8'));
const out = [];
for (const s of steps) {
  if (s.click) for (const sel of [].concat(s.click)) { await page.locator(sel).first().click(); await page.waitForTimeout(400); }
  if (s.fill) await page.locator(s.fill[0]).fill(s.fill[1]);
  if (s.type) { await page.locator(s.type[0]).first().click(); await page.keyboard.type(s.type[1], { delay: 10 }); }
  if (s.scroll) await page.locator(s.scroll).first().evaluate((el) => el.scrollIntoView({ block: 'start' }));
  if (s.hover) await page.locator(s.hover).first().hover();
  if (s.eval) await page.evaluate(s.eval);
  await page.waitForTimeout(s.wait ?? 500);
  out.push({ step: s.name, does: s, ...(await page.evaluate(TEXT)), rawKeys: await page.evaluate(RAW_KEYS) });
}
await browser.close();
srv.close();
writeFileSync(process.env.OUT, JSON.stringify(out, null, 1) + '\n');
console.log('text', process.env.OUT, out.length, 'steps');
