#!/usr/bin/env bash
# Runs bench.ps1's real Windows flow on Linux against a pretend desk, so its
# control flow (variants, data-ready, switch frames, report) is exercised
# before the owner spends a session on it. Needs pwsh 7 on PATH (or $PWSH).
#
#   scripts/rust/bench-sim.sh [out.json]
#
# Only two lines of bench.ps1 are swapped: the OS guard, and Add-Type of the
# Win32 helper for scripts/rust/sim/FakeNative.cs. Numbers are made up.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
pwsh="${PWSH:-pwsh}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
out="${1:-$tmp/sim-report.json}"
mkdir -p "$tmp/vault/indices-kb/state" "$tmp/appdata" "$tmp/local"
cp "$here/sim/fake-desk.py" "$tmp/strategy-desk"
chmod +x "$tmp/strategy-desk"
python3 - "$here/bench.ps1" "$tmp/bench.ps1" "$here/sim/FakeNative.cs" <<'PY'
import sys
src, dst, cs = sys.argv[1:]
s = open(src, encoding="utf-8-sig").read()
guard = "if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'bench.ps1 measures Windows only; use -SelfTest elsewhere.' }"
add = "Add-Type -TypeDefinition $NativeSource -Language CSharp"
assert guard in s and add in s, "bench.ps1 changed: update bench-sim.sh"
s = s.replace(guard, "# (sim) OS guard removed")
s = s.replace(add, "Add-Type -TypeDefinition (Get-Content -Raw '%s') -Language CSharp" % cs)
# The sim's script lives in a temp dir; RepoRoot must still be the repo.
s = s.replace("$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..\\..')).Path", "$RepoRoot = '%s'" % sys.argv[1].rsplit("/scripts/", 1)[0])
open(dst, "w", encoding="utf-8").write(s)
PY
APPDATA="$tmp/appdata" LOCALAPPDATA="$tmp/local" "$pwsh" -NoProfile -File "$tmp/bench.ps1" \
  -Kind rust -Exe "$tmp/strategy-desk" -Vault "$tmp/vault" -Runs 3 -IdleRuns 1 -IdleSeconds 1 -SettleSeconds 0 \
  -Variants "base|DESK_OPT=base,all|DESK_OPT=all" -StreamFrames -SwitchCount 40 \
  -FakeClaude "$tmp/strategy-desk" -Commit sim -Label "bench-sim (not a measurement)" -Out "$out" -Force
python3 - "$out" <<'PY'
import json, sys
r = json.load(open(sys.argv[1], encoding="utf-8-sig"))
m = {x["id"]: x for x in r["metrics"]}
v = {x["label"]: {y["id"]: y for y in x["metrics"]} for x in r["variants"]}
checks = [
    (len(r["variants"]) == 2, "two variant columns"),
    (v["base"]["data_ready_warm_ms"]["extra"]["self_reported_ms"][0] == 260.0, "base data ready self-reported"),
    (v["all"]["data_ready_warm_ms"]["extra"]["self_reported_ms"][0] == 150.0, "variant env reached the process"),
    (m["cold_start_ms"]["value"] is not None and m["cold_start_ms"]["value"] < m["data_ready_cold_ms"]["value"], "first frame derived before data ready"),
    (m["switch_ms"]["status"] in ("pass", "fail") and len(m["switch_ms"]["samples"]) == 40, "switch_ms from --bench-switch"),
    (m["switch_frame_p99_ms"]["value"] == 18.0, "switch frames: worst window"),
    (m["frame_p50_ms"]["status"] == "skip" and "stage 5" in m["frame_p50_ms"]["note"], "stream frames wait for stage 5"),
    (m["idle_private_mb"]["value"] == 88.0, "idle ran"),
    (r["harness"]["vault"] == "-Vault", "vault source, not path"),
    (all("/" not in x["exe"] for x in r["variants"]), "no paths in the report"),
]
bad = [n for ok, n in checks if not ok]
for ok, n in checks:
    print(("ok   " if ok else "FAIL ") + n)
sys.exit(1 if bad else 0)
PY
# An older build without --bench-data-ready: start falls back, data_ready says why.
SIM_OLD_BUILD=1 APPDATA="$tmp/appdata" LOCALAPPDATA="$tmp/local" "$pwsh" -NoProfile -File "$tmp/bench.ps1" \
  -Kind rust -Exe "$tmp/strategy-desk" -Vault "$tmp/vault" -Runs 2 -SkipIdle -Commit sim-old -Out "$tmp/old.json" -Force >/dev/null
python3 - "$tmp/old.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1], encoding="utf-8-sig"))
m = {x["id"]: x for x in r["metrics"]}
ok = m["warm_start_ms"]["value"] is not None and m["data_ready_warm_ms"]["status"] == "skip" and "variants" not in r
print(("ok   " if ok else "FAIL ") + "old build: --bench-first-frame fallback, data_ready skipped")
sys.exit(0 if ok else 1)
PY
echo "bench-sim passed: $out"
