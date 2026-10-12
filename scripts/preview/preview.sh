#!/usr/bin/env bash
# Screenshots of the renderer against the demo vault, without Electron (note 23 review loop).
# Needs: npm run build; a demo vault (python scripts/make-demo-vault.py demo-vault); playwright + chromium.
#   NOW=2026-09-15T06:40:00Z H=2000 bash scripts/preview/preview.sh
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT=scripts/preview/out
mkdir -p "$OUT"
npx esbuild scripts/preview/dump.ts --bundle --platform=node --format=cjs --alias:@shared=./src/shared --outfile="$OUT/dump.cjs" --log-level=warning
node "$OUT/dump.cjs" "${VAULT:-demo-vault}" "$OUT/data"
rm -rf "$OUT/site"
cp -r out/renderer "$OUT/site"
cp -r "$OUT/data" "$OUT/site/data"
mkdir -p "$OUT/site/vault" && cp -r "${VAULT:-demo-vault}/indices-kb" "$OUT/site/vault/"
cp scripts/preview/desk-mock.js "$OUT/site/"
sed -i 's#<script type="module"#<script src="./desk-mock.js"></script>\n    <script type="module"#' "$OUT/site/index.html"
node scripts/preview/shoot.mjs
echo "shots: $OUT/shots"
