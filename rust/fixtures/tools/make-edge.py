"""Write the hand-made edge-case vaults under rust/fixtures/edge/<case>/vault.

    python3 rust/fixtures/tools/make-edge.py [<edge_root>]

Each case is a tiny vault that breaks one rule of the happy path (a truncated
file, a file caught mid-write, unknown fields, wrong types, an old v1 scale…)
plus `case.json`: which dates to load, how the Rust side is compared with the
TS outcome, and why the case exists. The expected outcome is NOT written here:
`dump-edge.ts` runs the old app's `loadDay()` over these vaults and stores what
it returns (rule 4 of TEAM.md: the old app is the reference).

Content is written byte-for-byte on purpose (BOMs, CRLF, half files), so the
files are spelled out here rather than produced by json.dumps alone.
All data is synthetic.
"""
from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

ROOT = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).resolve().parents[1] / "edge")
BOM = "﻿"
D = "2026-10-08"  # a Thursday
D2 = "2026-10-09"  # a Friday


def j(x) -> str:
    return json.dumps(x, ensure_ascii=False, indent=1)


def factor(fid: str, side: str, share: float, strength: float, group: str = "контекст дня", **extra) -> dict:
    return {"id": fid, "name": fid, "group": group, "share_cfg": share, "on": True, "side": side, "strength": strength,
            "why": "синтетика", "share": share, "points": round(share * strength, 1) if side else 0.0, **extra}


def scale(region: str, date: str, needle: int = -30, **extra) -> dict:
    f = [factor("bias_a", "SHORT", 40.0, 1.0), factor("room", "LONG", 10.0, 1.0, "окно открытия"), factor("m_vol", "", 50.0, 0.0, "макро")]
    out = {"region": region, "date": date, "asof": f"{date} 09:35", "sym": "FDAX" if region == "eu" else "ES", "version": "v3.1",
           "factors": f, "result": {"long": 10.0, "short": 40.0, "undecided": 50.0, "max": 100, "needle": needle, "zone": "шорт",
                                    "side": "SHORT", "groups": {"контекст дня": {"share": 40.0, "long": 0.0, "short": 40.0, "needle": -40}}},
           "light": {"color": "green", "label": "🟢 торгуем", "red": [], "yellow": [], "risk_pct": 1},
           "levels": {"price": 25000, "adr": 280}, "discussion": [], "history": []}
    out.update(extra)
    return out


VERDICT = {"verdict": "SHORT", "confidence": "medium", "summary": "[синтетика] вердикт", "plan": ["строка плана"]}
CFG = {"version": "v3.1", "zones": {"neutral": 15, "strong": 40},
       "regions": {"eu": {"factors": [{"id": "bias_a", "name": "Точка A"}, {"id": "room", "name": "Место до цели"}]}}}
CSV = "date,region,factor,group,share,side,strength,dir,hit\n2026-10-07,eu,bias_a,g,40,SHORT,1,SHORT,1\n2026-10-07,eu,room,g,10,LONG,1,SHORT,0\n"


class Vault:
    def __init__(self, name: str, dates: list[str], compare: str, why: str, **meta):
        self.dir = ROOT / name
        if self.dir.exists():
            shutil.rmtree(self.dir)
        self.vault = self.dir / "vault"
        self.vault.mkdir(parents=True)
        (self.dir / "case.json").write_text(j({"dates": dates, "compare": compare, "why": why, **meta}) + "\n", encoding="utf-8")

    def put(self, rel: str, text: str | bytes) -> None:
        p = self.vault / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(text, bytes):
            p.write_bytes(text)
        else:
            p.write_bytes(text.encode("utf-8"))

    def state(self, name: str, text: str | bytes) -> None:
        self.put(f"indices-kb/state/{name}", text)

    def basics(self, cfg: bool = True, journal: bool = True) -> None:
        if cfg:
            self.put("indices-kb/config/scale.json", j(CFG))
        if journal:
            self.put("indices-kb/journal/lessons.md", "# Уроки\n\n- 2026-10-07: [синтетика] старый\n- 2026-10-08: [синтетика] новый\n")
            self.put("indices-kb/journal/scale_factor_scores.csv", CSV)


# ---------------------------------------------------------------- truncated JSON
v = Vault("truncated-json", [D], "strict", "A file cut off mid-write (crash, full disk, sync conflict) is read as absent; the rest of the day still loads.")
v.basics(journal=False)
full = j(scale("eu", D))
v.state(f"scale_{D}_eu.json", full[: len(full) // 2])
v.state(f"scale_{D}_us.json", j(scale("us", D, needle=12)))
v.state(f"macro_{D}_us_verdict.json", j(VERDICT)[:25])
v.state(f"macro_{D}_eu_verdict.json", j(VERDICT))
v.state(f"shots_{D}_eu.json", '{"ok": tru')
v.state(f"remember_{D}_eu.json", '{"items": [')
v.state(f"drift_{D}_eu.json", '{"date": "2026-10-08", "signals": [{"kind": "phase", "text": "фаза')
v.state(f"review_{D}.json", '{"date": "2026-10-08", "regions": {"eu": {"facts": {"dir": "SHO')
v.state("ideas.json", '{"asof": "2026-10-08 20:10", "items": [{"id": "a", "te')
v.put("indices-kb/journal/lessons.md", "# Уроки\n\n- 2026-10-07: [синтетика] целый\n- 2026-10-08: [синтетика] обрезан на сере")
# The last row stops after three cells: parseCsv fills the missing ones with ''.
v.put("indices-kb/journal/scale_factor_scores.csv", CSV + "2026-10-08,eu,bias_a,g,40,SHORT,1,SHORT,1\n2026-10-08,eu,room")

# ---------------------------------------------------------------- a file being written
v = Vault("being-written", [D, D2], "strict",
          "What the watcher can see during a write: a scale without `result` (half-written by an old writer), an empty file, and the temp files of the temp+rename write. "
          "TS quirk: `scale_<D>_<r>.json.<pid>.tmp` matches dateOfStateFile, so a date that has ONLY a temp file is still listed as a day (2026-10-09 here).")
v.basics()
s = scale("eu", D)
del s["result"]
v.state(f"scale_{D}_eu.json", j(s))
v.state(f"scale_{D}_us.json", j(scale("us", D, needle=20)))
v.state(f"scale_{D}_us.json.4242.tmp", j(scale("us", D, needle=99)))
v.state(f"macro_{D}_us_verdict.json", "")
v.state(f"review_{D}_agent.json", "")
v.state(f"human_read_{D}_eu.json.31337.tmp", j({"date": D, "region": "eu", "side": "LONG", "at": f"{D} 09:31", "before_scale": True}))
v.state(f"scale_{D2}_eu.json.77.tmp", j(scale("eu", D2)))

# ---------------------------------------------------------------- unknown fields
v = Vault("unknown-fields", [D], "strict",
          "A newer skill adds fields. Nothing may fail. TS quirk: files the desk passes through raw (scale, verdict, review, review_agent, collect.calendar, collect.regime) keep the unknown fields in the day JSON; "
          "the normalized ones (human_read, day_answers, remember, drift, week, ideas, shots) drop them. Unknown state files (regime_*, foo_*) are not read at all.")
v.basics()
s = scale("eu", D, version="v4.0", future_block={"a": 1, "b": [1, 2]}, plan={"side": "SHORT", "summary": "план", "new_plan_field": True})
s["factors"][0]["confidence"] = 0.8
s["result"]["entropy"] = 0.42
v.state(f"scale_{D}_eu.json", j(s))
v.state(f"macro_{D}_eu_verdict.json", j({**VERDICT, "model": "x", "ib": {"side": "SHORT", "reach": "IB1"}}))
v.state(f"macro_{D}_eu.json", j({"asof_eet": f"{D} 09:31", "open_eet": "10:00", "new_top": 1,
                                 "calendar": {"status": "TRADE", "events": [{"title": "Ifo", "impact": "Medium", "time_eet": "11:00", "extra": "x"}], "future": True},
                                 "markets": [{"label": "FDAX", "chg_pct": 0.3, "main": True, "kind": "index", "last": 25000, "extra": "drop me"}],
                                 "headlines": {"count": 10, "relevant": 4, "tone_sum": -1, "tags": {"ставки": 3}, "feeds": ["a"], "items": [{"t": "x"}]},
                                 "tilt": {"score": -1, "hint": "шорт", "more": 1}, "regime": {"phase": "пила", "new": 2},
                                 "prev_day_check": [{"x": 1}], "verdict": {"verdict": "LONG"}}))
v.state(f"human_read_{D}_eu.json", j({"date": D, "region": "eu", "side": "SHORT", "at": f"{D} 09:31", "before_scale": True, "mood": "calm", "confidence": 3}))
v.state(f"day_answers_{D}.json", j({"date": D, "at": f"{D} 20:05", "regions": {"eu": {"trade": "short", "by_rule": "yes", "r": 1.5}, "asia": {"trade": "long"}},
                                    "note": "заметка", "extra": [1]}))
v.state(f"remember_{D}_eu.json", j({"date": D, "region": "eu", "asof": "x", "items": [{"kind": "lesson", "text": "урок", "why": "", "ref": "", "tags": [], "score": 9}],
                                    "conditions": {"tags": ["eu"], "new_cond": "x"}, "similar": None, "lesson_candidates": 1, "enough_history": True, "new": 1}))
v.state(f"drift_{D}_eu.json", j({"date": D, "region": "eu", "days": 20, "enough_data": True, "signals": [{"kind": "new_kind", "text": "новый вид сигнала", "severity": "critical", "data": {"k": 1}, "extra": 1}]}))
v.state(f"shots_{D}_eu.json", j({"date": D, "region": "eu", "asof": "x", "ok": True, "error": None, "renderer": "v2",
                                 "shots": [{"name": "h4", "tf": "H4", "title": "H4", "path": "indices-kb/shots/x/h4.png", "verify_ok": None, "dpi": 2,
                                            "legend": [{"key": "k", "label": "линия", "kind": "new_kind", "color": "rgb(1,2,3)", "style": "dotted", "width": 2}]}]}))
v.state(f"review_{D}.json", j({"date": D, "asof": "x", "regions": {"eu": {"facts": {"dir": "SHORT", "day_move": -0.6}, "score": {"needle_hit": "1"}, "ib": {"side": "SHORT"}}},
                               "trader": ["наблюдение"], "new_section": {"a": 1}}))
v.state(f"review_{D}_agent.json", j({"summary": "итог", "lessons": ["урок"], "extra": True}))
v.state(f"regime_{D}.json", j({"phase": "пила"}))
v.state(f"foo_{D}.json", j({"x": 1}))
v.state("ideas.json", j({"asof": "x", "items": [{"id": "a", "date": D, "text": "идея", "side": "", "region": "eu", "source": "chat", "confirmed": 1, "refuted": 0,
                                                 "status": "open", "backtest": {"days": 1, "hit": 1, "rate": 1, "p": 0.1}, "last_check": D, "owner_note": "x"}], "schema": 2}))
v.state(f"week_{D}.json", j({"week_start": "2026-10-05", "week_end": D, "days": 4, "enough_data": False, "lines": ["строка"], "stats": {"a": 1}, "trader": [],
                             "share_proposal": None, "ib_stats": {"hit": 7}}))

# ---------------------------------------------------------------- empty state dir / no state dir
v = Vault("empty-state-dir", [D], "strict", "The state folder exists but is empty (fresh vault before the first run); no journal, no config: defaults everywhere.")
# git keeps no empty folder: a dotfile the reader ignores (not scale_/macro_/review_) holds it.
v.state(".gitkeep", "")
v.put("README.md", "# [синтетика] пустой vault\n")

v = Vault("no-state-dir", [D], "strict", "A vault folder without indices-kb/state (wrong folder picked): the index says no-state-dir, loadDay still answers an empty day.")
v.put("README.md", "# [синтетика] не тот vault\n")

# ---------------------------------------------------------------- old v1 scale
v = Vault("old-v1-scale", [D, D2], "strict",
          "Scales written by scale.py v1 (weights 1–3, points out of max, no share/undecided/group shares) and a v3 file missing `undecided` — normalizeScale derives the percentages.")
v.basics()
v1 = {"region": "eu", "date": D, "asof": "", "sym": "FDAX", "version": "v1",
      "factors": [{"id": "a", "name": "A", "group": "техника", "weight": 3, "side": "LONG", "strength": 1, "points": 3},
                  {"id": "b", "name": "B", "group": "сессия", "weight": 2, "side": "SHORT", "strength": 0.5, "points": 1},
                  {"id": "c", "name": "C", "group": "сессия", "weight": 1, "side": "", "strength": 0, "points": 0},
                  {"id": "d", "name": "D", "group": "сессия", "weight": 1, "side": "SHORT", "strength": 1, "points": 1, "on": False}],
      "result": {"long": 3, "short": 1, "max": 7, "needle": 50, "zone": "сильный лонг", "side": "LONG",
                 "groups": {"техника": {"long": 3, "short": 0, "needle": 100}, "сессия": {"long": 0, "short": 1, "needle": -100}}},
      "after": {"long": 3, "short": 2, "max": 7, "needle": 20, "zone": "лонг", "side": "LONG"},
      "light": {"color": "red", "label": ""}}
v.state(f"scale_{D}_eu.json", j(v1))
v1us = {"region": "us", "date": D, "asof": "", "sym": "ES", "version": "v1",
        "factors": [{"id": "a", "name": "A", "group": "техника", "weight": 2, "side": "LONG", "strength": 0.7, "points": 1.4},
                    {"id": "b", "name": "B", "group": "техника", "weight": 1, "side": "SHORT", "strength": 1, "points": 1}],
        "result": {"long": 1.4, "short": 1, "needle": 17, "zone": "лонг", "side": "LONG"}, "light": {"color": "green", "label": ""}}
v.state(f"scale_{D}_us.json", j(v1us))  # no max: the total of weights stands in
mixed = {"region": "eu", "date": D2, "asof": "", "sym": "FDAX", "version": "v2",
         "factors": [{"id": "a", "name": "A", "group": "g", "weight": 1, "side": "LONG", "strength": 1, "points": 1, "share": 50},
                     {"id": "b", "name": "B", "group": "g", "weight": 1, "side": "SHORT", "strength": 1, "points": 1}],
         "result": {"long": 1, "short": 1, "max": 2, "needle": 0, "zone": "нейтрально", "side": None}, "light": {"color": "yellow", "label": ""}}
v.state(f"scale_{D2}_eu.json", j(mixed))  # one factor lacks share → the whole file is treated as legacy
v3 = scale("us", D2)
del v3["result"]["undecided"]
v3["result"]["long"] = 10.04
v3["result"]["short"] = 40.06
v3["result"]["groups"] = {"контекст дня": {"long": 0.04, "short": 40.06, "needle": -40}}  # group share missing → derived from weights (none: 0)
v.state(f"scale_{D2}_us.json", j(v3))

# ---------------------------------------------------------------- wrong types
v = Vault("wrong-types", [D, D2], "strict",
          "Fields with the wrong type. Normalized files fall back field by field; raw files pass the wrong value through (TS quirk). "
          "Config zones written as strings stay strings in `zones` (TS `??` keeps them) — see TEST_PLAN «Известные причуды».")
v.put("indices-kb/config/scale.json", j({"zones": {"neutral": "20", "strong": None}, "regions": {"eu": {"factors": [{"id": "bias_a", "name": 7}, {"id": "room"}]}, "us": {"factors": "x"}}}))
v.put("indices-kb/journal/lessons.md", BOM + "- 2026-10-01: [синтетика] первая строка с BOM\r\n-без пробела не пункт\r\n* 2026-10-02: звёздочка\r\n  - 2026-10-03: с отступом\r\n")
v.put("indices-kb/journal/scale_factor_scores.csv", BOM + "date,region,factor,group,share,side,strength,dir,hit\r\n"
      "2026-10-07,EU,bias_a,g,1,LONG,1,LONG,1\r\n2026-10-07,eu,bias_a,g,1,LONG,1,LONG,1.0\r\n2026-10-07,eu,room,g,1,LONG,1,LONG,1\r\n"
      "2026-10-07,eu,Room,g,1,LONG,1,SHORT,0\r\n2026-10-07,eu,m_vol,g,1,LONG,1,LONG,1\r\n2026-10-07,eu,m15_flow,g,1,LONG,1,LONG,1\r\n"
      "2026-10-07,eu,\"q,uoted\",g,1,LONG,1,LONG,0\r\nbad-date,eu,room,g,1,LONG,1,LONG,1\r\n")
s = scale("eu", D)
s["factors"] = "not a list"
s["result"]["undecided"] = "50"  # a string: not a number, so the non-legacy path recomputes
v.state(f"scale_{D}_eu.json", j(s))
v.state(f"scale_{D}_us.json", '{"region": "us", "date": "2026-10-08", "result": {"needle": NaN, "long": 1, "short": 1, "undecided": 98}, "factors": []}')  # Python json writes NaN
v.state(f"macro_{D}_eu_verdict.json", '"LONG"')  # a JSON string, not an object
v.state(f"macro_{D}_eu.json", j({"asof_eet": 931, "markets": {"FDAX": 0.3}, "headlines": "много", "calendar": "нет данных", "tilt": 5, "prev_day_check": {"x": 1}}))
v.state(f"macro_{D}_us.json", j({"markets": [{"symbol": "ES", "chg_pct": "0.4", "main": "yes"}, {"label": 5}], "headlines": 0, "verdict": ""}))
v.state(f"human_read_{D}_eu.json", j({"date": 20261008, "region": "xx", "side": "SHORT", "at": 931, "before_scale": "no"}))
v.state(f"human_read_{D}_us.json", j({"side": "UP"}))
v.state(f"day_answers_{D}.json", j({"regions": [{"trade": "long"}], "note": "только заметка", "at": None}))
v.state(f"remember_{D}_eu.json", j({"items": "урок"}))
v.state(f"remember_{D}_us.json", j({"date": D, "items": [{"kind": "lesson", "text": "  с пробелами  ", "why": 5, "ref": None, "tags": ["a", 1, " ", " b "]}],
                                    "conditions": [], "similar": {"date": 20261001}, "lesson_candidates": 2.6, "enough_history": "yes"}))
v.state(f"drift_{D}_eu.json", j({"signals": {"kind": "phase"}}))
v.state(f"drift_{D}_us.json", j({"days": -5, "signals": [{"text": "без вида", "severity": "WARN", "data": [1]}]}))
v.state(f"shots_{D}_eu.json", j({"ok": "yes", "shots": [{"name": 5, "path": 7}, {"path": "indices-kb/shots/a/b.png", "legend": [{"label": 1}, {"label": "ок", "kind": "", "color": 5}]}], "error": 0}))
v.state(f"shots_{D}_us.json", j({"shots": [{"path": "indices-kb/shots/a/c.png"}], "error": ""}))  # no `ok`: taken at its pictures
v.state(f"week_{D2}.json", j({"lines": "пять строк", "trader": ["x"]}))
v.state(f"review_{D2}.json", j({"date": D2, "regions": [], "trader": "одной строкой"}))
v.state(f"scale_{D2}_eu.json", j(scale("eu", D2, result=[])))  # an array result: truthy, so it IS normalized (TS quirk)
v.state("ideas.json", j({"asof": 5, "items": [{"text": "без статуса"}, {"text": "ок", "status": "open", "confirmed": -3, "refuted": 2.5, "backtest": [], "date": 20261001}, "x", None]}))

# ---------------------------------------------------------------- BOM + CRLF
v = Vault("bom-crlf", [D], "strict", "Files written by PowerShell / Windows tools: UTF-8 BOM and CRLF everywhere. JSON readers strip the BOM; the CSV reader strips it; lessons.md is not stripped but JS trim() eats U+FEFF (Rust's trim() does not).")
for name, data in [(f"scale_{D}_eu.json", scale("eu", D)), (f"macro_{D}_eu_verdict.json", VERDICT),
                   (f"human_read_{D}_eu.json", {"date": D, "region": "eu", "side": "LONG", "at": f"{D} 09:31", "before_scale": False}),
                   ("ideas.json", {"asof": "x", "items": [{"id": "a", "date": D, "text": "идея", "status": "proposed"}]})]:
    v.state(name, BOM + j(data).replace("\n", "\r\n"))
v.put("indices-kb/config/scale.json", BOM + j(CFG).replace("\n", "\r\n"))
v.put("indices-kb/journal/lessons.md", BOM + "- 2026-10-08: [синтетика] урок сразу после BOM\r\n- 2026-10-09: второй\r\n")
v.put("indices-kb/journal/scale_factor_scores.csv", BOM + CSV.replace("\n", "\r\n"))

# ---------------------------------------------------------------- verdict fallback and notes
v = Vault("verdict-fallback", [D, D2], "strict",
          "The verdict file wins over the copy inside the collect file; a verdict file holding `null` falls back to the copy; an empty-string copy is no verdict. Notes are found by exact file name.")
v.basics()
v.state(f"macro_{D}_eu.json", j({"asof_eet": "09:31", "verdict": {"verdict": "LONG", "summary": "копия в сборе"}}))
v.state(f"macro_{D}_us.json", j({"asof_eet": "16:01", "verdict": {"verdict": "NEUTRAL"}}))
v.state(f"macro_{D}_us_verdict.json", "null")
v.state(f"macro_{D2}_eu.json", j({"verdict": ""}))
v.state(f"macro_{D2}_eu_verdict.json", j({"verdict": "NO_TRADE"}))
v.put(f"Макро-обзоры/{D} США.md", "# [синтетика]\n")
v.put(f"Макро-обзоры/{D} Итог дня.md", "# [синтетика]\n")
v.put(f"Макро-обзоры/{D2} европа.md", "# [синтетика] не тот регистр: не находится на Linux, находится на Windows\n")

# ---------------------------------------------------------------- the old app crashes here
v = Vault("ts-crash", [D, D2], "no-crash",
          "Inputs on which the old loadDay() THROWS (the whole day fails to load): a null inside collect.markets, a null inside config regions.*.factors. "
          "Rust must not reproduce the crash: the day loads; the expected TS outcome is recorded as {\"$error\": …} only for the record.",
          decision="Proposed Rust behaviour: skip the non-object entry, keep the rest. Needs the lead's OK (rule 4 exception).")
v.state(f"scale_{D}_eu.json", j(scale("eu", D)))
v.state(f"macro_{D}_eu.json", j({"markets": [None, {"label": "FDAX", "chg_pct": 0.1}]}))
v.state(f"scale_{D2}_eu.json", j(scale("eu", D2)))
v.put("indices-kb/config/scale.json", j({"regions": {"eu": {"factors": [None, {"id": "bias_a", "name": "Точка A"}]}}}))

print("edge cases:", sorted(p.name for p in ROOT.iterdir() if p.is_dir()))
