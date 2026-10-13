// The static server behind shoot.mjs and record.mjs: serves scripts/preview/out/site
// (out/renderer + the desk-mock + the dumped demo data) on :4173.
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import path from 'node:path';

const root = path.resolve('scripts/preview/out/site');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };

export const PORT = 4173;
export const BASE = `http://localhost:${PORT}/`;

export async function startServer() {
  const srv = createServer((req, res) => {
    let p = path.join(root, decodeURIComponent(req.url.split('?')[0]));
    if (existsSync(p) && statSync(p).isDirectory()) p = path.join(p, 'index.html');
    if (!existsSync(p)) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': types[path.extname(p)] || 'application/octet-stream' });
    res.end(readFileSync(p));
  });
  await new Promise((r) => srv.listen(PORT, r));
  return srv;
}

/**
 * The scroll container that actually holds the page: `.app` is 100% high and
 * `.main` hides its overflow, so a Playwright `fullPage` shot is never taller
 * than the viewport. The height this returns is what the viewport must be for
 * the whole content to show.
 */
export const FIT_HEIGHT = `(() => {
  let best = 0;
  for (const el of document.querySelectorAll('.main *')) {
    const cs = getComputedStyle(el);
    if (!/auto|scroll/.test(cs.overflowY)) continue;
    if (el.scrollHeight <= el.clientHeight) continue;
    const h = el.scrollHeight + el.getBoundingClientRect().top + 24;
    if (h > best) best = h;
  }
  return best;
})()`;

/** Raw i18n keys on screen are a failure, not a cosmetic: this finds them. */
export const RAW_KEYS = `(() => (document.body.innerText.match(/\\b(preflight|setup|macro|settings|help|tabs|sidebar|chat)\\.[a-zA-Z][\\w.]+/g) || []).slice(0, 10))()`;

/** A visible cursor for the recordings: Playwright's video shows none of its own. */
export const CURSOR_SCRIPT = `(() => {
  const make = () => {
    if (document.getElementById('__cursor')) return;
    const c = document.createElement('div');
    c.id = '__cursor';
    c.style.cssText = 'position:fixed;left:0;top:0;width:20px;height:20px;margin:-3px 0 0 -3px;border-radius:50%;background:rgba(25,25,45,.85);border:2px solid #fff;box-shadow:0 1px 5px rgba(0,0,0,.6);pointer-events:none;z-index:2147483647;transform:translate(-9999px,-9999px);transition:transform .05s linear, width .12s, height .12s, margin .12s';
    document.body.appendChild(c);
    let x = -9999, y = -9999;
    const place = () => { c.style.transform = 'translate(' + x + 'px,' + y + 'px)'; };
    window.addEventListener('mousemove', (e) => { x = e.clientX; y = e.clientY; place(); }, true);
    window.addEventListener('mousedown', () => { c.style.width = '26px'; c.style.height = '26px'; c.style.margin = '-6px 0 0 -6px'; c.style.background = 'rgba(99,91,255,.75)'; }, true);
    window.addEventListener('mouseup', () => { setTimeout(() => { c.style.width = '20px'; c.style.height = '20px'; c.style.margin = '-3px 0 0 -3px'; c.style.background = 'rgba(25,25,45,.85)'; }, 180); }, true);
  };
  if (document.body) make(); else document.addEventListener('DOMContentLoaded', make);
})()`;
