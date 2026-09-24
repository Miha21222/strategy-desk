// Draws the Strategy Desk app icon and writes build/icon.png, build/icon.ico
// and build/icon.icns. No image library and no native tooling: the mark is
// plain geometry, PNG is zlib + four chunks, and .ico / .icns are containers of
// PNGs. Run it with `npm run icons` after changing anything below.
//
// The mark: three candles chart an up-move: mint rise, coral pullback, mint rise.
// A deep navy (#0b1220) / indigo (#242743) tile has an upper-right glow in the
// desk's saturated accent (#6155ec), with mint #6cdbc0 and coral #f2544b candles.
// Every size uses the same composition, rendered natively with 4x coverage AA.
// At 16/24/32 px, bodies and wicks snap to whole pixels; bodies are at least
// 3 px wide and match the wick's odd width for a crisp, centred stem.
// Main-process ?asset imports use these same build files; no separate copy.
import { deflateSync } from 'node:zlib';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const OUT = join(dirname(fileURLToPath(import.meta.url)), '..', 'build');

// ---------- the drawing, in unit coordinates (0..1) ----------

const NAVY = [0x0b, 0x12, 0x20, 0xff];
const INDIGO = [0x24, 0x27, 0x43, 0xff];
const ACCENT = [0x61, 0x55, 0xec, 0xff];
const GREEN = [0x6c, 0xdb, 0xc0, 0xff];
const RED = [0xf2, 0x54, 0x4b, 0xff];

const CORNER = 0.22; // rounded-square, Windows/macOS tile look
const CANDLES = [
  { x: 0.25, bodyY: [0.49, 0.75], wickY: [0.40, 0.84], color: GREEN },
  { x: 0.50, bodyY: [0.49, 0.62], wickY: [0.38, 0.72], color: RED },
  { x: 0.75, bodyY: [0.25, 0.62], wickY: [0.16, 0.72], color: GREEN },
];

const mix = (a, b, t) => a.map((v, i) => v + (b[i] - v) * Math.max(0, Math.min(1, t)));

function geometry(size) {
  const small = size <= 32;
  const snap = (v) => Math.round(v * size) / size;
  const bodyW = small ? Math.max(3, 2 * Math.round((size * 0.14 - 1) / 2) + 1) : size * 0.14;
  const wickW = small ? 1 : size * 0.035;
  return CANDLES.map((candle) => {
    const left = small ? Math.round(candle.x * size - bodyW / 2) : candle.x * size - bodyW / 2;
    const wickLeft = left + (bodyW - wickW) / 2;
    return {
      bodyX: [left / size, (left + bodyW) / size],
      bodyY: small ? candle.bodyY.map(snap) : candle.bodyY,
      wickX: [wickLeft / size, (wickLeft + wickW) / size],
      wickY: small ? candle.wickY.map(snap) : candle.wickY,
      color: candle.color,
    };
  });
}

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
function sample(x, y, candles) {
  if (!inTile(x, y)) return null;
  for (const candle of candles) {
    if (inBox(x, y, candle.bodyX, candle.bodyY) || inBox(x, y, candle.wickX, candle.wickY)) {
      return candle.color;
    }
  }
  const tile = mix(INDIGO, NAVY, y);
  const accentGlow = Math.max(0, 1 - Math.hypot((x - 0.85) / 1.1, y / 1.1));
  return mix(tile, ACCENT, 0.48 * accentGlow * accentGlow);
}

/** RGBA pixels for one square size, 4x supersampled and box-filtered. */
function render(size) {
  const ss = 4;
  const shape = geometry(size);
  const px = Buffer.alloc(size * size * 4);
  for (let py = 0; py < size; py++) {
    for (let pxi = 0; pxi < size; pxi++) {
      let r = 0;
      let g = 0;
      let b = 0;
      let a = 0;
      for (let sy = 0; sy < ss; sy++) {
        for (let sx = 0; sx < ss; sx++) {
          const c = sample((pxi + (sx + 0.5) / ss) / size, (py + (sy + 0.5) / ss) / size, shape);
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

const pngs = new Map();

function png(size) {
  if (pngs.has(size)) return pngs.get(size);
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
  const data = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw, { level: 9 })),
    chunk('IEND', Buffer.alloc(0)),
  ]);
  pngs.set(size, data);
  return data;
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
