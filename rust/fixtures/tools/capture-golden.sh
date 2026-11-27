#!/usr/bin/env bash
# Regenerate every golden fixture from the OLD app (TEAM.md rule 4).
#
#   bash rust/fixtures/tools/capture-golden.sh
#
# Needs: python3 (+ Pillow for real screenshots; without it the shots json say
# «Pillow не установлен» and the goldens change), node ≥ 20 and the repo's
# node_modules (ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm ci --ignore-scripts is
# enough: only esbuild is used, nothing of Electron is loaded).
#
# Writes:
#   rust/fixtures/golden/vault/            the demo vault (scripts/make-demo-vault.py, clock pinned)
#   rust/fixtures/golden/expected/         index.json, day_<D>.json, defaults.json (scripts/preview/dump.ts)
#   rust/fixtures/golden/expected/helpers/ the pure helpers of src/shared/macro.ts
#   rust/fixtures/edge/<case>/vault, expected/   the hand-made edge vaults and loadDay() over them
set -euo pipefail
cd "$(dirname "$0")/../../.."
ROOT=$PWD
FIX=rust/fixtures
TOOLS=$FIX/tools
GOLD=$FIX/golden
TODAY=${GOLDEN_TODAY:-2026-10-26}
BUILD=$(mktemp -d)
trap 'rm -rf "$BUILD"' EXIT

[ -x node_modules/.bin/esbuild ] || { echo "node_modules missing: ELECTRON_SKIP_BINARY_DOWNLOAD=1 npm ci --ignore-scripts" >&2; exit 1; }
python3 -c 'import PIL' 2>/dev/null || echo "warning: Pillow not installed — screenshots will be «ok: false» and the goldens will differ" >&2

bundle() { # <entry.ts> <out.cjs>  — the same esbuild call scripts/preview/preview.sh makes
  node_modules/.bin/esbuild "$1" --bundle --platform=node --format=cjs --alias:@shared=./src/shared --outfile="$2" --log-level=warning
}

echo "== demo vault (today pinned to $TODAY)"
python3 $TOOLS/make-vault.py "$GOLD/vault" "$TODAY"

echo "== scripts/preview/dump.ts → expected/"
bundle scripts/preview/dump.ts "$BUILD/dump.cjs"
bundle $TOOLS/canon-dir.ts "$BUILD/canon-dir.cjs"
node "$BUILD/dump.cjs" "$ROOT/$GOLD/vault" "$BUILD/dump"
rm -rf "$GOLD/expected"
# defaults.json (DEFAULT_SETTINGS / DEFAULT_PROFILE) is desk-app's settings contract, not the day's,
# and it names the owner's GitHub repo: not a desk-core golden.
rm -f "$BUILD/dump/defaults.json"
node "$BUILD/canon-dir.cjs" "$BUILD/dump" "$ROOT/$GOLD/vault" "$GOLD/expected"

echo "== helpers → expected/helpers/"
bundle $TOOLS/capture-helpers.ts "$BUILD/helpers.cjs"
node "$BUILD/helpers.cjs" "$ROOT/$GOLD/vault" "$GOLD/expected/helpers"

echo "== stage 6 (preflight, wizard, settings import, schedule) → expected/stage6/"
bundle $TOOLS/capture-stage6.ts "$BUILD/stage6.cjs"
TZ=Europe/Kyiv node "$BUILD/stage6.cjs" "$GOLD/expected/stage6"

echo "== edge vaults"
python3 $TOOLS/make-edge.py "$FIX/edge"
bundle $TOOLS/dump-edge.ts "$BUILD/dump-edge.cjs"
node "$BUILD/dump-edge.cjs" "$FIX/edge"

echo "== size check (PNG ≤ 200 KB, no personal data)"
big=$(find "$FIX" -name '*.png' -size +200k)
[ -z "$big" ] || { echo "PNG over 200 KB: $big" >&2; exit 1; }
# Placeholder users in the fixtures are «T» and «trader»; any other Windows user name is a leak.
if grep -rIl -i -P 'avitr|michael|mykhailo|vitryk|C:\\\\+Users\\\\+(?!T\\|trader\\)|/home/|sessionid=' "$FIX" --exclude-dir=tools; then
  echo "personal data or a machine path in the fixtures (above)" >&2; exit 1
fi
echo "done: $(find "$GOLD/expected" -name '*.json' | wc -l) golden json, $(find "$FIX/edge" -name case.json | wc -l) edge cases"
