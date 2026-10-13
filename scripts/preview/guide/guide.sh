#!/usr/bin/env bash
# Regenerates every picture of the user guide (docs/guide/img/*.png, docs/guide/gif/*.gif)
# from the demo vault, without Electron. Needs: npm run build; playwright + chromium; ffmpeg.
#   bash scripts/preview/guide/guide.sh          # everything
#   bash scripts/preview/guide/guide.sh png      # only the screenshots
#   bash scripts/preview/guide/guide.sh gif      # only the recordings
# The demo days are 14–24 September 2026; NOW fakes the Kyiv clock (06:40Z = 09:40 Kyiv).
# `?fresh=1` strips the demo's own answers so a day starts with «Сначала твой взгляд»;
# `?wizard=1&pf=<variant>` opens the setup wizard over a fake diagnosis (see desk-mock.js).
set -euo pipefail
cd "$(dirname "$0")/../../.."
what=${1:-all}
SHOOT=0 bash scripts/preview/preview.sh
G=scripts/preview/guide
if [ "$what" != gif ]; then
  export OUT=docs/guide/img H=1000
  NOW=2026-09-14T06:40:00Z QUERY='?fresh=1'                 STEPS=$G/steps-morning.json        node scripts/preview/shoot.mjs
  NOW=2026-09-14T17:10:00Z QUERY='?fresh=1'                 STEPS=$G/steps-evening.json        node scripts/preview/shoot.mjs
  NOW=2026-09-18T17:30:00Z QUERY='?fresh=1'                 STEPS=$G/steps-friday.json         node scripts/preview/shoot.mjs
  NOW=2026-09-15T06:40:00Z QUERY='?wizard=1&pf=ok'          STEPS=$G/steps-wizard.json         node scripts/preview/shoot.mjs
  NOW=2026-09-15T06:40:00Z QUERY='?wizard=1&pf=fresh'       STEPS=$G/steps-wizard-fresh.json   node scripts/preview/shoot.mjs
  NOW=2026-09-15T06:40:00Z QUERY='?wizard=1&pf=session-empty' STEPS=$G/steps-wizard-session.json node scripts/preview/shoot.mjs
fi
if [ "$what" != png ]; then
  OUT=docs/guide/gif SCENES=$G/gifs.json node scripts/preview/record.mjs
fi
echo "guide images: docs/guide/img docs/guide/gif"
