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
cp "$here/sim/fake-desk.py" "$tmp/sim-desk"
chmod +x "$tmp/sim-desk"
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
  -Kind rust -Exe "$tmp/sim-desk" -Vault "$tmp/vault" -Runs 3 -IdleRuns 1 -IdleSeconds 1 -SettleSeconds 0 \
  -Variants "base|DESK_OPT=base,all|DESK_OPT=all" -StreamFrames -SwitchCount 40 \
  -FakeClaude "$tmp/sim-desk" -TraceDir "$tmp/traces-v" -Commit sim -Label "bench-sim (not a measurement)" -Out "$out" -Force
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
# The owner's A/B matrix: two exes x options, cold only for default, idle
# for default vs shots_full, traces into a folder, a day with a screenshot.
cp "$tmp/sim-desk" "$tmp/sim-desk-wgpu"
echo '{"ok": true, "shots": []}' > "$tmp/vault/indices-kb/state/shots_2026-11-20_eu.json"
echo '{"side": "LONG"}' > "$tmp/vault/indices-kb/state/human_read_2026-11-20_eu.json"
echo '{"ok": true, "shots": []}' > "$tmp/vault/indices-kb/state/shots_2026-11-24_eu.json"
APPDATA="$tmp/appdata" LOCALAPPDATA="$tmp/local" "$pwsh" -NoProfile -File "$tmp/bench.ps1" \
  -Kind rust -Vault "$tmp/vault" -Exes "glow=$tmp/sim-desk,wgpu=$tmp/sim-desk-wgpu" \
  -Opts "default,vsync0,shots_full" -Runs 2 -VariantRuns 2 -IdleRuns 1 -IdleSeconds 1 -SettleSeconds 0 \
  -TraceDir "$tmp/ab" -Commit sim-ab -Out "$tmp/ab.json" -Force > "$tmp/ab.log"
grep -c '^\[' "$tmp/ab.log" >/dev/null
python3 - "$tmp/ab.json" "$tmp/ab" <<'PY'
import json, os, sys
r = json.load(open(sys.argv[1], encoding="utf-8-sig"))
d = sys.argv[2]
v = {x["label"]: x for x in r["variants"]}
mid = lambda lab, i: {m["id"]: m for m in v[lab]["metrics"]}.get(i, {})
checks = [
    (len(v) == 6 and list(v)[0] == "glow/default", "6 variants, glow/default first"),
    (mid("glow/default", "data_ready_cold_ms").get("status") != "skip" and mid("wgpu/default", "data_ready_cold_ms").get("status") != "skip", "cold for both defaults"),
    (mid("glow/vsync0", "data_ready_cold_ms").get("status") == "skip", "no cold for the rest"),
    (mid("glow/vsync0", "data_ready_warm_ms")["extra"]["self_reported_ms"][0] == 150.0, "DESK_OPT reached the process"),
    (mid("glow/shots_full", "idle_private_mb").get("value") is not None and not mid("wgpu/shots_full", "idle_private_mb"), "idle for glow default + shots_full only"),
    (r["harness"].get("desk_now") == "2026-11-20T12:00", "DESK_NOW = newest day with a shot AND a read"),
    (v["glow/default"]["phases"]["warm"]["day_presented"]["n"] == 2, "phases per variant"),
    (all(os.path.exists(os.path.join(d, f)) for f in ("glow_default.jsonl", "wgpu_shots_full.jsonl", "phases.json")), "trace files"),
    (any('"first_shot_uploaded"' in l for l in open(os.path.join(d, "glow_shots_full.jsonl"))), "idle trace has the screenshot"),
    (mid("glow/default", "idle_working_set_mb")["extra"]["screenshot_on_screen"] == [True], "screenshot_on_screen recorded"),
]
for ok, n in checks:
    print(("ok   " if ok else "FAIL ") + n)
sys.exit(0 if all(c for c, _ in checks) else 1)
PY

# Stage 5: autochat for real, the real fake-claude binary as claude (10k-ish
# events, then a run where it dies mid-answer); frames + two app stress rows.
fake="$here/../../rust/target/release/fake-claude"
[ -x "$fake" ] || (cd "$here/../../rust" && cargo build -q --release -p desk-bench --bin fake-claude)
SIM_AUTOCHAT=real APPDATA="$tmp/appdata" LOCALAPPDATA="$tmp/local" "$pwsh" -NoProfile -File "$tmp/bench.ps1" \
  -Kind rust -Exe "$tmp/sim-desk" -Vault "$tmp/vault" -Runs 1 -SkipCold -SkipIdle -StreamFrames \
  -FakeClaude "$fake" -StreamEvents 3000 -StreamRate 1000 -DieAfter 800 -SwitchCount 20 \
  -TraceDir "$tmp/traces-chat" -Commit sim-chat -Out "$tmp/chat.json" -Force > "$tmp/chat.log"
grep -q 'TIMED OUT' "$tmp/chat.log" && { echo "FAIL a launch timed out (lineKey \"\" bug?)"; exit 1; }
python3 - "$tmp/chat.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1], encoding="utf-8-sig"))
m = {x["id"]: x for x in r["metrics"]}
st = {x["id"]: x for x in r.get("stress", [])}
checks = [
    (m["frame_p50_ms"]["status"] == "pass" and m["frame_p99_ms"]["value"] == 11.0, "stream frames measured"),
    (st.get("chat_stream_10k", {}).get("status") == "pass", "stream: desk alive after the stream"),
    (st.get("chat_claude_dies_mid_answer", {}).get("status") == "pass" and "exited after" in st["chat_claude_dies_mid_answer"]["detail"], "claude died mid-answer, desk alive"),
]
for ok, n in checks:
    print(("ok   " if ok else "FAIL ") + n)
sys.exit(0 if all(c for c, _ in checks) else 1)
PY

# An older build without --bench-data-ready: start falls back, data_ready says why.
SIM_OLD_BUILD=1 APPDATA="$tmp/appdata" LOCALAPPDATA="$tmp/local" "$pwsh" -NoProfile -File "$tmp/bench.ps1" \
  -Kind rust -Exe "$tmp/sim-desk" -Vault "$tmp/vault" -Runs 2 -SkipIdle -TraceDir "$tmp/traces-old" -Commit sim-old -Out "$tmp/old.json" -Force >/dev/null
python3 - "$tmp/old.json" <<'PY'
import json, sys
r = json.load(open(sys.argv[1], encoding="utf-8-sig"))
m = {x["id"]: x for x in r["metrics"]}
ok = m["warm_start_ms"]["value"] is not None and m["data_ready_warm_ms"]["status"] == "skip" and "variants" not in r
print(("ok   " if ok else "FAIL ") + "old build: --bench-first-frame fallback, data_ready skipped")
sys.exit(0 if ok else 1)
PY
if [ -d "$here/../../docs/rust/perf/ab" ] && [ -n "$(find "$here/../../docs/rust/perf/ab" -newer "$tmp/bench.ps1" -print -quit)" ]; then
  echo "FAIL the sim wrote traces into the repo"; exit 1
fi
echo "bench-sim passed: $out"
