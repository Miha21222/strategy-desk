#!/usr/bin/env python3
"""Compare perf reports: «метрика · до · после · цель · статус».

    python scripts/rust/report.py docs/rust/perf/                # every report there
    python scripts/rust/report.py base.json new.json            # first = до, last = после
    python scripts/rust/report.py --before E.json --after R.json [--summary out.json]
    python scripts/rust/report.py --rederive-stream docs/rust/perf/<report>.json [--trace <dir|file.jsonl>]
    python scripts/rust/report.py --self-test

Reads reports of schema desk-perf/1 (docs/rust/PERF.md), prints the table and
writes docs/rust/perf/summary.json for the dashboard: before/after per
metric, the trend over every Rust report, and what failed with the reason.

«До» is the newest Electron report unless --before says otherwise; «после»
is the newest Rust report, preferring representative ones (the owner's
laptop) over the rest. Standard library only, Python 3.9+.
"""
from __future__ import annotations

import argparse
import json
import math
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_SUMMARY = REPO / "docs" / "rust" / "perf" / "summary.json"

# Same ids as rust/crates/desk-bench/src/report.rs METRICS; the Russian
# label is the one people read, so it lives here too (bench.ps1 is ASCII).
LABELS = {
    "cold_start_ms": "Холодный старт до первого кадра",
    "warm_start_ms": "Тёплый старт",
    "data_ready_cold_ms": "Данные дня на экране, холодный старт",
    "data_ready_warm_ms": "Данные дня на экране, тёплый старт",
    "idle_working_set_mb": "RAM в покое (Working Set)",
    "idle_private_mb": "RAM в покое (Private Bytes)",
    "idle_cpu_pct": "CPU в покое",
    "idle_frames_per_min": "Кадров в минуту в покое",
    "frame_p50_ms": "Кадр p50 при стриме",
    "frame_p95_ms": "Кадр p95 при стриме",
    "frame_p99_ms": "Кадр p99 при стриме",
    "frame_p50_growth_ms_per_min": "Рост p50 кадра за минуту стрима",
    "switch_frame_p50_ms": "Кадр p50 при переключении дней",
    "switch_frame_p95_ms": "Кадр p95 при переключении дней",
    "switch_frame_p99_ms": "Кадр p99 при переключении дней",
    "day_load_ms": "Загрузка дня (~22 файла)",
    "day_load_real_size_ms": "Загрузка дня, файлы реального размера",
    "vault_x10_parse_ms": "Разбор всего vault ×10",
    "vault_x10_list_days_ms": "Список дней vault ×10",
    "stream_parse_10k_ms": "Разбор 10 тыс. событий stream-json",
    "stream_pipe_10k_ms": "10 тыс. событий через трубу fake-claude",
    "watch_latency_ms": "Перерисовка после записи в state/",
    "switch_ms": "Переключение экрана / дня",
    "exe_mb": "Размер exe",
    "install_dir_mb": "Размер установленной папки",
    "installer_mb": "Размер установщика",
    "soak_private_growth_pct": "Рост Private Bytes за сутки",
}
ORDER = list(LABELS)

# Current targets (target, stat) — the same as METRICS in report.rs and
# $Catalog in bench.ps1. A report keeps the target it was judged by; the
# summary re-judges every value by these, so an old report reads honestly
# after a target changes (the old one stays as `target_in_report`).
TARGETS = {
    "cold_start_ms": (300.0, "median"),
    "warm_start_ms": (200.0, "median"),
    "data_ready_cold_ms": (500.0, "median"),
    "data_ready_warm_ms": (300.0, "median"),
    "idle_working_set_mb": (100.0, "median"),
    "idle_private_mb": (100.0, "median"),
    "idle_cpu_pct": (1.0, "median"),
    "idle_frames_per_min": (None, "median"),
    "frame_p50_ms": (8.0, "median"),
    "frame_p95_ms": (16.0, "max"),
    "frame_p99_ms": (33.0, "max"),
    "frame_p50_growth_ms_per_min": (None, "max"),
    "switch_frame_p50_ms": (8.0, "median"),
    "switch_frame_p95_ms": (16.0, "max"),
    "switch_frame_p99_ms": (33.0, "max"),
    "day_load_ms": (30.0, "median"),
    "day_load_real_size_ms": (30.0, "median"),
    "vault_x10_parse_ms": (None, "median"),
    "vault_x10_list_days_ms": (None, "median"),
    "stream_parse_10k_ms": (None, "median"),
    "stream_pipe_10k_ms": (None, "median"),
    "watch_latency_ms": (350.0, "p95"),
    "switch_ms": (100.0, "p95"),
    "exe_mb": (15.0, "max"),
    "install_dir_mb": (None, "max"),
    "installer_mb": (20.0, "max"),
    "soak_private_growth_pct": (5.0, "max"),
}
# Why a target differs from the first TEAM.md table: one line each.
TARGET_WHY = {
    "warm_start_ms": "цель 150 → 200 мс (26.11): ~132 мс тёплого старта — окно и контекст OpenGL в драйвере, до нашего кода",
    "idle_working_set_mb": "цель 80 → 100 МБ (26.11): ~81 МБ берёт драйвер OpenGL до нашего кода; замер со снимком графика на экране",
    "idle_private_mb": "цель 80 → 100 МБ (26.11): ~81 МБ Private — драйвер OpenGL до нашего кода; замер со снимком графика на экране",
}


def rejudge(m: dict) -> dict:
    """Judge a metric by today's target (pass/fail only; skip/info stay)."""
    cur = TARGETS.get(m.get("id"))
    if cur is None:
        return m
    target = cur[0]
    if m.get("target") != target:
        m["target_in_report"] = m.get("target")
        m["target"] = target
        if m["id"] in TARGET_WHY:
            m["target_why"] = TARGET_WHY[m["id"]]
    v = m.get("value")
    if m.get("status") in ("pass", "fail") and isinstance(v, (int, float)) and target is not None:
        m["status"] = "pass" if v <= target else "fail"
    return m
STATUS_RU = {"pass": "ок", "fail": "ПРОВАЛ", "skip": "нет данных", "info": "инфо"}


def unwrap(v):
    """PowerShell 5.1 can write an array as {"value": [...], "Count": n}."""
    if isinstance(v, dict):
        if set(v) == {"value", "Count"} and isinstance(v["value"], list):
            return [unwrap(x) for x in v["value"]]
        return {k: unwrap(x) for k, x in v.items()}
    if isinstance(v, list):
        return [unwrap(x) for x in v]
    return v


def load(path: Path) -> dict:
    r = unwrap(json.loads(path.read_text(encoding="utf-8-sig")))
    if r.get("schema") != "desk-perf/1":
        raise ValueError(f"{path}: not a desk-perf/1 report (schema={r.get('schema')!r})")
    for key in ("machine", "build", "metrics"):
        if key not in r:
            raise ValueError(f"{path}: missing {key!r}")
    r["_file"] = str(path)
    for m in r["metrics"]:
        rejudge(m)
    for v in r.get("variants", []) or []:
        for m in v.get("metrics", []):
            rejudge(m)
    r["_metrics"] = {m["id"]: m for m in r["metrics"]}
    return r


def collect(paths: list[str]) -> list[dict]:
    files: list[Path] = []
    for p in paths:
        q = Path(p)
        if q.is_dir():
            files += sorted(x for x in q.glob("*.json") if x.name != "summary.json" and not x.name.endswith(".schema.json"))
        else:
            files.append(q)
    return [load(f) for f in files]


def kind(r: dict) -> str:
    return (r.get("build") or {}).get("kind", "")


def newest(reports: list[dict], kinds: tuple[str, ...]) -> dict | None:
    c = [r for r in reports if kind(r) in kinds]
    if not c:
        return None
    # Representative first, then the latest.
    return sorted(c, key=lambda r: (bool(r.get("representative")), r.get("created_at", "")))[-1]


def fmt(x, unit: str = "") -> str:
    if x is None:
        return "—"
    if isinstance(x, (int, float)):
        if float(x).is_integer():
            s = f"{x:.0f}"
        else:
            s = f"{x:.0f}" if abs(x) >= 100 else f"{x:.1f}" if abs(x) >= 10 else f"{x:.2f}"
        return f"{s} {unit}".strip()
    return str(x)


def describe(r: dict | None) -> dict | None:
    if r is None:
        return None
    b = r.get("build", {})
    return {"file": Path(r["_file"]).name, "label": r.get("label", ""), "kind": b.get("kind", ""),
            "version": b.get("version", ""), "commit": b.get("commit", ""), "date": r.get("date", ""),
            "created_at": r.get("created_at", ""), "representative": bool(r.get("representative"))}


def summarize(reports: list[dict], before: dict | None, after: dict | None) -> dict:
    ids = [i for i in ORDER]
    for r in reports:
        ids += [m for m in r["_metrics"] if m not in ids]
    trend_src = sorted((r for r in reports if kind(r) in ("rust", "synthetic")), key=lambda r: r.get("created_at", ""))
    rows, failed = [], []
    counts = {"pass": 0, "fail": 0, "skip": 0, "info": 0}
    # «После» per metric: the after-report's own number, or — when that run
    # did not measure it (a stream-only session) — the newest report of the
    # same trust (representative or not) that did, named in `after_from`.
    peers = sorted((r for r in reports if kind(r) == kind(after or {}) and bool(r.get("representative")) == bool((after or {}).get("representative"))
                    and r is not after), key=lambda r: r.get("created_at", ""), reverse=True)
    for i in ids:
        mb = before["_metrics"].get(i) if before else None
        ma = after["_metrics"].get(i) if after else None
        after_from = None
        if after is not None and (ma is None or ma.get("status") == "skip"):
            for r in peers:
                m = r["_metrics"].get(i)
                if m is not None and m.get("status") != "skip":
                    ma, after_from = m, Path(r["_file"]).name
                    break
        if mb is None and ma is None:
            continue
        ref = ma or mb or {}
        vb = mb.get("value") if mb else None
        va = ma.get("value") if ma else None
        status = ma.get("status", "skip") if ma else "skip"
        counts[status] = counts.get(status, 0) + 1
        delta = None
        if isinstance(vb, (int, float)) and isinstance(va, (int, float)) and vb:
            delta = round((va - vb) / vb * 100.0, 1)
        unit = ref.get("unit", "")
        row = {
            "id": i, "label": LABELS.get(i, ref.get("label", i)), "unit": unit, "target": ref.get("target"),
            "stat": ref.get("stat"), "before": vb, "after": va, "status": status, "delta_pct": delta,
            "note": (ma or {}).get("note", ""),
            "target_in_report": (ma or {}).get("target_in_report"),
            "after_from": after_from,
            "windows": ((ma or {}).get("extra") or {}).get("windows") if i == "frame_p50_ms" else None,
            "target_why": TARGET_WHY.get(i, ""),
            "trend": [{"date": r.get("date", ""), "commit": r["build"].get("commit", ""),
                       "value": r["_metrics"][i].get("value"), "representative": bool(r.get("representative"))}
                      for r in trend_src if i in r["_metrics"] and r["_metrics"][i].get("value") is not None],
        }
        rows.append(row)
        if status == "fail":
            failed.append({"id": i, "label": row["label"],
                           "reason": f"{ma.get('stat', 'value')} {fmt(va, unit)} > цель {fmt(row['target'], unit)}"})
    for s in (after or {}).get("stress", []) or []:
        if s.get("status") == "fail":
            failed.append({"id": s.get("id", ""), "label": f"стресс: {s.get('id', '')}", "reason": s.get("detail", "")})
    variants = []
    for v in (after or {}).get("variants", []) or []:
        variants.append({
            "id": v.get("id", ""), "label": v.get("label", ""), "env": v.get("env", {}), "exe": v.get("exe", ""),
            "mode": v.get("mode", ""), "build": v.get("build", ""), "opt": v.get("opt", ""),
            "phases": v.get("phases", {}),
            "metrics": {m["id"]: {"value": m.get("value"), "status": m.get("status"), "target": m.get("target"),
                                  "unit": m.get("unit", ""), "label": LABELS.get(m["id"], m.get("label", m["id"]))}
                        for m in v.get("metrics", [])},
        })
    return {
        "schema": "desk-perf-summary/1",
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "before": describe(before), "after": describe(after),
        "representative": bool(after and after.get("representative")),
        "counts": counts, "metrics": rows, "failed": failed,
        "stress": (after or {}).get("stress", []) or [],
        "variants": variants,
        "vault": ((after or {}).get("harness") or {}).get("vault", ""),
    }


def table(summary: dict) -> str:
    head = ("метрика", "до", "после", "цель", "статус")
    lines = [head]
    for m in summary["metrics"]:
        tgt = "—" if m["target"] is None else f"≤ {fmt(m['target'], m['unit'])}"
        aft = fmt(m["after"], m["unit"]) + (" ¹" if m.get("after_from") else "")
        lines.append((m["label"], fmt(m["before"], m["unit"]), aft, tgt, STATUS_RU.get(m["status"], m["status"])))
    w = [max(len(r[c]) for r in lines) for c in range(5)]
    out = []
    for n, r in enumerate(lines):
        out.append("  ".join(r[c].ljust(w[c]) if c == 0 else r[c].rjust(w[c]) if c < 4 else r[c] for c in range(5)))
        if n == 0:
            out.append("  ".join("─" * w[c] for c in range(5)))
    b, a = summary["before"], summary["after"]
    top = [f"до:    {b['file']} ({b['label'] or b['kind']}, {b['date']})" if b else "до:    — (нет отчёта Electron)",
           f"после: {a['file']} ({a['label'] or a['kind']}, {a['date']})" if a else "после: —"]
    if a and not a["representative"]:
        top.append("ВНИМАНИЕ: «после» снят не на ноутбуке владельца — цифры не репрезентативны.")
    vs = summary.get("variants") or []
    if vs:
        ids = [i for i in ORDER if any(i in v["metrics"] for v in vs)]
        rows = [("вариант «после»",) + tuple(v["label"] for v in vs)]
        for i in ids:
            cells = []
            for v in vs:
                m = v["metrics"].get(i)
                cells.append("—" if not m or m["value"] is None else f"{fmt(m['value'], m['unit'])} {STATUS_RU.get(m['status'], '')}")
            rows.append((LABELS.get(i, i),) + tuple(cells))
        w = [max(len(r[c]) for r in rows) for c in range(len(rows[0]))]
        out.append("")
        for n, r in enumerate(rows):
            out.append("  ".join(r[c].ljust(w[c]) if c == 0 else r[c].rjust(w[c]) for c in range(len(r))))
            if n == 0:
                out.append("  ".join("─" * x for x in w))
        for v in vs:
            env = ", ".join(f"{k}={x}" for k, x in (v["env"] or {}).items()) or "без переменных"
            out.append(f"  {v['label']}: {v['exe']}, {env}, протокол {v['mode']}")
        out += phase_table(vs, "warm") + phase_table(vs, "idle")
    for m in summary["metrics"]:
        if m.get("windows"):
            out.append("")
            out.append("  тренд p50 кадра по 10-с окнам стрима: " + " → ".join(f"{w['p50']:g}" for w in m["windows"]) + " мс")
    older = sorted({m["after_from"] for m in summary["metrics"] if m.get("after_from")})
    if older:
        out.append("")
        out.append("  ¹ этот прогон метрику не мерил; число из более раннего отчёта: " + ", ".join(older))
    changed = [m for m in summary["metrics"] if m.get("target_why")]
    if changed:
        out.append("")
        out += [f"  * {m['label']}: {m['target_why']}" for m in changed]
    tail = [f"провалы: {len(summary['failed'])}"] + [f"  · {f['label']}: {f['reason']}" for f in summary["failed"]]
    return "\n".join(top + [""] + out + [""] + tail)


KEY_PHASES = ["first_frame_presented", "day_drawn", "day_presented", "first_shot_uploaded", "idle_10s"]


def phase_table(vs: list[dict], run: str = "warm") -> list[str]:
    """Medians of the DESK_STARTUP_TRACE phases, «мс / Private МБ» per variant;
    run = "warm" (every variant) or "idle" (the ones with an idle run)."""
    vs = [v for v in vs if ((v.get("phases") or {}).get(run))]
    have = [p for p in KEY_PHASES if any(p in v["phases"][run] for v in vs)]
    if not have:
        return []
    title = {"warm": "фазы старта, тёплый: мс / Private МБ", "idle": "фазы в простое: мс / Private МБ"}[run]
    rows = [(title,) + tuple(have)]
    for v in vs:
        w = v["phases"][run]
        cells = []
        for p in have:
            x = w.get(p)
            cells.append("—" if not x or x.get("ms") is None else f"{x['ms']:.0f} / {fmt(x.get('private_mb'))}")
        rows.append((v["label"],) + tuple(cells))
    wd = [max(len(r[c]) for r in rows) for c in range(len(rows[0]))]
    out = [""]
    for n, r in enumerate(rows):
        out.append("  ".join(r[c].ljust(wd[c]) if c == 0 else r[c].rjust(wd[c]) for c in range(len(r))))
        if n == 0:
            out.append("  ".join("─" * x for x in wd))
    return out


# ---------------------------------------------------------------- the chat stream from a trace
# The same rule as Get-StreamWindows in bench.ps1. The stream starts at the
# desk's `autochat_sent` trace. It ends at `chat_turn_done` / `chat_turn_error`
# (if the build traces it) or at the first DESK_FRAME_STATS window with n=0
# after a window with frames: with --input-format stream-json, claude (and
# fake-claude) stay alive after `result`, waiting for the next user frame,
# so the process exit says nothing about the turn.

FRAME_RE = re.compile(r"frame_stats\b.*?(?<![A-Za-z_])n=(\d+).*?p50=([\d.]+)\D.*?p95=([\d.]+)\D.*?p99=([\d.]+)(?:\D.*?max=([\d.]+))?")
END_TRACES = ("chat_turn_done", "chat_turn_error")


def parse_utc(s: str) -> datetime:
    m = re.match(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(\.\d+)?", s)
    if not m:
        raise ValueError(f"not a UTC stamp: {s!r}")
    frac = (m.group(2) or ".0")[1:7].ljust(6, "0")
    return datetime.strptime(m.group(1), "%Y-%m-%dT%H:%M:%S").replace(microsecond=int(frac), tzinfo=timezone.utc)


def stream_windows(records: list[dict], run: str) -> dict:
    recs = sorted((r for r in records if r.get("run") == run and r.get("utc")), key=lambda r: r["utc"])
    start = done = None
    for r in recs:
        t = (r.get("trace") or {}).get("trace")
        if t == "autochat_sent" and start is None:
            start = parse_utc(r["utc"])
        if t in END_TRACES and done is None:
            done = parse_utc(r["utc"])
    out = {"run": run, "start": start.isoformat() if start else None, "windows": [], "end": None, "seconds": None}
    if start is None:
        return out
    for r in recs:
        raw = r.get("raw") or ""
        m = FRAME_RE.search(raw)
        if not m:
            continue
        at = parse_utc(r["utc"])
        if at <= start:
            continue
        n = int(m.group(1))
        if n == 0:
            if out["windows"]:
                out["end"] = "first frame_stats window with n=0"
                break
            continue
        w = {"t_s": round((at - start).total_seconds(), 1), "n": n, "p50": float(m.group(2)), "p95": float(m.group(3)),
             "p99": float(m.group(4)), "max": float(m.group(5)) if m.group(5) else None}
        out["windows"].append(w)
        if done is not None and at >= done:
            out["end"] = "chat_turn_done trace"
            break
    if out["windows"]:
        out["seconds"] = out["windows"][-1]["t_s"]
    return out


def growth_per_min(windows: list[dict]) -> float | None:
    """p50 slope in ms per minute of stream: least squares over the windows,
    weighted by their frame count (a 26-frame tail counts for less)."""
    if len(windows) < 2:
        return None
    xs = [w["t_s"] / 60.0 for w in windows]
    ys = [w["p50"] for w in windows]
    ws = [float(w["n"]) for w in windows]
    sw = sum(ws)
    mx = sum(w * x for w, x in zip(ws, xs)) / sw
    my = sum(w * y for w, y in zip(ws, ys)) / sw
    den = sum(w * (x - mx) ** 2 for w, x in zip(ws, xs))
    if den == 0:
        return None
    return round(sum(w * (x - mx) * (y - my) for w, x, y in zip(ws, xs, ys)) / den, 3)


def nearest_rank(xs: list[float], p: float) -> float | None:
    if not xs:
        return None
    v = sorted(xs)
    return v[min(max(math.ceil(p / 100 * len(v)), 1), len(v)) - 1]


def median(xs: list[float]) -> float | None:
    if not xs:
        return None
    v = sorted(xs)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2


def make_metric(mid: str, samples: list[float], method: str, note: str = "", extra: dict | None = None) -> dict:
    target, stat = TARGETS.get(mid, (None, "median"))
    value = {"median": median, "max": lambda x: nearest_rank(x, 100), "p95": lambda x: nearest_rank(x, 95)}[stat](samples)
    status = "info" if value is None or target is None else ("pass" if value <= target else "fail")
    m = {"id": mid, "label": LABELS.get(mid, mid), "unit": "ms/min" if "growth" in mid else "ms",
         "samples": [round(x, 3) for x in samples], "median": median(samples), "p95": nearest_rank(samples, 95),
         "min": min(samples) if samples else None, "max": max(samples) if samples else None,
         "target": target, "stat": stat, "value": None if value is None else round(value, 3), "status": status, "method": method}
    if note:
        m["note"] = note
    if extra:
        m["extra"] = extra
    return m


def stream_metrics(sw: dict, method: str) -> list[dict]:
    ws = sw["windows"]
    extra = {"windows": ws, "stream_s": sw["seconds"], "end": sw["end"],
             "p50_first": ws[0]["p50"] if ws else None, "p50_last": ws[-1]["p50"] if ws else None}
    out = [make_metric("frame_p50_ms", [w["p50"] for w in ws], method, extra=extra),
           make_metric("frame_p95_ms", [w["p95"] for w in ws], method),
           make_metric("frame_p99_ms", [w["p99"] for w in ws], method)]
    g = growth_per_min(ws)
    out.append(make_metric("frame_p50_growth_ms_per_min", [] if g is None else [g],
                           "weighted least-squares slope of the per-window p50 over the stream, ms per minute",
                           note="растёт, если каждый кадр делает работу, пропорциональную длине ответа (например, разбирает живой Markdown целиком)"))
    return out


def rederive_stream(report_path: Path, trace: Path | None) -> int:
    raw = json.loads(report_path.read_text(encoding="utf-8-sig"))
    r = unwrap(raw)
    if trace is None:
        rel = next((n.split(": ", 1)[1] for n in r.get("notes", []) if n.startswith("startup traces: ")), None)
        if rel is None:
            print("no 'startup traces:' note in the report; pass --trace", file=sys.stderr)
            return 1
        trace = REPO / rel
    files = [trace] if trace.is_file() else sorted(trace.glob("*.jsonl"))
    records = [json.loads(l) for f in files for l in f.read_text(encoding="utf-8-sig").splitlines() if l.strip()]
    sw = stream_windows(records, "stream 10k")
    if not sw["windows"]:
        print("no stream windows found in the trace (run 'stream 10k')", file=sys.stderr)
        return 1
    src = ", ".join(str(f.relative_to(REPO)) if f.is_relative_to(REPO) else f.name for f in files)
    method = (f"DESK_FRAME_STATS=1 per-10-s windows from autochat_sent to the turn's end ({sw['end']}); "
              f"p50: median of the windows, p95/p99: worst window; re-derived by report.py from {src}")
    new = stream_metrics(sw, method)
    ids = {m["id"] for m in new}
    r["metrics"] = [m for m in r["metrics"] if m["id"] not in ids] + new
    for st in r.get("stress", []):
        if st.get("id") == "chat_stream_10k":
            alive = "desk alive True" in st.get("detail", "")
            resp = "responding True" in st.get("detail", "")
            ok = alive and resp and sw["end"] is not None
            st["status"] = "pass" if ok else "fail"
            st["detail"] = (f"stream {sw['seconds']:.0f} s by the desk's frame windows ({sw['end']}), "
                            f"desk alive {alive}, responding {resp}; re-derived by report.py "
                            f"(bench.ps1 v4 waited for fake-claude to exit, which it never does in stream-json mode)")
    r["notes"] = [n for n in r.get("notes", []) if "re-derived on" not in n]
    r["notes"].append(
        f"frame_* and chat_stream_10k re-derived on {datetime.now(timezone.utc):%Y-%m-%d} by report.py --rederive-stream "
        f"from {src}: bench.ps1 v4 ended the stream at fake-claude's exit, which never comes in stream-json mode")
    report_path.write_text(json.dumps(r, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for m in new:
        print(f"{m['id']:<30} {fmt(m['value'], m['unit']):>12}  {STATUS_RU.get(m['status'], m['status'])}")
    print("p50 по окнам: " + " → ".join(f"{w['p50']:g}" for w in sw["windows"]) + " мс")
    print(f"wrote {report_path}")
    return 0


def check_catalogs() -> list[str]:
    """The three catalogs (report.rs, bench.ps1, this file) must agree."""
    errs = []
    rs = (REPO / "rust/crates/desk-bench/src/report.rs").read_text(encoding="utf-8")
    cat = rs[rs.index("pub const METRICS"):rs.index("pub fn def")]
    rust = {i: (float(t) if t else None, st.lower()) for i, _, t, st in re.findall(
        r'id: "([a-z0-9_]+)",\s*label: "[^"]*",\s*unit: "[^"]*",\s*target: (Some\(([0-9.]+)\)|None),\s*stat: Stat::(\w+)', cat)}
    ps = (REPO / "scripts/rust/bench.ps1").read_text(encoding="utf-8-sig")
    psc = {i: (None if t == "$null" else float(t), st) for i, t, st in re.findall(
        r"'([a-z0-9_]+)'\s*=\s*@\('[^']*',\s*([0-9.]+|\$null),\s*'(\w+)'", ps)}
    for i, v in TARGETS.items():
        if rust.get(i) != v:
            errs.append(f"report.rs {i}: {rust.get(i)} != report.py {v}")
        if i in psc and psc[i] != v:
            errs.append(f"bench.ps1 {i}: {psc[i]} != report.py {v}")
    errs += [f"report.rs has {i}, report.py does not" for i in rust if i not in TARGETS]
    errs += [f"bench.ps1 has {i}, report.py does not" for i in psc if i not in TARGETS]
    return errs


def self_test() -> int:
    def rep(kind_, rep_, created, vals):
        return {"schema": "desk-perf/1", "representative": rep_, "label": kind_, "created_at": created, "date": created[:10],
                "machine": {}, "build": {"kind": kind_, "version": "x", "commit": created[-3:]},
                "metrics": [{"id": k, "unit": "ms", "value": v, "target": 150.0, "stat": "median",
                             "status": "pass" if v <= 150 else "fail", "samples": {"value": [v], "Count": 1}} for k, v in vals.items()],
                "stress": [{"id": "x", "status": "fail", "detail": "boom"}],
                "variants": [{"id": "v1", "label": "base", "env": {"DESK_OPT": "base"}, "exe": "d.exe", "mode": "data-ready",
                              "phases": {"warm": {"day_presented": {"n": 5, "ms": 240.0, "private_mb": 91.0}}},
                              "metrics": [{"id": "data_ready_warm_ms", "unit": "ms", "value": 250.0, "target": 300.0, "status": "pass"}]},
                             {"id": "v2", "label": "all", "env": {"DESK_OPT": "all"}, "exe": "d.exe", "mode": "data-ready",
                              "metrics": [{"id": "data_ready_warm_ms", "unit": "ms", "value": 180.0, "target": 300.0, "status": "pass"}]}]}
    with tempfile.TemporaryDirectory() as d:
        p = Path(d)
        (p / "e.json").write_text(json.dumps(rep("electron", True, "2026-11-25T10:00:00Z", {"warm_start_ms": 900.0})), encoding="utf-8")
        (p / "r1.json").write_text(json.dumps(rep("rust", False, "2026-11-25T11:00:00Z", {"warm_start_ms": 200.0})), encoding="utf-8-sig")
        (p / "r2.json").write_text(json.dumps(rep("rust", True, "2026-11-24T11:00:00Z", {"warm_start_ms": 120.0})), encoding="utf-8")
        reps = collect([d])
        s = summarize(reps, newest(reps, ("electron",)), newest(reps, ("rust", "synthetic")))
        ok = True
        m = s["metrics"][0]
        checks = [
            (s["after"]["file"] == "r2.json", "representative beats newer"),
            (m["before"] == 900.0 and m["after"] == 120.0, "values"),
            (m["delta_pct"] == -86.7, "delta"),
            (len(m["trend"]) == 2 and m["trend"][0]["value"] == 120.0, "trend ordered"),
            (reps[1]["metrics"][0]["samples"] == [200.0], "ps 5.1 array unwrapped"),
            (any(f["reason"] == "boom" for f in s["failed"]), "stress failure listed"),
            ("Тёплый старт" in table(s), "table label"),
            ([v["metrics"]["data_ready_warm_ms"]["value"] for v in s["variants"]] == [250.0, 180.0], "variant columns"),
            ("Данные дня на экране, тёплый старт" in table(s), "variant table"),
            ("240 / 91" in table(s), "phase table"),
            (rejudge({"id": "warm_start_ms", "value": 189.0, "target": 150.0, "status": "fail"})["status"] == "pass", "re-judged by today's target"),
            (rejudge({"id": "idle_private_mb", "value": 115.0, "target": 80.0, "status": "fail"})["target_in_report"] == 80.0, "old target kept"),
            (rejudge({"id": "cold_start_ms", "value": None, "target": 300.0, "status": "skip"})["status"] == "skip", "skip stays skip"),
            (check_catalogs() == [], "catalogs agree: " + "; ".join(check_catalogs())),
        ]
        # The 27.11 owner trace, shortened: the stream ends at the first n=0 window.
        recs = [{"run": "stream 10k", "utc": "2026-11-27T09:01:29.0050000Z", "trace": {"trace": "autochat_sent", "ms": 252.4}}]
        for t, line in [("09:01:38.758", "n=627 p50=0.63ms p95=1.04ms p99=1.28ms max=35.04ms"),
                        ("09:01:48.758", "n=656 p50=1.47ms p95=2.08ms p99=3.25ms max=7.53ms"),
                        ("09:01:58.759", "n=644 p50=2.48ms p95=2.98ms p99=7.44ms max=17.86ms"),
                        ("09:02:08.759", "n=636 p50=3.41ms p95=3.88ms p99=4.70ms max=11.45ms"),
                        ("09:02:18.760", "n=647 p50=4.18ms p95=4.80ms p99=5.14ms max=7.16ms"),
                        ("09:02:28.761", "n=26 p50=4.61ms p95=5.14ms p99=5.20ms max=5.20ms"),
                        ("09:02:38.761", "n=0 p50=0.00ms p95=0.00ms p99=0.00ms max=0.00ms"),
                        ("09:02:48.762", "n=0 p50=0.00ms p95=0.00ms p99=0.00ms max=0.00ms")]:
            recs.append({"run": "stream 10k", "utc": f"2026-11-27T{t}0000Z", "raw": "frame_stats " + line})
        sw = stream_windows(recs, "stream 10k")
        sm = {m["id"]: m for m in stream_metrics(sw, "t")}
        checks += [
            (len(sw["windows"]) == 6 and sw["end"].startswith("first frame_stats window with n=0"), "stream ends at the first n=0 window"),
            (sm["frame_p50_ms"]["value"] == 2.945 and sm["frame_p95_ms"]["value"] == 5.14 and sm["frame_p99_ms"]["value"] == 7.44, "stream frame numbers"),
            (5.0 < sm["frame_p50_growth_ms_per_min"]["value"] < 6.0, "p50 trend ms/min"),
            (stream_windows(recs + [{"run": "stream 10k", "utc": "2026-11-27T09:01:50.0000000Z", "trace": {"trace": "chat_turn_done"}}], "stream 10k")["end"] == "chat_turn_done trace", "chat_turn_done wins"),
        ]
        for c, name in checks:
            print(("ok   " if c else "FAIL ") + name)
            ok &= c
    return 0 if ok else 1


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("reports", nargs="*", help="report files or folders (default: docs/rust/perf)")
    ap.add_argument("--before", help="the «до» report (default: newest Electron)")
    ap.add_argument("--after", help="the «после» report (default: newest Rust, representative first)")
    ap.add_argument("--summary", default=str(DEFAULT_SUMMARY), help="where to write summary.json ('-' = do not write)")
    ap.add_argument("--rederive-stream", metavar="REPORT", help="recompute frame_* and chat_stream_10k of REPORT from its startup trace")
    ap.add_argument("--trace", help="trace .jsonl file or folder for --rederive-stream (default: the report's 'startup traces' note)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args(argv)
    if a.self_test:
        return self_test()
    if a.rederive_stream:
        return rederive_stream(Path(a.rederive_stream), Path(a.trace) if a.trace else None)
    paths = a.reports or [str(REPO / "docs" / "rust" / "perf")]
    reports = collect(paths)
    before = load(Path(a.before)) if a.before else None
    after = load(Path(a.after)) if a.after else None
    if a.reports and len(a.reports) >= 2 and all(Path(p).is_file() for p in a.reports) and not (a.before or a.after):
        before, after = reports[0], reports[-1]
    before = before or newest(reports, ("electron",))
    after = after or newest(reports, ("rust",)) or newest(reports, ("synthetic",))
    if after is before:
        before = None
    if before is None and after is None:
        print("no reports found", file=sys.stderr)
        return 1
    s = summarize(reports, before, after)
    print(table(s))
    if a.summary != "-":
        out = Path(a.summary)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(s, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
        print(f"\nwrote {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
