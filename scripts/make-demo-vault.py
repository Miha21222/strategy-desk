"""Build a demo vault for visual testing of the «Обзор дня» tab.

    python scripts/make-demo-vault.py <out_dir> [<real_scale.json>]

Writes <out_dir>/indices-kb/{state,journal,config} and <out_dir>/Макро-обзоры with
nine invented September days, each covering a different case of the interface
(strong/neutral/red days, discussions that move the needle, half-written reviews,
an old v1 scale, very long texts, chart screenshots in every state). Every file follows the contract of the real
`macro-review` skill (scale.py v3.1, verdict, review, review_agent, lessons,
factor scores, shots); the numbers are made up and say so in every summary.
The screenshots are drawn with Pillow when it is installed (else their json
says ok: false, «Pillow не установлен»).

Point Настройки → Папка проекта at <out_dir> to browse it. Nothing here touches
the real vault.
"""
from __future__ import annotations

import csv
import json
import re
import random
import subprocess
import sys
import zlib
from pathlib import Path

OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "demo-vault")
CFG_SRC = Path(sys.argv[2]) if len(sys.argv) > 2 else None
STATE = OUT / "indices-kb" / "state"
SHOTS = OUT / "indices-kb" / "shots"
JOURNAL = OUT / "indices-kb" / "journal"
CONFIG = OUT / "indices-kb" / "config"
NOTES = OUT / "Макро-обзоры"
for d in (STATE, JOURNAL, CONFIG, NOTES):
    d.mkdir(parents=True, exist_ok=True)

ZONES = {"neutral": 15, "strong": 40}
DEMO = "[демо] "

# The factor lists of config/scale.json v3.1 (names, groups, shares).
FACTORS = {
    "eu": [
        ("bias_a", "Точка A / направление D1–H4", "контекст дня", 12), ("d1_flow", "D1 — направление (PO3)", "контекст дня", 6),
        ("h4_flow", "Структура H4", "контекст дня", 6), ("dr_zone", "Premium / Discount H4 DR", "контекст дня", 4),
        ("h1_flow", "Структура H1 (POI)", "контекст дня", 5), ("night_sweep", "Что снял премаркет", "окно открытия", 14),
        ("m15_flow", "Структура M15 (премаркет)", "окно открытия", 6), ("room", "Место до цели в окне", "окно открытия", 12),
        ("night_move", "Ход премаркета к закрытию Xetra", "окно открытия", 6), ("opens", "Цена к DO / WO", "окно открытия", 5),
        ("prev_close", "Как закрылась вчерашняя Xetra", "окно открытия", 4), ("m_verdict", "Новостной вердикт агента", "макро", 7),
        ("m_peers", "ES / NQ за ночь", "макро", 3), ("m_asia", "Азия", "макро", 1), ("m_vol", "VSTOXX / VIX", "макро", 1),
        ("m_yields", "Доходности Bund / US10Y", "макро", 1), ("m_dollar", "Евро (EURUSD)", "макро", 1), ("m_oil", "Нефть Brent", "макро", 1),
    ],
    "us": [
        ("bias_a", "Точка A / BIAS", "контекст дня", 12), ("d1_flow", "D1 — направление (PO3)", "контекст дня", 6),
        ("h4_flow", "Структура H4", "контекст дня", 5), ("dr_zone", "Premium / Discount H4 DR", "контекст дня", 4),
        ("h1_flow", "Структура H1 (POI)", "контекст дня", 6), ("night_sweep", "Что сняла ночь", "окно открытия", 11),
        ("m15_flow", "Структура M15 (ночь)", "окно открытия", 10), ("room", "Место до цели в окне", "окно открытия", 12),
        ("night_move", "Ход ночи к вчерашнему закрытию", "окно открытия", 6), ("opens", "Цена к DO / WO / NYM", "окно открытия", 5),
        ("prev_close", "Как закрылся вчерашний RTH", "окно открытия", 3), ("m_verdict", "Новостной вердикт агента", "макро", 7),
        ("m_peers", "Европа к 16:00 (FDAX, FESX)", "макро", 2), ("m_vol", "VIX", "макро", 2), ("m_yields", "Доходности US2Y / US10Y", "макро", 1),
        ("m_dollar", "Доллар (DXY)", "макро", 1), ("m_oil", "Нефть", "макро", 1), ("m_tone", "Тон заголовков", "макро", 1),
    ],
}
SYM = {"eu": ("FDAX", "FESX"), "us": ("ES", "NQ")}
WHY = {
    "bias_a": ("D1 вчера: сняли минимум прошлого дня и закрылись выше — точка A для лонга", "D1 вчера: сняли максимум прошлого дня и закрылись ниже — точка A для шорта"),
    "d1_flow": ("D1: тренд вверх (2 слома подряд)", "D1: тренд вниз (3 слома подряд)"),
    "h4_flow": ("H4: слом вверх, последний 3 свечи назад", "H4: слом вниз, последний 4 свечи назад"),
    "dr_zone": ("цена в нижней половине диапазона H4 — Discount", "цена в верхней половине диапазона H4 — Premium"),
    "h1_flow": ("H1: слом вверх у POI", "H1: слом вниз у POI"),
    "night_sweep": ("ночь сняла вчерашний минимум и вернулась выше", "ночь сняла вчерашний максимум и вернулась ниже"),
    "m15_flow": ("M15: последний слом за ночь вверх", "M15: последний слом за ночь вниз"),
    "room": ("вверх до ликвидности 0.5 ADR, вниз 0.2 ADR", "вниз до ликвидности 0.5 ADR, вверх 0.2 ADR"),
    "night_move": ("ночь выше вчерашнего закрытия на 0.3 ADR", "ночь ниже вчерашнего закрытия на 0.3 ADR"),
    "opens": ("цена выше открытий дня и недели", "цена ниже открытий дня и недели"),
    "prev_close": ("вчера закрылись в верхней четверти дня", "вчера закрылись в нижней четверти дня"),
    "m_verdict": ("новости толкают открытие вверх", "новости толкают открытие вниз"),
    "m_peers": ("смежный рынок +0.5 %", "смежный рынок −0.5 %"),
    "m_asia": ("Азия +0.8 %", "Азия −0.9 %"), "m_vol": ("индекс страха падает на 5 %", "индекс страха растёт на 7 %"),
    "m_yields": ("доходности −6 б.п.", "доходности +7 б.п."), "m_dollar": ("доллар слабеет", "доллар крепнет"),
    "m_oil": ("нефть −2.5 %", "нефть +3 %"), "m_tone": ("тон заголовков +6", "тон заголовков −5"),
}
ZONE_RU = {"strong-short": "сильный шорт", "short": "шорт", "neutral": "нейтрально", "long": "лонг", "strong-long": "сильный лонг"}


def zone(n: float) -> str:
    a = abs(n)
    if a < ZONES["neutral"]:
        return "neutral"
    if n > 0:
        return "strong-long" if a >= ZONES["strong"] else "long"
    return "strong-short" if a >= ZONES["strong"] else "short"


def needle(factors: list[dict], talk: list[dict] | None = None) -> dict:
    """Same arithmetic as scale.py: arguments take their share, base factors shrink."""
    talk = talk or []
    k = (100 - sum(t["share"] for t in talk)) / 100
    rows = [dict(f, share=round(f["share"] * k, 1), points=round(f["points"] * k, 1)) for f in factors] + talk
    long_ = round(sum(f["points"] for f in rows if f["side"] == "LONG"), 1)
    short = round(sum(f["points"] for f in rows if f["side"] == "SHORT"), 1)
    groups: dict[str, dict] = {}
    for f in rows:
        g = groups.setdefault(f["group"], {"share": 0.0, "long": 0.0, "short": 0.0})
        g["share"] = round(g["share"] + f["share"], 1)
        if f["side"] == "LONG":
            g["long"] = round(g["long"] + f["points"], 1)
        elif f["side"] == "SHORT":
            g["short"] = round(g["short"] + f["points"], 1)
    for g in groups.values():
        g["needle"] = round(g["long"] - g["short"])
    n = round(long_ - short)
    z = zone(n)
    return {"long": long_, "short": short, "undecided": round(100 - long_ - short, 1), "max": 100, "needle": n,
            "zone": ZONE_RU[z], "side": None if z == "neutral" else ("LONG" if n > 0 else "SHORT"), "groups": groups}


def factors_for(region: str, bias: float, seed: int, silent: int = 3, drop: tuple[str, ...] = ()) -> list[dict]:
    """bias in [-1, 1]: −1 = everything votes short, +1 = everything long."""
    rnd = random.Random(seed)
    base = [f for f in FACTORS[region] if f[0] not in drop]
    total = sum(f[3] for f in base)
    silent_ids = set(rnd.sample([f[0] for f in base], silent))
    out = []
    for fid, name, group, share_cfg in base:
        share = round(share_cfg * 100 / total, 1)
        if fid in silent_ids:
            side, strength, why = "", 0.0, "сегодня молчит: сигнал слабый"
        else:
            side = "LONG" if rnd.random() < (bias + 1) / 2 else "SHORT"
            strength = rnd.choice([1.0, 1.0, 0.7, 0.6, 0.4])
            why = WHY.get(fid, ("за лонг", "за шорт"))[0 if side == "LONG" else 1]
        out.append({"id": fid, "name": name, "group": group, "share_cfg": float(share_cfg), "on": True, "side": side,
                    "strength": strength, "why": why, "share": share, "points": round(share * strength, 1) if side else 0.0})
    return out


def light(color: str, why: list[str]) -> dict:
    return {"green": {"color": "green", "label": "🟢 торгуем", "red": [], "yellow": [], "risk_pct": 1},
            "yellow": {"color": "yellow", "label": "🟡 осторожно", "red": [], "yellow": why, "risk_pct": 0.5},
            "red": {"color": "red", "label": "🔴 не торгуем", "red": why, "yellow": [], "risk_pct": 0}}[color]


def plan(region: str, side: str | None, price: float, adr: float) -> dict:
    if region == "eu":
        session = {"entry": "10:30–12:00 (поздний — до 13:00)", "exit": "15:30 при красных новостях США, иначе 16:30", "review_asof": "20:00"}
        setups = [{"setup": "Сетап 3 (Европа)", "why": "единственный рабочий; обычный IB — средний (0.37 ADR)",
                   "entry": "лимит на возврат к пробитой границе IB после полнотелой M5 за IB", "stop": "за противоположной границей IB + запас",
                   "target": "IB1", "until": "вход до 12:00, поздний до 13:00"}]
    else:
        session = {"entry": "16:30–18:30", "setup1_until": "17:30", "exit": "18:30 (крайний срок 22:00)", "review_asof": "20:00"}
        setups = [{"setup": "Сетап 1", "why": "уровень в 0.12 ADR от цены — первая 5-минутка вероятно его снимет",
                   "entry": "лимит на возврат к краю первой 5-минутки после поглощения", "stop": "за экстремумом манипуляции + запас 0.02%",
                   "target": "ближайшее из: край ночи, PDH/PDL, IB1", "until": "лимит до 17:30"},
                  {"setup": "Сетап 2", "why": "обычный IB последних 20 дней — средний (0.42 ADR)",
                   "entry": "лимит на середину IB после двух M5 за IB 17:00+", "stop": "за противоположной границей IB + запас",
                   "target": "IB1 (или PDH/PDL рядом)", "until": "лимит до 18:30"}]
    if not side:
        return {"side": None, "price": price, "adr": adr, "session": session, "manipulation_levels": [], "targets": [], "setups": [],
                "summary": "Направления нет — по правилам (заметка 20, п.2) день пропускаем."}
    sgn = 1 if side == "LONG" else -1
    lvl = lambda k, name: {"price": round(price - sgn * k * adr, 2), "name": name, "adr": k}
    tgt = lambda k, name: {"price": round(price + sgn * k * adr, 2), "name": name, "adr": k}
    word = "лонг" if side == "LONG" else "шорт"
    first = "минимум ночи" if side == "LONG" else "максимум ночи"
    return {"side": side, "price": price, "adr": adr, "ib_typical_adr": 0.4, "ib_typical_class": "средний", "session": session,
            "manipulation_levels": [lvl(0.12, first), lvl(0.31, "открытие дня")],
            "targets": [tgt(0.22, "вчерашнее закрытие"), tgt(0.48, "максимум H4" if side == "LONG" else "минимум H4")],
            "setups": setups,
            "cancel": f"идея {'лонга' if side == 'LONG' else 'шорта'} отменяется, если две M5 закроются {'ниже' if side == 'LONG' else 'выше'} «открытие дня» до входа — тогда сделки нет",
            "summary": f"План A — {word}: ждём вынос {'вниз' if side == 'LONG' else 'вверх'} к «{first}» на открытии и возврат, цель — ближайшая ликвидность в пределах окна."}


def write(name: str, data: dict) -> None:
    (STATE / name).write_text(json.dumps(data, ensure_ascii=False, indent=1), encoding="utf-8")


def collect(region: str, d: str, status: str, events: list[tuple[str, str, str]], tone: int) -> dict:
    mk = [("FDAX", 0.4), ("FESX", 0.3), ("ES (S&P 500)", 0.2), ("NQ (Nasdaq 100)", 0.5), ("Nikkei 225", -0.3), ("VIX", -4.1),
          ("US 10Y", 0.04), ("DXY", -0.2), ("Brent", 1.8), ("Золото", 0.6)]
    return {"region": region, "date": d, "asof_eet": f"{d} {'09:31' if region == 'eu' else '16:01'}",
            "calendar": {"status": status, "reasons": [f"{DEMO}{e[1]} в {e[0]} — внутри окна" for e in events if e[2] == "High"] if status == "STOP" else [],
                         "events": [{"time_eet": e[0], "title": e[1], "impact": e[2]} for e in events]},
            "markets": [{"label": l, "chg_pct": round(c * (1 if tone >= 0 else -1), 2), "main": i < 2} for i, (l, c) in enumerate(mk)],
            "headlines": {"count": 240, "relevant": 120, "tone_sum": tone, "tags": {"ставки/ЦБ": 30, "энергия": 18}},
            "prev_day_check": []}


def verdict(v: str, conf: str, summary: str, plan_lines: list[str], **extra) -> dict:
    out = {"verdict": v, "confidence": conf, "summary": DEMO + summary,
           "reasons": ["Фьючерсы ночью в одну сторону с фоном.", "Главный драйвер дня — ставки и доходности.", "Смежные рынки подтверждают."],
           "risks": ["15:30 данные США — резкий ход в любую сторону."], "watch": ["Держится ли ночной экстремум после открытия."],
           "sources": [{"title": "Демо-источник: утренняя сводка", "url": "https://www.investing.com/news/", "source": "Investing.com"}],
           "plan": plan_lines}
    out.update(extra)
    return out


def scale_state(region: str, d: str, factors: list[dict], lt: dict, price: float, adr: float, talk: list[dict] | None = None,
                asof: str | None = None, history: int = 1) -> dict:
    res = needle(factors)
    st = {"region": region, "date": d, "asof": asof or f"{d} {'09:35' if region == 'eu' else '16:05'}", "sym": SYM[region][0],
          "pair": SYM[region][1], "version": "v3.1", "factors": factors, "result": res, "light": lt,
          "levels": {"price": price, "adr": adr}, "plan": plan(region, res["side"], price, adr), "discussion": [],
          "history": [{"asof": f"{d} 0{9 if region == 'eu' else 16}:3{i}", "needle": res["needle"] + i * 3, "after": None} for i in range(history)]}
    if talk:
        st["discussion"] = [{"at": t["at"], "by": t["by"], "side": t["side"], "share": t["share"], "text": t["text"],
                             "reply": t.get("reply", ""), "agent_view": t.get("view", "")} for t in talk]
        tf = [{"id": f"talk_{i + 1}", "name": f"Довод {i + 1}", "group": "обсуждение", "share": t["share"], "side": t["side"],
               "strength": 1.0, "points": t["share"], "why": t["text"]} for i, t in enumerate(talk)]
        st["after"] = needle(factors, tf)
    return st


def review(d: str, regions: dict) -> None:
    out = {"date": d, "asof": f"{d} 20:01", "regions": {}}
    for r, x in regions.items():
        if x is None:
            out["regions"][r] = {"missing": "шкалы за этот день нет"}
            continue
        st, dirn, chg, plan_hit, manip, partial = x
        side = (st.get("after") or st["result"])["side"]
        hit = "" if dirn == "FLAT" or not side else ("1" if side == dirn else "0")
        rows = [{"factor": f["id"], "group": f["group"], "share": f["share"], "side": f["side"], "strength": f["strength"], "dir": dirn,
                 "hit": "" if (not f["side"] or dirn == "FLAT") else ("1" if f["side"] == dirn else "0")} for f in st["factors"]]
        out["regions"][r] = {"facts": {"sym": st["sym"], "date": d, "dir": dirn, "chg_adr": chg, "first_break": "up" if dirn == "LONG" else "down",
                                       "ib_adr": 0.36, "partial": partial},
                             "score": {"needle_hit": hit if not st.get("after") else ("" if dirn == "FLAT" else ("1" if st["result"]["side"] == dirn else "0")),
                                       "after_hit": hit if st.get("after") else "", "plan_hit": plan_hit, "manip_swept": manip},
                             "factors": rows, "discussion": st.get("discussion", []), "needle": st["result"]["needle"],
                             "after": (st.get("after") or {}).get("needle"), "zone": st["result"]["zone"], "light": st["light"]}
    write(f"review_{d}.json", out)


stats_rows: list[dict] = []


def score(d: str, region: str, st: dict, dirn: str) -> None:
    for f in st["factors"]:
        if f["side"] and dirn != "FLAT":
            stats_rows.append({"date": d, "region": region, "factor": f["id"], "group": f["group"], "share": f["share"], "side": f["side"],
                               "strength": f["strength"], "dir": dirn, "hit": "1" if f["side"] == dirn else "0"})


def note(d: str, title: str) -> None:
    (NOTES / f"{d} {title}.md").write_text(f"# {DEMO}{d} {title}\n\nДемо-заметка для проверки кнопки «Открыть заметку».\n", encoding="utf-8")


# ---------- chart screenshots (state/shots_<D>_<region>.json + PNGs) ----------
# The real skill draws the plan's levels on TradingView and screenshots them.
# Here: a dark candlestick picture with the same lines, so the desk has
# something to show. Without Pillow the json says so (ok: false), like a real
# run that could not take pictures.
try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:  # the generator still runs; the desk then shows «Снимков графика нет»
    Image = None

COLORS = {"manip": "#d32f2f", "target": "#1e8e3e", "level": "#37474f", "open": "#9e9e9e"}
# On the dark chart the «level» grey of the legend would vanish; the picture uses a lighter twin.
DRAW_COLOR = {"#37474f": "#90a4ae"}


def cyrillic_font(size: int):
    """DejaVuSans (or any font fontconfig says covers Russian); None when there is none."""
    if Image is None:
        return None
    cands = ["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf",
             "/Library/Fonts/Arial Unicode.ttf", "C:/Windows/Fonts/segoeui.ttf", "C:/Windows/Fonts/arial.ttf", "DejaVuSans.ttf"]
    try:
        out = subprocess.run(["fc-list", ":lang=ru", "file"], capture_output=True, text=True, timeout=5).stdout
        found = [ln.split(":")[0].strip() for ln in out.splitlines() if ln.strip().lower().endswith((".ttf:", ".ttf"))]
        cands += sorted(found, key=lambda f: ("dejavusans" not in f.lower().replace("-", ""), f))
    except (OSError, subprocess.SubprocessError):
        pass
    for c in cands:
        try:
            return ImageFont.truetype(c, size)
        except OSError:
            continue
    return None  # PIL's default font has no Cyrillic: draw the lines without captions


def legend_for(st: dict, region: str) -> list[dict]:
    p = st.get("plan") or {}
    side = p.get("side")
    up = side != "SHORT"
    out = [{"key": f"manip{i + 1}", "label": f"манипуляция: {l['name']}", "kind": "manip", "color": COLORS["manip"], "price": l["price"]}
           for i, l in enumerate(p.get("manipulation_levels") or [])]
    out += [{"key": f"target{i + 1}", "label": f"цель: {l['name']}", "kind": "target", "color": COLORS["target"], "price": l["price"]}
            for i, l in enumerate(p.get("targets") or [])]
    price, adr = st["levels"]["price"], st["levels"]["adr"]
    out.append({"key": "prev_high" if up else "prev_low", "label": "вчерашний максимум" if up else "вчерашний минимум", "kind": "level",
                "color": COLORS["level"], "price": round(price + (0.62 if up else -0.62) * adr, 2)})
    out.append({"key": "NYM" if region == "us" else "XO", "label": "полночь Нью-Йорка" if region == "us" else "открытие Xetra", "kind": "open",
                "color": COLORS["open"], "price": round(price + (0.05 if up else -0.05) * adr, 2)})
    return out


def draw_chart(path: Path, seed: int, price: float, adr: float, levels: list[dict], bars: int, vol: float, shift: float = 0.0) -> None:
    """~1200×700 dark candles (random walk ending at `price`), the levels as lines, captions at the right, no prices."""
    W, H, PAD_L, PAD_Y = 1200, 700, 24, 40
    font = cyrillic_font(15)
    probe = ImageDraw.Draw(Image.new("RGB", (1, 1)))
    PAD_R = 40 + int(max((probe.textlength(l["label"], font=font) for l in levels), default=0)) if font is not None else 60
    rnd = random.Random(seed)
    closes = [0.0]
    for _ in range(bars - 1):
        closes.append(closes[-1] + rnd.gauss(0, vol * adr))
    off = price - closes[-1]
    candles = []
    prev = closes[0] + off + rnd.gauss(0, vol * adr)
    for c in closes:
        c += off
        o = prev
        hi = max(o, c) + abs(rnd.gauss(0, vol * adr * 0.6))
        lo = min(o, c) - abs(rnd.gauss(0, vol * adr * 0.6))
        candles.append((o, hi, lo, c))
        prev = c
    lv = [dict(l, price=l["price"] + shift * adr * (1 if i == 0 else 0)) for i, l in enumerate(levels)]
    top = max(max(c[1] for c in candles), max(l["price"] for l in lv))
    bot = min(min(c[2] for c in candles), min(l["price"] for l in lv))
    span = (top - bot) or 1
    y = lambda v: PAD_Y + (top - v) / span * (H - 2 * PAD_Y)

    img = Image.new("RGB", (W, H), "#131722")
    d = ImageDraw.Draw(img)
    for gx in range(PAD_L, W - PAD_R, 100):  # faint grid like TradingView's
        d.line([(gx, 0), (gx, H)], fill="#1c2030")
    for gy in range(PAD_Y, H, 80):
        d.line([(0, gy), (W, gy)], fill="#1c2030")
    step = (W - PAD_L - PAD_R) / bars
    body = max(2, int(step * 0.62))
    for i, (o, hi, lo, c) in enumerate(candles):
        x = PAD_L + i * step + step / 2
        col = "#26a69a" if c >= o else "#ef5350"
        d.line([(x, y(hi)), (x, y(lo))], fill=col, width=1)
        y0, y1 = sorted((y(o), y(c)))
        d.rectangle([x - body / 2, y0, x + body / 2, max(y1, y0 + 1)], fill=col)
    for l in lv:
        col = DRAW_COLOR.get(l["color"], l["color"])
        ly = y(l["price"])
        if l["kind"] == "open":
            for x0 in range(PAD_L, W - PAD_R + 10, 14):
                d.line([(x0, ly), (x0 + 7, ly)], fill=col, width=2)
        else:
            d.line([(PAD_L, ly), (W - PAD_R + 10, ly)], fill=col, width=2)
        if font is not None:
            tw = d.textlength(l["label"], font=font)
            tx = W - PAD_R + 18
            d.rectangle([tx - 6, ly - 11, tx + tw + 6, ly + 11], fill="#131722", outline=col)
            d.text((tx, ly), l["label"], fill=col, font=font, anchor="lm")
    path.parent.mkdir(parents=True, exist_ok=True)
    img.save(path, "PNG", optimize=True)


def shots(d: str, region: str, st: dict | None, *, error: str | None = None, verify_ok: bool = True, missing: tuple[str, ...] = ()) -> None:
    """Write shots_<d>_<region>.json and, when it is a good run, the two PNGs it lists."""
    asof = f"{d} {'09:40' if region == 'eu' else '15:55'}"
    if error is None and Image is None:
        error = "Pillow не установлен"
    if error is not None or st is None:
        write(f"shots_{d}_{region}.json", {"date": d, "region": region, "asof": asof, "ok": False, "error": error, "shots": []})
        return
    price, adr = st["levels"]["price"], st["levels"]["adr"]
    legend = legend_for(st, region)
    night = "ночь" if region == "us" else "премаркет"
    specs = [("m15", "M15", f"M15 — {night} и уровни у открытия", 64, 0.06, ["manip", "target", "level", "open"]),
             ("h1", "H1", "H1 — зоны и цели дня", 72, 0.11, ["manip", "target", "level"])]
    out = []
    for k, (name, tf, title, bars, vol, kinds) in enumerate(specs):
        rel = f"indices-kb/shots/{d}/macro_{region}/{name}.png"
        lg = [l for l in legend if l["kind"] in kinds]
        # A failed drawing check: the first line sits visibly off its place on M15.
        ok = verify_ok or name != "m15"
        if name in missing:
            (OUT / rel).unlink(missing_ok=True)
        else:
            draw_chart(OUT / rel, zlib.crc32(f"{d}/{region}/{name}".encode()), price, adr, lg, bars, vol, shift=0.0 if ok else 0.25)
        out.append({"name": name, "tf": tf, "title": title, "path": rel, "verify_ok": ok,
                    "legend": [{k2: l[k2] for k2 in ("key", "label", "kind", "color")} for l in lg]})
    write(f"shots_{d}_{region}.json", {"date": d, "region": region, "asof": asof, "ok": True, "error": None, "shots": out})


EV_EU = [("10:00", "Ifo — деловой климат Германии", "Medium"), ("11:00", "ZEW — настроения", "Medium"), ("15:30", "Заявки на пособие США", "Medium")]
EV_US = [("15:30", "CPI США", "High"), ("17:00", "Запасы нефти", "Medium"), ("21:00", "Выступление ФРС", "Medium")]

# ---------- 14.09 Пн: обычный хороший день ----------
d = "2026-09-14"
eu = scale_state("eu", d, factors_for("eu", -0.8, 1), light("yellow", ["фаза ПИЛА (5 из 10 дней в обе стороны) — риск ×0.5, цель до IB1"]), 25100, 285,
                 talk=[{"at": f"{d} 09:48", "by": "owner", "side": "LONG", "share": 10, "text": "DAX у недельного минимума — шортить в самый низ поздно",
                        "reply": "Частично: минимум недели рядом, но ночь сняла вчерашний максимум и вернулась ниже — шорт, но только до IB1.", "view": "partial"}], history=3)
write(f"scale_{d}_eu.json", eu)
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, -6))
write(f"macro_{d}_eu_verdict.json", verdict("SHORT", "medium", "Фон за шорт: доходности растут, нефть дорожает, фьючерсы ночью в минусе.",
      ["Вероятнее: открытие снимает ночной максимум и разворачивается вниз — шорт по сетапу 3, цель IB1.",
       "Манипуляцию ждём у ночного максимума; вход после возврата под уровень.",
       "Фаза пилы: риск 0.5 %, без сделки до 12:00 — день пропускаем."],
      day_shape={"forecast": "TWO_SIDED", "prob_two_sided": 0.42, "summary": "Пила и данные США в 15:30 — второй вынос вероятнее обычного.",
                 "windows": [{"window": "10:30–12:00", "expect": "первый вынос за IB"}, {"window": "12:00–15:30", "expect": "второй вынос в другую сторону"}]},
      scale_judgment=[{"side": "SHORT", "strength": 0.7, "text": "Нефть выше 100 — давление на открытие, которого нет в технике"}]))
note(d, "Европа")
shots(d, "eu", eu)
us = scale_state("us", d, factors_for("us", -0.5, 2), light("green", []), 7800, 58)
write(f"scale_{d}_us.json", us)
shots(d, "us", us)
write(f"macro_{d}_us.json", collect("us", d, "TRADE", EV_US[1:], -3))
write(f"macro_{d}_us_verdict.json", verdict("SHORT", "low", "Фон слабо за шорт: Европа в минусе, доллар крепнет.",
      ["Скорее вниз, но уверенность низкая — работаем только сетап 2 после закрытия IB.", "Цель IB1, риск 1 %."]))
note(d, "США")
# Note 23: the owner's read before the gauge and his two evening answers (Strategy Desk writes these).
write(f"human_read_{d}_eu.json", {"date": d, "region": "eu", "side": "SHORT", "at": f"{d} 09:28", "before_scale": True})
write(f"human_read_{d}_us.json", {"date": d, "region": "us", "side": "LONG", "at": f"{d} 16:03", "before_scale": False})
write(f"day_answers_{d}.json", {"date": d, "at": f"{d} 20:04", "regions": {"eu": {"trade": "short", "by_rule": "yes"}, "us": {"trade": "none"}},
      "note": "Ждал вторую манипуляцию, как в уроке — сработало; в США не входил: вялое открытие"})
review(d, {"eu": (eu, "SHORT", -0.42, "да", True, False), "us": (us, "SHORT", -0.25, "пробой без IB1", True, False)})
write(f"review_{d}_agent.json", {"summary": DEMO + "Оба окна пошли вниз, как показывала стрелка. Европа дошла до цели, США — пробой без цели.",
      "regions": {"eu": {"what_worked": "Снятие ночного максимума с отказом дало вход по сетапу 3.", "what_failed": "«Место до цели» голосовало за лонг — уровень был вплотную к цене.",
                         "discussion": "Довод про недельный минимум не подтвердился, но ограничил цель IB1 — это было верно."},
                  "us": {"what_worked": "Направление угадали.", "what_failed": "Цель IB1 не взяли — окно было вялым.", "discussion": "Обсуждения не было."}},
      "lessons": ["В пилу на DAX первая манипуляция у открытия дня не держит — ждать вторую.", "Уровень ближе 0.1 ADR — не цель, а шум."],
      "weight_ideas": ["Ход ночи и «что сняла ночь» — одна история, считать одним фактором?"]})
note(d, "Итог дня")
score(d, "eu", eu, "SHORT"); score(d, "us", us, "SHORT")

# ---------- 15.09 Вт: сильный лонг, стрелка ошиблась ----------
d = "2026-09-15"
eu = scale_state("eu", d, factors_for("eu", 0.85, 3), light("green", []), 25300, 270)
write(f"scale_{d}_eu.json", eu)
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, 8))
write(f"macro_{d}_eu_verdict.json", verdict("LONG", "high", "Фон за лонг: сильная отчётность SAP, Азия растёт, доходности падают.",
      ["Всё за лонг — ждём снятие минимума ночи на открытии и лонг по сетапу 3.", "Цель — вчерашний максимум, риск 1 %."]))
us = scale_state("us", d, factors_for("us", 0.7, 4), light("green", []), 7850, 60)
write(f"scale_{d}_us.json", us)
write(f"macro_{d}_us.json", collect("us", d, "TRADE", EV_US[1:], 5))
write(f"macro_{d}_us_verdict.json", verdict("LONG", "medium", "Фон за лонг: Европа сильная, VIX падает.", ["Лонг от снятия минимума ночи, сетап 1 до 17:30.", "Отмена — две M5 ниже открытия дня."]))
review(d, {"eu": (eu, "SHORT", -0.55, "нет", True, False), "us": (us, "SHORT", -0.38, "нет", False, False)})
write(f"review_{d}_agent.json", {"summary": DEMO + "Плохой день для стрелки: оба окна ушли против неё. По правилам сделки остановил бы стоп у открытия дня.",
      "regions": {"eu": {"what_worked": "Отмена по плану сработала — две M5 за уровнем, сделки не было бы.", "what_failed": "Контекст дня был за лонг, но премаркет уже снял максимум — это был сигнал разворота.", "discussion": "Обсуждения не было."},
                  "us": {"what_worked": "Ничего.", "what_failed": "Точка A за лонг, окно ушло вниз после слабой Европы.", "discussion": "Обсуждения не было."}},
      "lessons": ["Если премаркет уже снял цель по направлению — место до цели исчерпано, не входить."], "weight_ideas": ["Снизить вес «Точки A» на ES, если ещё 2 промаха."]})
score(d, "eu", eu, "SHORT"); score(d, "us", us, "SHORT")

# ---------- 16.09 Ср: нет перевеса ----------
d = "2026-09-16"
for r, seed in (("eu", 5), ("us", 6)):
    st = scale_state(r, d, factors_for(r, 0.0, seed, silent=6), light("green", []), 25200 if r == "eu" else 7820, 280 if r == "eu" else 59)
    # force a neutral needle: balance the two columns
    st["result"] = needle(st["factors"]) if abs(st["result"]["needle"]) < 15 else {**st["result"], "long": 31.0, "short": 27.0, "undecided": 42.0, "needle": 4, "zone": "нейтрально", "side": None}
    st["plan"] = plan(r, None, 0, 0)
    write(f"scale_{d}_{r}.json", st)
    write(f"macro_{d}_{r}.json", collect(r, d, "TRADE", EV_EU if r == "eu" else EV_US[1:], 0))
    write(f"macro_{d}_{r}_verdict.json", verdict("NEUTRAL", "low", "Фон смешанный: новостей мало, рынки без направления.", ["Перевеса нет — по правилам день пропускаем.", "Наблюдаем, куда пойдёт первый вынос за IB, — для статистики."]))
    globals()[r] = st
review(d, {"eu": (eu, "FLAT", 0.04, "нет пробоя", False, False), "us": (us, "LONG", 0.21, None, None, False)})
write(f"review_{d}_agent.json", {"summary": DEMO + "Пропуск по правилам. Европа простояла на месте, США немного выросли.",
      "regions": {"eu": {"what_worked": "Пропуск — правильно, окно было плоским."}, "us": {"what_worked": "Пропуск, рост был слабым."}}, "lessons": []})

# ---------- 17.09 Чт: красный светофор, CPI ----------
d = "2026-09-17"
eu = scale_state("eu", d, factors_for("eu", 0.4, 7), light("yellow", ["пятница-канун: 3 узких дня подряд — цель до IB1"]), 25250, 260)
write(f"scale_{d}_eu.json", eu)
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, 2))
write(f"macro_{d}_eu_verdict.json", verdict("LONG", "low", "Фон слабо за лонг перед CPI США.", ["Скорее вверх, но перед CPI — осторожно, риск 0.5 %.", "Закрыть всё до 15:30."]))
us = scale_state("us", d, factors_for("us", 0.6, 8), light("red", ["календарь: CPI США в 15:30 — день высокого риска", "ночь уже прошла 85 % среднего дневного хода"]), 7880, 61)
write(f"scale_{d}_us.json", us)
shots(d, "us", us)
write(f"macro_{d}_us.json", collect("us", d, "STOP", EV_US, 1))
write(f"macro_{d}_us_verdict.json", verdict("NO_TRADE", "high", "Не торгуем: CPI в 15:30 и ночь уже съела почти весь дневной ход.", ["Сегодня по правилам не торгуем.", "Наблюдаем: держится ли ночной минимум после CPI."]))
review(d, {"eu": (eu, "LONG", 0.18, "пробой без IB1", True, True), "us": (us, "LONG", 0.6, None, None, True)})  # agent part not written yet
score(d, "eu", eu, "LONG")

# ---------- 18.09 Пт: обсуждение переворачивает стрелку ----------
d = "2026-09-18"
eu = scale_state("eu", d, factors_for("eu", -0.75, 9, silent=4), light("yellow", ["пятница в Европе — риск ×0.5"]), 25150, 275,
                 talk=[{"at": f"{d} 09:41", "by": "owner", "side": "LONG", "share": 20, "text": "Квартальная экспирация опционов — к открытию цену будут держать у страйка 25 200, шорт сегодня ненадёжен",
                        "reply": "Согласен: в день экспирации выносы у открытия чаще ложные. Стрелка уходит в нейтраль — по правилам это пропуск.", "view": "agree"},
                       {"at": f"{d} 09:44", "by": "agent", "side": "SHORT", "share": 5, "text": "Ночь всё-таки закрылась ниже открытия недели — слабый довод за шорт остаётся",
                        "reply": "", "view": ""},
                       {"at": f"{d} 09:52", "by": "owner", "side": "LONG", "share": 10, "text": "Азия закрылась на максимумах",
                        "reply": "Не согласен: Азия уже в цене премаркета, её доля в шкале и так учтена — двойной счёт.", "view": "disagree"}])
write(f"scale_{d}_eu.json", eu)
shots(d, "eu", eu, verify_ok=False)
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, -1))
write(f"macro_{d}_eu_verdict.json", verdict("SHORT", "low", "Фон слабо за шорт, но день экспирации.", ["Утром — сильный шорт.", "После обсуждения перевеса нет — по правилам пропуск."]))
note(d, "Европа")
review(d, {"eu": (eu, "LONG", 0.12, None, None, False), "us": None})
write(f"review_{d}_agent.json", {"summary": DEMO + "Владелец был прав: в день экспирации окно пошло вверх, шорт по утренней стрелке дал бы стоп.",
      "regions": {"eu": {"what_worked": "Довод про экспирацию перевёл день в пропуск.", "what_failed": "Механика не знает про экспирацию.", "discussion": "Прав владелец: довод на 20 % оказался решающим. Мой довод про открытие недели — мимо."}},
      "lessons": ["День квартальной экспирации — по умолчанию пропуск в первые 30 минут."], "weight_ideas": ["Добавить фактор «экспирация» в светофор, а не в стрелку."]})
note(d, "Итог дня")
score(d, "eu", eu, "LONG")

# ---------- 21.09 Пн: обзоры в процессе ----------
d = "2026-09-21"
eu = scale_state("eu", d, factors_for("eu", -0.6, 10), light("green", []), 25000, 290)
write(f"scale_{d}_eu.json", eu)  # scale written, verdict not yet → «собирается»
shots(d, "eu", eu, error="TradingView не запущен")
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, -4))
write(f"macro_{d}_us_verdict.json", verdict("LONG", "medium", "Фон за лонг. Шкала не построилась: нет баров ночи (scale_error).", ["Механики нет — опираюсь только на новости: лонг от открытия, риск 0.5 %."]))

# ---------- 22.09 Вт: очень длинные тексты ----------
d = "2026-09-22"
long_plan = [("Очень длинная строка плана, чтобы проверить перенос: " + "открытие снимает ночной максимум, возвращается под него, формирует узкий IB и даёт пробой вниз с ретестом; " * 3).strip() for _ in range(2)] + [f"Пункт {i}: короткий." for i in range(3, 9)]
eu = scale_state("eu", d, factors_for("eu", -0.7, 11, silent=1), light("yellow", ["фаза ПИЛА (6 из 10 дней в обе стороны) — риск ×0.5, цель до IB1, пробой только с ретестом; плюс узкий ADR — меньше 0.8 % цены, цель только IB1"]), 25080, 240,
                 talk=[{"at": f"{d} 09:4{i}", "by": "owner" if i % 2 == 0 else "agent", "side": "LONG" if i % 2 == 0 else "SHORT", "share": 5,
                        "text": f"Довод {i + 1}: " + "длинный текст довода, чтобы проверить, как карточка переносит строки и не ломает вёрстку; " * 2,
                        "reply": "Ответ агента тоже длинный: " + "проверяю перенос строк в ответе и иконку позиции; " * 2, "view": ["agree", "partial", "disagree"][i % 3]} for i in range(5)], history=8)
write(f"scale_{d}_eu.json", eu)
shots(d, "eu", eu, missing=("h1",))  # the H1 picture is listed but its file is gone
c = collect("eu", d, "TRADE", EV_EU * 4, -9)
write(f"macro_{d}_eu.json", c)
write(f"macro_{d}_eu_verdict.json", verdict("SHORT", "medium", "Очень длинное резюме новостного фона: " + "доходности растут, нефть дорожает, Азия падает, евро слабеет, VSTOXX растёт; " * 4, long_plan,
      sources=[{"title": f"Источник {i}: очень длинный заголовок статьи о рынках, ставках и нефти, чтобы проверить перенос в списке", "url": "https://www.cnbc.com/markets/", "source": "CNBC"} for i in range(12)],
      day_shape={"forecast": "TREND_DOWN", "prob_two_sided": 0.25, "summary": "Трендовый день вниз вероятнее.", "windows": [{"window": f"окно {i}", "expect": "ожидание " * 6} for i in range(5)]}))
note(d, "Европа")

# ---------- 23.09 Ср: только Европа, итог без рефлексии ----------
d = "2026-09-23"
eu = scale_state("eu", d, factors_for("eu", 0.5, 12), light("green", []), 25400, 265)
write(f"scale_{d}_eu.json", eu)
shots(d, "eu", eu, error="dry-run")
write(f"macro_{d}_eu.json", collect("eu", d, "TRADE", EV_EU, 4))
write(f"macro_{d}_eu_verdict.json", verdict("LONG", "medium", "Фон за лонг.", ["Лонг от снятия минимума премаркета, цель IB1."]))
review(d, {"eu": (eu, "LONG", 0.33, "да", True, False), "us": None})
score(d, "eu", eu, "LONG")

# ---------- 24.09 Чт: старый формат шкалы (v1) ----------
d = "2026-09-24"
v1f = [{"id": f[0], "name": f[1], "group": "техника" if f[2] == "контекст дня" else "сессия", "weight": w, "on": True, "side": s, "strength": st,
        "why": WHY.get(f[0], ("", ""))[0 if s == "LONG" else 1] if s else "нет сигнала", "points": round(w * st, 2) if s else 0}
       for f, w, s, st in zip(FACTORS["us"][:8], [3, 2, 2, 1, 1, 2, 2, 1], ["LONG", "LONG", "LONG", "SHORT", "SHORT", "LONG", "", "SHORT"], [1, 1, 1, 0.4, 1, 0.94, 0, 0.33])]
L = round(sum(f["points"] for f in v1f if f["side"] == "LONG"), 2); S = round(sum(f["points"] for f in v1f if f["side"] == "SHORT"), 2)
write(f"scale_{d}_us.json", {"region": "us", "date": d, "asof": f"{d} 16:00", "sym": "ES", "pair": "NQ", "version": "v1", "factors": v1f,
      "result": {"long": L, "short": S, "max": 14, "needle": round((L - S) / (L + S) * 100), "zone": "сильный лонг", "side": "LONG",
                 "groups": {"техника": {"long": 7.0, "short": 1.4, "needle": 67}, "сессия": {"long": 1.88, "short": 0.33, "needle": 70}}},
      "light": light("green", []), "levels": {}, "plan": plan("us", "LONG", 7900, 60), "discussion": [], "history": []})

# ---------- journal ----------
(JOURNAL / "lessons.md").write_text("# Уроки вечерних разборов\n\n" + "".join(
    f"- 2026-09-{14 + i}: {DEMO}{t}\n" for i, t in enumerate([
        "В пилу на DAX первая манипуляция у открытия дня не держит — ждать вторую.",
        "Если премаркет уже снял цель по направлению — место до цели исчерпано.",
        "День квартальной экспирации — по умолчанию пропуск в первые 30 минут.",
        "Уровень ближе 0.1 ADR — не цель, а шум.",
        "После CPI первые 15 минут не входить.",
    ])), encoding="utf-8")
# pad statistics so «угадывал» shows (needs ≥ 3 cases per factor)
rnd = random.Random(42)
for day in range(1, 12):
    for region in ("eu", "us"):
        for fid, *_ in FACTORS[region]:
            if rnd.random() < 0.7:
                p = {"bias_a": 0.65, "night_sweep": 0.62, "room": 0.4, "dr_zone": 0.42}.get(fid, 0.52)
                stats_rows.append({"date": f"2026-09-{day:02d}", "region": region, "factor": fid, "group": "", "share": 5, "side": "LONG",
                                   "strength": 1, "dir": "LONG" if rnd.random() < p else "SHORT", "hit": ""})
for r in stats_rows:
    if r["hit"] == "":
        r["hit"] = "1" if r["side"] == r["dir"] else "0"
with (JOURNAL / "scale_factor_scores.csv").open("w", encoding="utf-8", newline="") as fh:
    w = csv.DictWriter(fh, fieldnames=["date", "region", "factor", "group", "share", "side", "strength", "dir", "hit"])
    w.writeheader()
    w.writerows(stats_rows)

cfg = {"version": "v3.1", "zones": ZONES, "regions": {r: {"factors": [{"id": f[0], "name": f[1], "group": f[2], "share": f[3]} for f in fs]} for r, fs in FACTORS.items()}}
if CFG_SRC and CFG_SRC.exists():
    cfg = json.loads(CFG_SRC.read_text(encoding="utf-8"))
(CONFIG / "scale.json").write_text(json.dumps(cfg, ensure_ascii=False, indent=1), encoding="utf-8")
(OUT / "README.md").write_text("# Демо-данные Strategy Desk\n\nПридуманные дни 14–24.09.2026 для проверки вкладки «Обзор дня». Создано scripts/make-demo-vault.py.\n", encoding="utf-8")
print("demo vault:", OUT.resolve(), "days:", sorted({m.group(1) for p in STATE.glob("*_2026-*.json") for m in [re.search(r"_(\d{4}-\d{2}-\d{2})", p.name)] if m}))

# ---------- 20:00 picture of the window (review shots), 14.09 Europe ----------
_src = OUT / "indices-kb" / "shots" / "2026-09-14" / "macro_eu" / "m15.png"
if _src.exists():
    import shutil as _sh
    _sh.copyfile(_src, _src.with_name("review_m15.png"))
    write("shots_2026-09-14_eu_review.json", {
        "date": "2026-09-14", "region": "eu", "asof": "2026-09-14 13:00", "ok": True, "error": None,
        "shots": [{"name": "review_m15", "tf": "M15", "title": "M15 — как прошло окно открытия",
                   "path": "indices-kb/shots/2026-09-14/macro_eu/review_m15.png", "verify_ok": True,
                   "legend": [{"key": "ib", "label": "IB — первые 30 минут", "kind": "fact", "color": "#ef6c00", "style": "solid"},
                              {"key": "ib1_dn", "label": "IB1 вниз", "kind": "fact", "color": "#ef6c00", "style": "dashed"},
                              {"key": "manip1", "label": "манипуляция: максимум премаркета", "kind": "manip", "color": "#d32f2f", "style": "solid"},
                              {"key": "manip2", "label": "открытие дня — не манипуляция: за ним максимум премаркета", "kind": "manip", "color": "#757575", "style": "dashed"},
                              {"key": "t1", "label": "цель: минимум H4 — вне ADR", "kind": "target", "color": "#1e8e3e", "style": "dashed"},
                              {"key": "zone1", "label": "точка A: снятие максимума D1", "kind": "zone", "color": "#5c6bc0", "style": "solid"},
                              {"key": "break", "label": "первый пробой IB вниз — 10:45", "kind": "fact", "color": "#ef6c00", "style": "solid"}]}]})

# ---------- «сегодня» для проверки заметки 23 ----------
# A copy of 14.09 under today's Kyiv date — with its reviews and the 20:00 result,
# but WITHOUT the owner's read and evening answers, so the question-first flow,
# «Показать обзор без ответа», «Твой день» and «Кто был прав» can be tried live
# in the demo. Everything else in the demo stays on the invented September days.
import shutil as _sh2
from datetime import datetime as _dt, timezone as _tz, timedelta as _td
_TODAY = (_dt.now(_tz.utc) + _td(hours=3)).strftime("%Y-%m-%d")  # Kyiv, close enough for a demo
_SRC = "2026-09-14"
# The whole vault is rewritten on every run, so a previous «today» (another date)
# is simply removed first — the marker remembers which one.
_MARK = OUT / ".today"
_OLD = _MARK.read_text(encoding="utf-8").strip() if _MARK.exists() else ""
if _OLD and _OLD != _TODAY:
    for _p in list(STATE.glob(f"*_{_OLD}*.json")) + list(NOTES.glob(f"{_OLD} *.md")):
        _p.unlink()
    _sh2.rmtree(SHOTS / _OLD, ignore_errors=True)
_MARK.write_text(_TODAY, encoding="utf-8")
for _p in list(STATE.glob(f"*_{_SRC}*.json")):
    if _p.name.startswith(("human_read_", "day_answers_")):
        continue
    _txt = _p.read_text(encoding="utf-8").replace(_SRC, _TODAY)
    (STATE / _p.name.replace(_SRC, _TODAY)).write_text(_txt, encoding="utf-8")
if (SHOTS / _SRC).exists():
    _sh2.copytree(SHOTS / _SRC, SHOTS / _TODAY, dirs_exist_ok=True)
for _n in NOTES.glob(f"{_SRC} *.md"):
    (NOTES / _n.name.replace(_SRC, _TODAY)).write_text(_n.read_text(encoding="utf-8").replace(_SRC, _TODAY), encoding="utf-8")
print("today for note 23:", _TODAY, "(copy of", _SRC + ", no read / answers)")
