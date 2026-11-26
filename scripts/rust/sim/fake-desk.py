#!/usr/bin/env python3
"""A pretend strategy-desk for bench-sim.sh: speaks the bench protocol, draws nothing."""
import json, os, sys, time

T0 = time.monotonic()


def at(ms):
    """Sleep until `ms` after this process started, like a real clock would show."""
    time.sleep(max(0.0, ms / 1000.0 - (time.monotonic() - T0)))

args = sys.argv[1:]
opts = [o for o in os.environ.get("DESK_OPT", "").split(",") if o]
fast = "all" in opts or "vsync0" in opts
TRACE = os.environ.get("DESK_STARTUP_TRACE") == "1"


def trace(phase, ms, pm=60.0):
    if TRACE:
        at(ms)
        print(json.dumps({"trace": phase, "ms": ms, "private_mb": pm, "ws_mb": pm - 10}), file=sys.stderr, flush=True)

vault = os.environ.get("DESK_VAULT", "")
if os.environ.get("DESK_BENCH_AUTOCHAT") == "1":
    print("strategy-desk: DESK_BENCH_AUTOCHAT=1 ignored — the chat arrives in stage 5", file=sys.stderr, flush=True)
if "--bench-data-ready" in args:
    if os.environ.get("SIM_OLD_BUILD") == "1":
        print("strategy-desk: unknown argument --bench-data-ready", file=sys.stderr)
        sys.exit(2)
    ff, dr = (90.0, 150.0) if fast else (120.0, 260.0)
    if not vault:
        time.sleep(100)
    trace("main", 1.0)
    trace("first_frame_presented", ff - 1, 70.0)
    trace("day_drawn", dr - 5, 85.0)
    trace("day_presented", dr - 1, 88.0)
    at(dr)
    print(json.dumps({"first_frame_ms": ff, "data_ready_ms": dr, "private_mb": 91.5, "ws_mb": 80.2}), flush=True)
    sys.exit(0)
if "--bench-first-frame" in args:
    time.sleep(0.05)
    print(json.dumps({"first_frame_ms": 110.0, "rss_mb": 40.0}), flush=True)
    sys.exit(0)
if "--bench-switch" in args:
    n = int(args[args.index("--bench-switch") + 1])
    print("frame_stats n=600 p50=3.10ms p95=7.90ms p99=12.40ms max=20.00ms", file=sys.stderr, flush=True)
    print("frame_stats n=580 p50=3.30ms p95=9.00ms p99=18.00ms max=25.00ms", file=sys.stderr, flush=True)
    print(json.dumps({"switch_ms": [20.0 + (i % 7) for i in range(n)]}), flush=True)
    sys.exit(0)
# A window that stays until killed.
trace("first_frame_presented", 100.0, 70.0)
if os.environ.get("DESK_NOW"):
    trace("first_shot_uploaded", 150.0, 140.0 if "shots_full" in opts else 95.0)
if os.environ.get("DESK_FRAME_STATS") == "1":
    print("frame_stats n=1 p50=1.00ms p95=1.00ms p99=1.00ms max=1.00ms", file=sys.stderr, flush=True)
time.sleep(3600)
