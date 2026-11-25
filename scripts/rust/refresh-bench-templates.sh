#!/usr/bin/env bash
# Re-copies the JSON templates gen-vault perturbs out of a fresh demo vault.
#
#   scripts/rust/refresh-bench-templates.sh
#
# gen-vault does not invent file shapes: it takes one full demo day
# (2026-09-14, the richest day of scripts/make-demo-vault.py) plus the
# dateless files, and rewrites dates and numbers. Run this after the demo
# vault generator changes a file shape, then commit the diff under
# rust/crates/desk-bench/templates/.
set -euo pipefail
repo="$(cd "$(dirname "$0")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 "$repo/scripts/make-demo-vault.py" "$tmp/v" >/dev/null
src="$tmp/v/indices-kb"
dst="$repo/rust/crates/desk-bench/templates"
rm -rf "$dst"
mkdir -p "$dst/state" "$dst/config" "$dst/journal"
d=2026-09-14
for f in "$src"/state/*_"$d"*.json; do cp "$f" "$dst/state/"; done
cp "$src/state/week_2026-09-18.json" "$src/state/ideas.json" "$dst/state/"
cp "$src/config/scale.json" "$dst/config/"
head -n 1 "$src/journal/scale_factor_scores.csv" > "$dst/journal/scale_factor_scores.header.csv"
head -n 2 "$src/journal/lessons.md" > "$dst/journal/lessons.header.md"
ls "$dst"/*/ | sed 's/^/  /'
echo "templates refreshed in $dst"
