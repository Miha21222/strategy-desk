#!/usr/bin/env python3
"""Compare perf reports: «метрика · до · после · цель · статус».

    python scripts/rust/report.py docs/rust/perf/                # every report there
    python scripts/rust/report.py base.json new.json            # first = до, last = после
    python scripts/rust/report.py --before E.json --after R.json [--summary out.json]
    python scripts/rust/report.py --self-test

Reads reports of schema desk-perf/1 (docs/rust/PERF.md), prints the table and
writes docs/rust/perf/summary.json for the dashboard: before/after per
metric, the trend over every Rust report, and what failed with the reason.

«До» is the newest Electron report unless --before says otherwise; «после»
is the newest Rust report, preferring representative ones (the owner's
laptop) over the rest. Standard library only, Python 3.8+.
"""
from __future__ import annotations

import argparse
import json
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
    for i in ids:
        mb = before["_metrics"].get(i) if before else None
        ma = after["_metrics"].get(i) if after else None
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
            "mode": v.get("mode", ""),
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
        lines.append((m["label"], fmt(m["before"], m["unit"]), fmt(m["after"], m["unit"]), tgt, STATUS_RU.get(m["status"], m["status"])))
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
    tail = [f"провалы: {len(summary['failed'])}"] + [f"  · {f['label']}: {f['reason']}" for f in summary["failed"]]
    return "\n".join(top + [""] + out + [""] + tail)


def self_test() -> int:
    def rep(kind_, rep_, created, vals):
        return {"schema": "desk-perf/1", "representative": rep_, "label": kind_, "created_at": created, "date": created[:10],
                "machine": {}, "build": {"kind": kind_, "version": "x", "commit": created[-3:]},
                "metrics": [{"id": k, "unit": "ms", "value": v, "target": 150.0, "stat": "median",
                             "status": "pass" if v <= 150 else "fail", "samples": {"value": [v], "Count": 1}} for k, v in vals.items()],
                "stress": [{"id": "x", "status": "fail", "detail": "boom"}],
                "variants": [{"id": "v1", "label": "base", "env": {"DESK_OPT": "base"}, "exe": "d.exe", "mode": "data-ready",
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
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args(argv)
    if a.self_test:
        return self_test()
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
