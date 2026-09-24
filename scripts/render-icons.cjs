// Offscreen SVG rasterizer for Strategy Desk. Launched only by npm run icons.
// Full original bull/bear/chart scene: build/icon-src/icon.svg (64 px and up).
// Two trading candles: build/icon-src/icon-small.svg (16/24/32/48 px).
// Every size is rendered directly from SVG, never resized from a larger PNG.
// One hidden window hosts exact-size canvases; its viewport never sets PNG size.
// The existing runtime/builder wiring consumes build/icon.{png,ico,icns}.
const { app, BrowserWindow } = require('electron');
const { readFileSync, writeFileSync, mkdirSync, mkdtempSync } = require('node:fs');
const { join } = require('node:path');
const { tmpdir } = require('node:os');

const OUT = join(__dirname, '..', 'build');
const pngs = new Map();
const sizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024];
const smallMaxSize = 48;
const profile = mkdtempSync(join(tmpdir(), 'strategy-desk-icon-render-'));
app.setPath('userData', profile);
app.setPath('sessionData', profile);
app.commandLine.appendSwitch('force-device-scale-factor', '1');
app.disableHardwareAcceleration();
app.on('window-all-closed', () => {}); // Finish writing icons after the window closes.
const timeout = setTimeout(() => {
  console.error('Icon rendering timed out; existing icons have not been replaced.');
  app.exit(1);
}, 45000);

async function render(win, size, svg) {
  const src = 'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64');
  const dataUrl = await win.webContents.executeJavaScript(`(async () => {
    const size = ${JSON.stringify(size)};
    const img = new Image();
    img.src = ${JSON.stringify(src)};
    await img.decode();
    const canvas = document.createElement('canvas');
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Canvas 2D context unavailable');
    ctx.drawImage(img, 0, 0, size, size);
    const pixels = ctx.getImageData(0, 0, size, size).data;
    const alpha = (x, y) => pixels[(y * size + x) * 4 + 3];
    if (alpha(Math.floor(size / 2), Math.floor(size / 2)) !== 255 ||
        [[0, 0], [size - 1, 0], [0, size - 1], [size - 1, size - 1]]
          .some(([x, y]) => alpha(x, y) > 24)) { // anti-aliasing leaves a few alpha units in the corners at 16 px
      throw new Error('Missing tile or transparent corners in rendered ' + size + 'px canvas (centre alpha ' + alpha(Math.floor(size / 2), Math.floor(size / 2)) + ', corners ' + [[0, 0], [size - 1, 0], [0, size - 1], [size - 1, size - 1]].map(([x, y]) => alpha(x, y)).join('/') + ')');
    }
    return canvas.toDataURL('image/png');
  })()`);
  const prefix = 'data:image/png;base64,';
  if (!dataUrl.startsWith(prefix)) throw new Error('Canvas did not return a PNG');
  const data = Buffer.from(dataUrl.slice(prefix.length), 'base64');
  if (data.length < 24 || !data.subarray(0, 8).equals(Buffer.from('89504e470d0a1a0a', 'hex')) ||
      data.readUInt32BE(16) !== size || data.readUInt32BE(20) !== size) {
    throw new Error(`Expected a ${size}x${size} PNG`);
  }
  return data;
}

// ---------- .ico (Vista+: PNG-compressed entries) ----------

function ico(sizes) {
  const images = sizes.map((s) => ({ s, data: pngs.get(s) }));
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
    const data = pngs.get(size);
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


app.whenReady().then(async () => {
  const full = readFileSync(join(OUT, 'icon-src', 'icon.svg'), 'utf8');
  const small = readFileSync(join(OUT, 'icon-src', 'icon-small.svg'), 'utf8');
  const win = new BrowserWindow({
    width: 1024, height: 1024, useContentSize: true, frame: false,
    show: false, skipTaskbar: true, transparent: true,
    backgroundColor: '#00000000', enableLargerThanScreen: true,
    webPreferences: {
      offscreen: true, backgroundThrottling: false, zoomFactor: 1,
      nodeIntegration: false, contextIsolation: true, sandbox: true,
    },
  });
  try {
    const html = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:"><body></body>`;
    await win.loadURL('data:text/html;charset=utf-8,' + encodeURIComponent(html));
    for (const size of sizes) {
      pngs.set(size, await render(win, size, size <= smallMaxSize ? small : full));
      console.log(`Rendered ${size}px (${size <= smallMaxSize ? 'small' : 'full'} SVG)`);
    }
  } finally {
    win.destroy();
  }
  // Finish every render/container before replacing any of the existing icons.
  const outputs = [
    ['icon.png', pngs.get(1024)],
    ['icon.ico', ico([16, 24, 32, 48, 64, 128, 256])],
    ['icon.icns', icns([['ic11', 32], ['ic12', 64], ['ic07', 128], ['ic08', 256], ['ic09', 512], ['ic10', 1024]])],
  ];
  mkdirSync(OUT, { recursive: true });
  for (const [name, data] of outputs) writeFileSync(join(OUT, name), data);
  console.log('Generated build/icon.png, build/icon.ico and build/icon.icns');
  clearTimeout(timeout);
  app.exit(0);
}).catch((error) => {
  // Avoid dumping the SVG's entire data URL on a renderer startup failure.
  console.error('Icon renderer failed:', error.code ?? error.message);
  clearTimeout(timeout);
  app.exit(1);
});
