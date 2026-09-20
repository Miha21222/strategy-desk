// Draws the Strategy Desk app icon and writes build/icon.png, build/icon.ico
// and build/icon.icns. No image library and no native tooling: the mark is
// plain geometry, PNG is zlib + four chunks, and .ico / .icns are containers of
// PNGs. Run it with `npm run icons` after changing anything below.
//
// The mark: a session range (two grey rails) with one bright candle whose body
// has closed above the upper rail — the moment the «День» panel exists to
// catch. Deliberately nothing like TradingView's chart-arrow or Anthropic's
// burst: a dark tile, two horizontals, one vertical.
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = join(dirname(fileURLToPath(import.meta.url)), '..', 'build');

// ---------- the drawing, in unit coordinates (0..1) ----------

const BG = [0x14, 0x16, 0x1a, 0xff]; // app window background
const RAIL = [0x8b, 0x91, 0x9c, 0xff]; // --fg-muted: the range high/low
const CANDLE = [0x4c, 0x8d, 0xff, 0xff]; // accent blue: the bar that broke it

const CORNER = 0.22; // rounded-square, Windows/macOS tile look
const RAIL_X = [0.13, 0.87];
const RAIL_H = 0.06;
const RAIL_TOP_Y = 0.42;
const RAIL_BOTTOM_Y = 0.72;
const BODY_X = [0.375, 0.625];
const BODY_Y = [0.155, 0.385];
const WICK_X = [0.468, 0.532];
const WICK_Y = [0.09, 0.87];

/** Coverage of the rounded-square tile at unit point (x, y); 0 or 1 (AA comes from supersampling). */
function inTile(x, y) {
  const r = CORNER;
  const cx = Math.min(Math.max(x, r), 1 - r);
  const cy = Math.min(Math.max(y, r), 1 - r);
  const dx = x - cx;
  const dy = y - cy;
  return dx * dx + dy * dy <= r * r;
}

const inBox = (x, y, [x0, x1], [y0, y1]) => x >= x0 && x < x1 && y >= y0 && y < y1;

/** The colour at unit point (x, y), or null for transparent. */
function sample(x, y) {
  if (!inTile(x, y)) return null;
  if (inBox(x, y, BODY_X, BODY_Y) || inBox(x, y, WICK_X, WICK_Y)) return CANDLE;
  if (inBox(x, y, RAIL_X, [RAIL_TOP_Y, RAIL_TOP_Y + RAIL_H])) return RAIL;
  if (inBox(x, y, RAIL_X, [RAIL_BOTTOM_Y, RAIL_BOTTOM_Y + RAIL_H])) return RAIL;
  return BG;
}

/** RGBA pixels for one square size, 4x supersampled and box-filtered. */
function render(size) {
  const ss = 4;
  const px = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let pxi = 0; pxi < size; pxi++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < ss; sy++) {
        for (let sx = 0; sx < ss; sx++) {
          const c = sample((pxi + (sx + 0.5) / ss) / size, (py + (sy + 0.5) / ss) / size);
          if (!c) continue;
          r += c[0];
          g += c[1];
          b += c[2];
          a += c[3];
        }
      }
      const n = ss * ss;
      const i = (py * size + pxi) * 4;
      const cov = a / (255 * n);
      // Straight (non-premultiplied) alpha: average only the covered samples.
      px[i] = cov ? Math.round(r / (cov * n)) : 0;
      px[i + 1] = cov ? Math.round(g / (cov * n)) : 0;
      px[i + 2] = cov ? Math.round(b / (cov * n)) : 0;
      px[i + 3] = Math.round(a / n);
    }
  }
  return px;
}

// ---------- PNG ----------

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'latin1'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size) {
  const px = render(size);
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0; // filter: none
    px.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // colour type: RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// ---------- .ico (Vista+: PNG-compressed entries) ----------

function ico(sizes) {
  const images = sizes.map((s) => ({ s, data: png(s) }));
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(images.length, 4);
  let offset = 6 + images.length * 16;
  const dir = [];
  for (const { s, data } of images) {
    const e = Buffer.alloc(16);
    e[0] = s >= 256 ? 0 : s; // 0 means 256
    e[1] = s >= 256 ? 0 : s;
    e.writeUInt16LE(1, 4); // colour planes
    e.writeUInt16LE(32, 6); // bits per pixel
    e.writeUInt32LE(data.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += data.length;
    dir.push(e);
  }
  return Buffer.concat([header, ...dir, ...images.map((i) => i.data)]);
}

// ---------- .icns (PNG-typed entries; macOS 10.7+) ----------

function icns(entries) {
  const parts = entries.map(([type, size]) => {
    const data = png(size);
    const head = Buffer.alloc(8);
    head.write(type, 0, 'latin1');
    head.writeUInt32BE(data.length + 8, 4);
    return Buffer.concat([head, data]);
  });
  const body = Buffer.concat(parts);
  const head = Buffer.alloc(8);
  head.write('icns', 0, 'latin1');
  head.writeUInt32BE(body.length + 8, 4);
  return Buffer.concat([head, body]);
}

mkdirSync(OUT, { recursive: true });
writeFileSync(join(OUT, 'icon.png'), png(1024));
writeFileSync(join(OUT, 'icon.ico'), ico([16, 24, 32, 48, 64, 128, 256]));
// ic11/ic12 are the @2x variants of 16/32; ic07..ic10 are 128/256/512/1024.
writeFileSync(join(OUT, 'icon.icns'), icns([['ic11', 32], ['ic12', 64], ['ic07', 128], ['ic08', 256], ['ic09', 512], ['ic10', 1024]]));
console.log('icons written to build/: icon.png (1024), icon.ico (16..256), icon.icns (32..1024)');
