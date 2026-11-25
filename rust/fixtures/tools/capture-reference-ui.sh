#!/usr/bin/env bash
# Reference screenshots of the OLD macro-review screen (Electron renderer, no Electron),
# for TZ stage 3: «снимки egui рядом с снимками нынешнего preview.sh по тем же шагам steps.json».
#
#   bash rust/fixtures/tools/capture-reference-ui.sh
#
# Wraps scripts/preview/ without changing it:
#   1. electron-vite build → out/renderer (the renderer only needs Vite; no Electron binary)
#   2. VAULT=rust/fixtures/golden/vault SHOOT=0 scripts/preview/preview.sh → the site + the real loadDay() dump
#   3. scripts/preview/shoot.mjs for steps.json and steps-memory.json, light and dark
#   4. rust/fixtures/tools/ui-text.mjs: the same steps, the on-screen text per step (JSON)
#   5. PNGs quantized / downscaled to ≤ 250 KB into rust/fixtures/reference-ui/<theme>/
#
# The clock: the demo vault's «today» is 2026-10-26 (pinned by make-vault.py). The preview
# scripts were reviewed at 09:40 Kyiv (NOW=…T06:40:00Z in September, summer time); the same
# Kyiv wall clock on 2026-10-26 (winter time, UTC+2) is 07:40Z.
#
# Browser: Playwright wants its own Chromium build. When it is not installed and cannot be
# downloaded (the sandbox proxy refuses cdn.playwright.dev), a Chromium that IS present under
# $PLAYWRIGHT_BROWSERS_PATH (another build number) is linked in under the expected name.
set -euo pipefail
cd "$(dirname "$0")/../../.."
FIX=rust/fixtures
OUTDIR=$FIX/reference-ui
NOW=${NOW:-2026-10-26T07:40:00Z}
H=${H:-1400}
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

[ -x node_modules/.bin/electron-vite ] || { echo "node_modules missing: ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm ci --ignore-scripts" >&2; exit 1; }
python3 -c 'import PIL' || { echo "Pillow needed for the downscale" >&2; exit 1; }
[ -d $FIX/golden/vault/indices-kb ] || { echo "golden vault missing: bash $FIX/tools/capture-golden.sh" >&2; exit 1; }

want=$(node -e "console.log(require('playwright').chromium.executablePath())")
if [ ! -x "$want" ]; then
  src=${PLAYWRIGHT_BROWSERS_PATH:-$HOME/.cache/ms-playwright}
  have=$(ls -d "$src"/chromium-[0-9]* 2>/dev/null | sort -V | tail -1 || true)
  shell=$(ls -d "$src"/chromium_headless_shell-[0-9]* 2>/dev/null | sort -V | tail -1 || true)
  [ -n "$have" ] && [ -n "$shell" ] || { echo "no Playwright Chromium: npx playwright install chromium (cdn.playwright.dev must be reachable)" >&2; exit 1; }
  build=$(basename "$(dirname "$(dirname "$want")")" | sed 's/^chromium-//')
  mkdir -p "$TMP/pw"
  ln -s "$have" "$TMP/pw/chromium-$build"
  ln -s "$shell" "$TMP/pw/chromium_headless_shell-$build"
  for d in "$src"/ffmpeg-*; do [ -e "$d" ] && ln -s "$d" "$TMP/pw/"; done
  export PLAYWRIGHT_BROWSERS_PATH=$TMP/pw
  echo "note: Playwright wants chromium-$build; using $(basename "$have") from $src"
fi

echo "== renderer build"
node_modules/.bin/electron-vite build >/dev/null
echo "== preview site over the golden vault"
VAULT=$FIX/golden/vault SHOOT=0 bash scripts/preview/preview.sh >/dev/null

rm -rf "$OUTDIR/light" "$OUTDIR/dark"
for theme in light dark; do
  dark=""; [ $theme = dark ] && dark=1
  for steps in steps steps-memory; do
    echo "== $theme $steps"
    NOW=$NOW H=$H STEPS=scripts/preview/$steps.json OUT=$TMP/$theme DARK=$dark node scripts/preview/shoot.mjs | grep -v '^shot ' || true
    NOW=$NOW H=$H STEPS=scripts/preview/$steps.json OUT=$TMP/$theme-$steps.json DARK=$dark node $FIX/tools/ui-text.mjs >/dev/null
  done
done

echo "== downscale (≤ 250 KB each) and text"
python3 - "$TMP" "$OUTDIR" <<'PY'
import json, sys
from pathlib import Path
from PIL import Image

tmp, out = Path(sys.argv[1]), Path(sys.argv[2])
LIMIT = 250 * 1024
report = {}
for theme in ("light", "dark"):
    (out / theme).mkdir(parents=True, exist_ok=True)
    for png in sorted((tmp / theme).glob("*.png")):
        im = Image.open(png).convert("RGB")
        w0, h0 = im.size
        scale = 1.0
        while True:
            cur = im if scale == 1.0 else im.resize((round(w0 * scale), round(h0 * scale)), Image.LANCZOS)
            # 256 colours keep UI text crisp and cut the size several times over.
            q = cur.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)
            dst = out / theme / png.name
            q.save(dst, "PNG", optimize=True)
            if dst.stat().st_size <= LIMIT or scale < 0.3:
                break
            scale *= 0.85
        report[f"{theme}/{png.name}"] = {"size": list(cur.size), "original": [w0, h0], "scale": round(scale, 3), "bytes": dst.stat().st_size}
    steps = []
    for name in ("steps", "steps-memory"):
        steps += json.loads((tmp / f"{theme}-{name}.json").read_text(encoding="utf-8"))
    (out / theme / "text.json").write_text(json.dumps(steps, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
(out / "shots.json").write_text(json.dumps(report, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
big = [k for k, v in report.items() if v["bytes"] > LIMIT]
print(len(report), "PNGs,", sum(v["bytes"] for v in report.values()) // 1024, "KB total", ("OVER LIMIT: " + ", ".join(big)) if big else "")
PY
if grep -rIl -i -E 'avitr|michael|mykhailo|vitryk|/home/' "$OUTDIR"/*/text.json; then echo "personal data or a machine path in the text dumps" >&2; exit 1; fi
echo "done: $OUTDIR"
