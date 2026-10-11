import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync, mkdirSync } from 'node:fs';
import path from 'node:path';
const root = path.resolve('scripts/preview/out/site');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.png': 'image/png' };
const srv = createServer((req, res) => {
  let p = path.join(root, decodeURIComponent(req.url.split('?')[0]));
  if (existsSync(p) && statSync(p).isDirectory()) p = path.join(p, 'index.html');
  if (!existsSync(p)) { res.writeHead(404); res.end(); return; }
  res.writeHead(200, { 'content-type': types[path.extname(p)] || 'application/octet-stream' });
  res.end(readFileSync(p));
});
await new Promise((r) => srv.listen(4173, r));
const out = 'scripts/preview/out/shots'; mkdirSync(out, { recursive: true });
const browser = await chromium.launch();
const ctx = await browser.newContext({ viewport: { width: 1380, height: Number(process.env.H || 900) }, deviceScaleFactor: 1, colorScheme: process.env.DARK ? 'dark' : 'light' });
const page = await ctx.newPage();
if (process.env.NOW) { await page.clock.install({ time: new Date(process.env.NOW) }); }
page.on('pageerror', (e) => console.log('PAGEERROR', e.message));
page.on('console', (m) => { if (m.type() === 'error') console.log('CONSOLE', m.text()); });
await page.goto('http://localhost:4173/');
await page.waitForTimeout(1200);
const steps = JSON.parse(readFileSync(process.env.STEPS || 'scripts/preview/steps.json', 'utf8'));
for (const s of steps) {
  if (s.click) { for (const sel of [].concat(s.click)) { await page.locator(sel).first().click(); await page.waitForTimeout(400); } }
  if (s.fill) { await page.locator(s.fill[0]).fill(s.fill[1]); }
  if (s.eval) await page.evaluate(s.eval);
  await page.waitForTimeout(s.wait ?? 500);
  await page.screenshot({ path: `${out}/${s.name}.png`, fullPage: Boolean(s.full) });
  console.log('shot', s.name);
}
await browser.close(); srv.close();
