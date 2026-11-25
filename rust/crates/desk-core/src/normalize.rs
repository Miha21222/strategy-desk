//! The `normalize*` functions of `src/shared/macro.ts` (and `trimCollect` of
//! `src/main/macro-state.ts`), with the same tolerance: a file that is not
//! one at all is `None`, a half-good one keeps its good parts, a missing field
//! gets the same default the TS gives it.
//!
//! They take `&Value` (a file parsed with [`crate::js::parse_json`]) instead
//! of the contract's `Value` by value: nothing here needs to own it.

use crate::js::{self, as_str, int_of, num, num_or_null, prop, rec, str_of, strs, truthy};
use crate::vault::*;
use serde_json::{Map, Value};

/// «Сегодня помни» shows at most this many lines.
pub const REMEMBER_MAX: usize = 3;
/// «Идеи на проверке» shows at most this many ideas.
pub const IDEAS_MAX: usize = 5;
/// «Похожие дни» keeps at most this many days.
pub const SIMILAR_MAX: usize = 5;
/// `lessons.md` lines kept for the day, newest first.
pub const LESSONS_TAIL: usize = 15;

/// `r.region === 'eu' || r.region === 'us' ? r.region : region`.
fn region_or(v: Option<&Value>, fallback: Region) -> Region {
    as_str(v).and_then(Region::parse).unwrap_or(fallback)
}

/// `typeof r.date === 'string' ? r.date : date`.
fn string_or(v: Option<&Value>, fallback: &str) -> String {
    as_str(v).unwrap_or(fallback).to_owned()
}

/// `str(x).trim()`.
fn trimmed(v: Option<&Value>) -> String {
    js::trim(&str_of(v)).to_owned()
}

/// A non-blank string (`typeof x === 'string' && x.trim() !== ''`).
fn has_text(v: Option<&Value>) -> bool {
    as_str(v).is_some_and(|s| !js::trim(s).is_empty())
}

// ---------------------------------------------------------------- the owner's two files

/// `normalizeHumanRead`: a parsed human_read file, or `None` when it is not one.
pub fn normalize_human_read(raw: &Value, date: &str, region: Region) -> Option<HumanRead> {
    let r = raw.as_object()?;
    let side = as_str(r.get("side")).and_then(HumanSide::parse)?;
    Some(HumanRead {
        date: string_or(r.get("date"), date),
        region: region_or(r.get("region"), region),
        side,
        at: string_or(r.get("at"), ""),
        before_scale: r.get("before_scale") != Some(&Value::Bool(false)),
    })
}

/// `normalizeDayAnswers`: a parsed day_answers file, or `None` when it is not one.
pub fn normalize_day_answers(raw: &Value, date: &str) -> Option<DayAnswers> {
    let r = raw.as_object()?;
    let src = r
        .get("regions")
        .filter(|v| truthy(v) && (v.is_object() || v.is_array()));
    let read = |k: &str| -> Option<DayAnswerRegion> {
        let x = prop(src, k)?.as_object()?;
        let trade = as_str(x.get("trade")).and_then(TradeTaken::parse)?;
        let by_rule = as_str(x.get("by_rule")).and_then(ByRule::parse);
        Some(DayAnswerRegion { trade, by_rule })
    };
    let regions = DayAnswerRegions {
        eu: read("eu"),
        us: read("us"),
    };
    let note = as_str(r.get("note"));
    if regions.is_empty() && note.is_none() {
        return None;
    }
    Some(DayAnswers {
        date: string_or(r.get("date"), date),
        at: string_or(r.get("at"), ""),
        regions,
        note: note.unwrap_or("").to_owned(),
    })
}

/// `humanHit`: did the owner's read match the window? Same rule as
/// `scale.py`'s `hit`: a directional read against the window's direction;
/// neutral / no-trade and a flat window are «no verdict» (`None`).
pub fn human_hit(side: Option<&str>, dir: Option<&str>) -> Option<bool> {
    let side = side.filter(|s| *s == "LONG" || *s == "SHORT")?;
    let dir = dir.filter(|d| *d == "LONG" || *d == "SHORT")?;
    Some(side == dir)
}

// ---------------------------------------------------------------- the memory cards

/// `normalizeRemember`: items without text or with an unknown kind are
/// dropped; at most three stay. `None` without an `items` array.
pub fn normalize_remember(raw: &Value, date: &str, region: Region) -> Option<Remember> {
    let r = raw.as_object()?;
    let items = r.get("items")?.as_array()?;
    let items: Vec<RememberItem> = items
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|x| {
            let kind = as_str(x.get("kind")).and_then(RememberKind::parse)?;
            if !has_text(x.get("text")) {
                return None;
            }
            Some(RememberItem {
                kind,
                text: trimmed(x.get("text")),
                why: trimmed(x.get("why")),
                ref_: trimmed(x.get("ref")),
                tags: strs(x.get("tags")),
            })
        })
        .take(REMEMBER_MAX)
        .collect();
    let conditions = rec(r.get("conditions")).map(|c| {
        let s = |k: &str| as_str(c.get(k)).map(str::to_owned);
        RememberConditions {
            region: s("region"),
            weekday: s("weekday"),
            phase: s("phase"),
            night: s("night"),
            night_adr: num_or_null(c.get("night_adr")),
            light: s("light"),
            earnings_giant_prev: c.get("earnings_giant_prev").and_then(Value::as_bool),
            tags: strs(c.get("tags")),
        }
    });
    let similar = rec(r.get("similar")).and_then(|s| {
        Some(RememberSimilar {
            date: as_str(s.get("date"))?.to_owned(),
            region: str_of(s.get("region")),
            distance: num_or_null(s.get("distance")),
            needle: num_or_null(s.get("needle")),
            dir: str_of(s.get("dir")),
            chg_adr: num_or_null(s.get("chg_adr")),
            human_side: str_of(s.get("human_side")),
        })
    });
    Some(Remember {
        date: string_or(r.get("date"), date),
        region: region_or(r.get("region"), region),
        asof: str_of(r.get("asof")),
        items,
        conditions,
        similar,
        lesson_candidates: int_of(r.get("lesson_candidates")),
        enough_history: r.get("enough_history") == Some(&Value::Bool(true)),
    })
}

/// `normalizeDrift`: signals without text are dropped; severity defaults to `info`.
pub fn normalize_drift(raw: &Value, date: &str, region: Region) -> Option<Drift> {
    let r = raw.as_object()?;
    let signals = r.get("signals")?.as_array()?;
    let signals = signals
        .iter()
        .filter_map(Value::as_object)
        .filter(|x| has_text(x.get("text")))
        .map(|x| {
            let kind = str_of(x.get("kind"));
            DriftSignal {
                kind: if kind.is_empty() {
                    "factor".into()
                } else {
                    kind
                },
                text: trimmed(x.get("text")),
                severity: if as_str(x.get("severity")) == Some("warn") {
                    "warn"
                } else {
                    "info"
                }
                .into(),
                data: rec(x.get("data")).cloned().unwrap_or_default(),
            }
        })
        .collect();
    Some(Drift {
        date: string_or(r.get("date"), date),
        region: region_or(r.get("region"), region),
        days: int_of(r.get("days")),
        enough_data: r.get("enough_data") == Some(&Value::Bool(true)),
        signals,
    })
}

/// `normalizeWeek`: `None` when it is not one (no `lines` array at all).
pub fn normalize_week(raw: &Value) -> Option<Week> {
    let r = raw.as_object()?;
    if !r.get("lines")?.is_array() {
        return None;
    }
    let share_proposal = rec(r.get("share_proposal")).and_then(|p| {
        Some(ShareProposal {
            factor: as_str(p.get("factor"))?.to_owned(),
            from: NumOrStr::from_value(p.get("from")),
            to: NumOrStr::from_value(p.get("to")),
            why: trimmed(p.get("why")),
        })
    });
    Some(Week {
        week_start: str_of(r.get("week_start")),
        week_end: str_of(r.get("week_end")),
        days: int_of(r.get("days")),
        enough_data: r.get("enough_data") == Some(&Value::Bool(true)),
        lines: strs(r.get("lines")),
        stats: rec(r.get("stats")).cloned().unwrap_or_default(),
        trader: strs(r.get("trader")),
        share_proposal,
    })
}

/// `normalizeIdeas`: items without text or with an unknown status are dropped.
pub fn normalize_ideas(raw: &Value) -> Option<Ideas> {
    let r = raw.as_object()?;
    let items = r.get("items")?.as_array()?;
    let items = items
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|x| {
            let status = as_str(x.get("status")).and_then(IdeaStatus::parse)?;
            has_text(x.get("text")).then_some((x, status))
        })
        .enumerate()
        .map(|(i, (x, status))| {
            let id = str_of(x.get("id"));
            Idea {
                // The index counts the ideas that survived the filter, as in the TS.
                id: if id.is_empty() {
                    format!("idea-{i}")
                } else {
                    id
                },
                date: str_of(x.get("date")),
                text: trimmed(x.get("text")),
                side: str_of(x.get("side")),
                region: str_of(x.get("region")),
                source: str_of(x.get("source")),
                confirmed: int_of(x.get("confirmed")),
                refuted: int_of(x.get("refuted")),
                status,
                backtest: rec(x.get("backtest")).map(|b| IdeaBacktest {
                    days: int_of(b.get("days")),
                    hit: int_of(b.get("hit")),
                    rate: num_or_null(b.get("rate")),
                }),
                last_check: str_of(x.get("last_check")),
            }
        })
        .collect();
    Some(Ideas {
        asof: str_of(r.get("asof")),
        items,
    })
}

/// `visibleIdeas`: open and proposed only, newest first, at most [`IDEAS_MAX`].
pub fn visible_ideas(ideas: Option<&Ideas>) -> Vec<&Idea> {
    let Some(ideas) = ideas else {
        return Vec::new();
    };
    let mut v: Vec<&Idea> = ideas
        .items
        .iter()
        .filter(|x| matches!(x.status, IdeaStatus::Open | IdeaStatus::Proposed))
        .collect();
    // Array.prototype.sort is stable, and so is sort_by.
    v.sort_by(|a, b| js::locale_compare(&b.date, &a.date));
    v.truncate(IDEAS_MAX);
    v
}

/// `similar_<D>_<region>.json` (no TS twin: a new skill file, read with the
/// same tolerance). `None` without a `days` array. A similar day must lie
/// strictly before the file's date (its `date`, else the day being loaded):
/// a day without a real ISO date, or on / after that date, is dropped; unknown sides / reaches are `None`; a score is clamped
/// to 0..1; `factors_hit` keeps only boolean entries; at most [`SIMILAR_MAX`].
pub fn normalize_similar(raw: &Value, date: &str, region: Region) -> Option<Similar> {
    let r = raw.as_object()?;
    let days = r.get("days")?.as_array()?;
    let side = |v: Option<&Value>| as_str(v).and_then(MoveSide::parse);
    let file_date = string_or(r.get("date"), date);
    // No checkable date for the file itself → no day can be shown as «before» it.
    let before = crate::names::parse_date(&file_date)
        .or_else(|_| crate::names::parse_date(date))
        .ok();
    let days = days
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|x| {
            let d = as_str(x.get("date")).map(js::trim).filter(|d| {
                crate::names::parse_date(d).is_ok_and(|d| before.is_some_and(|b| d < b))
            })?;
            let day_move = rec(x.get("day_move")).map(|m| DayMove {
                side: side(m.get("side")),
                pct_adr: num_or_null(m.get("pct_adr")),
            });
            let ib_move = rec(x.get("ib_move")).map(|m| IbMove {
                side: side(m.get("side")),
                reach: as_str(m.get("reach")).and_then(IbReach::parse),
            });
            let factors_hit = rec(x.get("factors_hit"))
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| v.as_bool().map(|b| (k.clone(), b)))
                        .collect()
                })
                .unwrap_or_default();
            Some(SimilarDay {
                date: d.to_owned(),
                score: num_or_null(x.get("score")).map(|s| s.clamp(0.0, 1.0)),
                matched: strs(x.get("match")),
                day_move,
                ib_move,
                verdict: as_str(x.get("verdict"))
                    .map(js::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                factors_hit,
            })
        })
        .take(SIMILAR_MAX)
        .collect();
    Some(Similar {
        date: file_date,
        region: region_or(r.get("region"), region),
        asof: str_of(r.get("asof")),
        days,
    })
}

// ---------------------------------------------------------------- chart screenshots

/// `normalizeShots`: `None` when it is not a shots file at all (garbage, an
/// array, a half-written file). Shots without a path are dropped; a missing
/// legend is an empty one.
pub fn normalize_shots(raw: &Value, region: Region) -> Option<Shots> {
    let r = raw.as_object()?;
    let ok = r.get("ok").and_then(Value::as_bool);
    let list = r.get("shots").and_then(Value::as_array);
    if ok.is_none() && list.is_none() {
        return None;
    }
    let shots: Vec<MacroShot> = list
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|x| as_str(x.get("path")).is_some())
        .map(|x| MacroShot {
            name: str_of(x.get("name")),
            tf: str_of(x.get("tf")),
            title: str_of(x.get("title")),
            path: str_of(x.get("path")),
            verify_ok: x.get("verify_ok").and_then(Value::as_bool),
            legend: x
                .get("legend")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
                .filter(|l| as_str(l.get("label")).is_some())
                .map(|l| {
                    let kind = str_of(l.get("kind"));
                    ShotLegend {
                        key: str_of(l.get("key")),
                        label: str_of(l.get("label")),
                        kind: if kind.is_empty() {
                            "level".into()
                        } else {
                            kind
                        },
                        color: str_of(l.get("color")),
                        style: as_str(l.get("style"))
                            .filter(|s| *s == "dashed" || *s == "solid")
                            .map(str::to_owned),
                    }
                })
                .collect(),
        })
        .collect();
    let error = match r.get("error") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        e => Some(str_of(e)),
    };
    Some(Shots {
        date: str_of(r.get("date")),
        region: region_or(r.get("region"), region),
        asof: str_of(r.get("asof")),
        // A file that lists pictures but forgot `ok` is taken at its pictures.
        ok: ok.unwrap_or(!shots.is_empty()),
        error,
        shots,
    })
}

// ---------------------------------------------------------------- the scale

/// `normalizeScale` + the gate in `regionSnapshot`: `None` unless the file is
/// an object with a (truthy) `result` — a scale without a result is
/// half-written and shown as «not yet». (In the TS the gate sits in
/// macro-state.ts and `normalizeScale` itself assumes a scale.)
pub fn normalize_scale(raw: &Value) -> Option<Raw<ScaleFile>> {
    if !raw.is_object() || !js::truthy_opt(raw.get("result")) {
        return None;
    }
    let mut v = normalize_scale_value(raw);
    js::jsify(&mut v);
    Some(Raw::new(v))
}

/// The JSON half of `normalizeScale`: bring a scale written by an older
/// `scale.py` to the v3 shape the desk draws.
///
/// v1 (before 10.10.2026) counted weights 1–3 and points out of `max`, with
/// no `share` per factor, no `undecided` and no group shares. The needle
/// itself is kept as written; only the missing percentages are derived, the
/// way v3 would: share = weight / Σweights, points = share × strength,
/// long % = long / max. A v3 file passes through untouched.
///
/// Where the TS would throw (`null` inside `factors` or `groups`), the
/// element is read as an empty object instead.
pub fn normalize_scale_value(s: &Value) -> Value {
    let empty = Vec::new();
    let factors: &Vec<Value> = s.get("factors").and_then(Value::as_array).unwrap_or(&empty);
    let legacy = factors
        .iter()
        .any(|f| !prop(Some(f), "share").is_some_and(Value::is_number));
    let on = |f: &Value| prop(Some(f), "on") != Some(&Value::Bool(false));
    let weight = |f: &Value| num(prop(Some(f), "weight"));

    let fix_result = |r: Option<&Value>, scale: f64| -> Option<Value> {
        let r = r?;
        if !truthy(r) {
            return Some(r.clone());
        }
        let is_num = |k: &str| prop(Some(r), k).is_some_and(Value::is_number);
        if is_num("undecided") && is_num("long") && is_num("short") && !legacy {
            return Some(r.clone());
        }
        let long = js::round(num(prop(Some(r), "long")) * scale * 10.0) / 10.0;
        let short = js::round(num(prop(Some(r), "short")) * scale * 10.0) / 10.0;
        let groups = prop(Some(r), "groups").filter(|g| truthy(g)).map(|g| {
            let mut out = Map::new();
            for (k, g) in js::spread(g) {
                let share: f64 = factors
                    .iter()
                    .filter(|f| as_str(prop(Some(f), "group")) == Some(k.as_str()) && on(f))
                    .fold(0.0, |n, f| n + weight(f))
                    * scale;
                let gp = |key: &str| prop(Some(&g), key);
                let mut e = Map::new();
                e.insert(
                    "share".into(),
                    match gp("share") {
                        Some(v @ Value::Number(_)) => v.clone(),
                        _ => js::number(js::round(share * 10.0) / 10.0),
                    },
                );
                e.insert(
                    "long".into(),
                    js::number(js::round(num(gp("long")) * scale * 10.0) / 10.0),
                );
                e.insert(
                    "short".into(),
                    js::number(js::round(num(gp("short")) * scale * 10.0) / 10.0),
                );
                e.insert("needle".into(), js::number(num(gp("needle"))));
                out.insert(k, Value::Object(e));
            }
            Value::Object(out)
        });
        let mut m = js::spread(r);
        m.insert("long".into(), js::number(long));
        m.insert("short".into(), js::number(short));
        let undecided = (js::round((100.0 - long - short) * 10.0) / 10.0).max(0.0);
        m.insert("undecided".into(), js::number(undecided));
        m.insert("needle".into(), js::number(num(prop(Some(r), "needle"))));
        match groups {
            Some(g) => {
                m.insert("groups".into(), g);
            }
            // `groups: undefined` — JSON.stringify leaves the key out, but an
            // existing key keeps its place as undefined, i.e. disappears too.
            None => {
                m.shift_remove("groups");
            }
        }
        Some(Value::Object(m))
    };

    let mut out = js::spread(s);
    let after_raw = s.get("after");
    let (fixed_factors, r_scale) = if !legacy {
        (factors.clone(), 1.0)
    } else {
        let result = s.get("result");
        let total = factors
            .iter()
            .filter(|f| on(f))
            .fold(0.0, |n, f| n + weight(f));
        let total = [total, num(prop(result, "max"))]
            .into_iter()
            .find(|x| *x != 0.0 && !x.is_nan())
            .unwrap_or(1.0);
        let scale = 100.0 / total;
        let fixed = factors
            .iter()
            .map(|f| {
                let share = js::round(weight(f) * scale * 10.0) / 10.0;
                let strength = num(prop(Some(f), "strength"));
                let points = if js::truthy_opt(prop(Some(f), "side")) {
                    js::round(share * strength * 10.0) / 10.0
                } else {
                    0.0
                };
                let mut m = js::spread(f);
                m.insert("share".into(), js::number(share));
                m.insert("strength".into(), js::number(strength));
                m.insert("points".into(), js::number(points));
                Value::Object(m)
            })
            .collect();
        let max = num(prop(result, "max"));
        let r_scale = 100.0 / if max != 0.0 { max } else { total };
        (fixed, r_scale)
    };
    out.insert("factors".into(), Value::Array(fixed_factors));
    if let Some(r) = fix_result(s.get("result"), r_scale) {
        out.insert("result".into(), r);
    }
    // `after: fixResult(s.after) ?? s.after`: absent stays absent (undefined),
    // null stays null, anything else is fixed like the result.
    match fix_result(after_raw, r_scale) {
        Some(a) => {
            out.insert("after".into(), a);
        }
        None => {
            out.shift_remove("after");
        }
    }
    Value::Object(out)
}

// ---------------------------------------------------------------- the collect file

/// `trimCollect` of macro-state.ts: the collect file is large (hundreds of
/// headlines); the desk gets the part it draws. `None` for a falsy file
/// (`null`, `false`, `0`, `""`); any other non-object reads as `{}`.
pub fn trim_collect(raw: &Value) -> Option<Raw<MacroFile>> {
    if !truthy(raw) {
        return None;
    }
    let r = Some(raw);
    let mut m = Map::new();
    let pass = |m: &mut Map<String, Value>, k: &str| {
        // `x: raw.x` — undefined is left out, everything else as written.
        if let Some(v) = prop(r, k) {
            m.insert(k.into(), v.clone());
        }
    };
    let or_null = |k: &str| js::nullish(prop(r, k)).cloned().unwrap_or(Value::Null);
    pass(&mut m, "asof_eet");
    pass(&mut m, "open_eet");
    m.insert("calendar".into(), or_null("calendar"));
    let markets = match prop(r, "markets") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                let x = Some(x);
                let mut e = Map::new();
                let label = js::nullish(prop(x, "label")).or(js::nullish(prop(x, "symbol")));
                e.insert(
                    "label".into(),
                    Value::String(label.map(js::to_string).unwrap_or_default()),
                );
                if let Some(c) = prop(x, "chg_pct") {
                    e.insert("chg_pct".into(), c.clone());
                }
                e.insert("main".into(), Value::Bool(js::truthy_opt(prop(x, "main"))));
                if let Some(k) = prop(x, "kind") {
                    e.insert("kind".into(), k.clone());
                }
                Value::Object(e)
            })
            .collect(),
        _ => Vec::new(),
    };
    m.insert("markets".into(), Value::Array(markets));
    let headlines = prop(r, "headlines").filter(|h| truthy(h)).map(|h| {
        let mut e = Map::new();
        for k in ["count", "relevant", "tone_sum", "tags"] {
            if let Some(v) = prop(Some(h), k) {
                e.insert(k.into(), v.clone());
            }
        }
        Value::Object(e)
    });
    m.insert("headlines".into(), headlines.unwrap_or(Value::Null));
    m.insert("tilt".into(), or_null("tilt"));
    m.insert("regime".into(), or_null("regime"));
    m.insert(
        "prev_day_check".into(),
        match prop(r, "prev_day_check") {
            Some(v @ Value::Array(_)) => v.clone(),
            _ => Value::Array(Vec::new()),
        },
    );
    m.insert("previous_verdict".into(), or_null("previous_verdict"));
    Some(Raw::new(Value::Object(m)))
}

/// `lessonsTail`: bullet lines of `journal/lessons.md`, newest last in the
/// file → newest first here, at most `n`. Like `slice(-n)` in JS, `n = 0`
/// means all of them (`slice(-0)` is `slice(0)`).
pub fn lessons_tail(md: &str, n: usize) -> Vec<String> {
    let bullets: Vec<String> = md
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .map(js::trim)
        .filter_map(|l| {
            // /^[-*]\s+/: a marker, at least one space, then the text.
            let rest = l.strip_prefix(['-', '*'])?;
            let text = rest.trim_start_matches(js::is_space);
            (text.len() < rest.len()).then(|| text.to_owned())
        })
        .collect();
    let skip = if n == 0 {
        0
    } else {
        bullets.len().saturating_sub(n)
    };
    bullets.into_iter().skip(skip).rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const D: &str = "2026-10-12";

    // ---- ported from macro.test.ts: chart screenshots

    fn shots_file() -> Value {
        json!({
            "date": "2026-10-09", "region": "us", "asof": "2026-10-09 15:55", "ok": true, "error": null,
            "shots": [
                { "name": "h1", "tf": "H1", "title": "H1 — зоны и цели дня", "path": "indices-kb/shots/2026-10-09/macro_us/h1.png", "verify_ok": true, "legend": [] },
                { "name": "m15", "tf": "M15", "title": "M15 — ночь и уровни у открытия", "path": "indices-kb/shots/2026-10-09/macro_us/m15.png", "verify_ok": false,
                  "legend": [{ "key": "manip1+DO", "label": "манипуляция: открытие дня", "kind": "manip", "color": "#d32f2f" }, { "key": "x", "kind": "level" }, "junk"] },
                { "name": "nopath", "tf": "D1" }
            ]
        })
    }

    #[test]
    fn shots_read_the_contract_and_drop_what_is_not_a_picture() {
        let s = normalize_shots(&shots_file(), Region::Eu).unwrap();
        assert_eq!(s.region, Region::Us);
        assert!(s.ok);
        assert_eq!(s.error, None);
        assert_eq!(
            s.shots.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
            ["h1", "m15"]
        );
        assert_eq!(s.shots[1].verify_ok, Some(false));
        assert_eq!(
            serde_json::to_value(&s.shots[1].legend).unwrap(),
            json!([{ "key": "manip1+DO", "label": "манипуляция: открытие дня", "kind": "manip", "color": "#d32f2f" }])
        );
    }

    #[test]
    fn shots_survive_garbage_and_a_failed_run() {
        assert_eq!(normalize_shots(&Value::Null, Region::Eu), None);
        assert_eq!(normalize_shots(&json!([1, 2]), Region::Eu), None);
        assert_eq!(normalize_shots(&json!("x"), Region::Eu), None);
        assert_eq!(normalize_shots(&json!({"hello": 1}), Region::Eu), None);
        let f = normalize_shots(
            &json!({"ok": false, "error": "dry-run", "shots": null}),
            Region::Eu,
        )
        .unwrap();
        assert!(!f.ok);
        assert_eq!(f.error.as_deref(), Some("dry-run"));
        assert!(f.shots.is_empty());
        assert_eq!(f.region, Region::Eu);
    }

    #[test]
    fn shots_edge_cases() {
        // Pictures but no `ok`: taken at the pictures.
        let s = normalize_shots(&json!({"shots": [{"path": "p"}]}), Region::Us).unwrap();
        assert!(s.ok);
        assert_eq!(s.shots[0].verify_ok, None);
        assert_eq!(
            serde_json::to_value(&s).unwrap()["shots"][0]["verify_ok"],
            Value::Null
        );
        // Empty error is no error; a numeric error is shown as text.
        assert_eq!(
            normalize_shots(&json!({"ok": false, "error": ""}), Region::Us)
                .unwrap()
                .error,
            None
        );
        assert_eq!(
            normalize_shots(&json!({"ok": false, "error": 42}), Region::Us)
                .unwrap()
                .error
                .as_deref(),
            Some("42")
        );
        // A legend line with a style keeps it only when it is one of the two.
        let s = normalize_shots(
            &json!({"ok": true, "shots": [{"path": "p", "legend": [{"label": "a", "style": "dashed"}, {"label": "b", "style": "dotted"}]}]}),
            Region::Us,
        )
        .unwrap();
        let l = serde_json::to_value(&s.shots[0].legend).unwrap();
        assert_eq!(
            l,
            json!([{"key": "", "label": "a", "kind": "level", "color": "", "style": "dashed"}, {"key": "", "label": "b", "kind": "level", "color": ""}])
        );
    }

    // ---- ported: the owner's two files

    #[test]
    fn human_read_is_read_and_anything_else_refused() {
        let r = normalize_human_read(
            &json!({"side": "LONG", "at": "2026-10-12 09:31", "before_scale": true}),
            D,
            Region::Eu,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&r).unwrap(),
            json!({"date": D, "region": "eu", "side": "LONG", "at": "2026-10-12 09:31", "before_scale": true})
        );
        assert_eq!(
            normalize_human_read(&json!({"side": "UP"}), D, Region::Eu),
            None
        );
        assert_eq!(normalize_human_read(&json!("LONG"), D, Region::Eu), None);
        let r = normalize_human_read(
            &json!({"side": "NO_TRADE", "before_scale": false, "region": "xx"}),
            D,
            Region::Us,
        )
        .unwrap();
        assert_eq!(r.region, Region::Us);
        assert!(!r.before_scale);
        // Anything but `false` counts as before the scale (the honest default).
        assert!(
            normalize_human_read(&json!({"side": "SHORT", "before_scale": 0}), D, Region::Us)
                .unwrap()
                .before_scale
        );
    }

    #[test]
    fn day_answers_drop_bad_regions() {
        let a = normalize_day_answers(
            &json!({"regions": {"eu": {"trade": "long", "by_rule": "yes"}, "us": {"trade": "maybe"}, "xx": {"trade": "short"}}, "note": "ждал вторую манипуляцию"}),
            D,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&a).unwrap(),
            json!({"date": D, "at": "", "regions": {"eu": {"trade": "long", "by_rule": "yes"}}, "note": "ждал вторую манипуляцию"})
        );
        assert_eq!(normalize_day_answers(&json!({"regions": {}}), D), None);
        let a = normalize_day_answers(
            &json!({"regions": {"us": {"trade": "none", "by_rule": "what"}}, "note": 7}),
            D,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&a.regions.us).unwrap(),
            json!({"trade": "none"})
        );
        assert_eq!(a.note, "");
        // A note alone is an answer; regions that are not an object are none.
        assert!(
            normalize_day_answers(&json!({"regions": "x", "note": ""}), D)
                .unwrap()
                .regions
                .is_empty()
        );
        assert_eq!(normalize_day_answers(&json!([]), D), None);
    }

    #[test]
    fn human_hit_scores_like_scale_py() {
        assert_eq!(human_hit(Some("LONG"), Some("LONG")), Some(true));
        assert_eq!(human_hit(Some("SHORT"), Some("LONG")), Some(false));
        assert_eq!(human_hit(Some("NEUTRAL"), Some("LONG")), None);
        assert_eq!(human_hit(Some("LONG"), Some("FLAT")), None);
        assert_eq!(human_hit(None, Some("LONG")), None);
    }

    // ---- ported: the memory cards

    #[test]
    fn remember_keeps_three_good_items_and_tolerates_missing_keys() {
        let r = normalize_remember(
            &json!({
                "date": D, "region": "eu", "asof": "2026-10-12 09:31",
                "items": [
                    { "kind": "lesson", "text": "В пилу ждать вторую манипуляцию", "why": "те же теги: пила, пятница", "ref": "lessons.md 08.10", "tags": ["eu", "пила"] },
                    { "kind": "rule", "text": "Пятница в Европе — риск ×0.5" },
                    { "kind": "similar", "text": "Похожий день 2026-10-08: шорт, −0.4 ADR", "why": "", "ref": "2026-10-08 eu", "tags": "nope" },
                    { "kind": "lesson", "text": "четвёртый — лишний" },
                    { "kind": "joke", "text": "не тот вид" },
                    { "kind": "lesson", "text": "" },
                    "garbage"
                ],
                "conditions": { "region": "eu", "weekday": "fri", "phase": "пила", "night": "wide", "night_adr": 0.9, "light": "yellow", "earnings_giant_prev": false, "tags": ["eu", "пила", 3] },
                "similar": { "date": "2026-10-08", "region": "eu", "distance": 0.12, "needle": -38, "dir": "SHORT", "chg_adr": -0.4, "human_side": "SHORT" },
                "lesson_candidates": 4, "enough_history": true
            }),
            D,
            Region::Eu,
        )
        .unwrap();
        assert_eq!(
            r.items.iter().map(|x| x.kind).collect::<Vec<_>>(),
            [
                RememberKind::Lesson,
                RememberKind::Rule,
                RememberKind::Similar
            ]
        );
        assert_eq!(
            serde_json::to_value(&r.items[1]).unwrap(),
            json!({"kind": "rule", "text": "Пятница в Европе — риск ×0.5", "why": "", "ref": "", "tags": []})
        );
        assert!(r.items[2].tags.is_empty());
        let c = r.conditions.as_ref().unwrap();
        assert_eq!(c.tags, ["eu", "пила"]);
        assert_eq!(c.night_adr, Some(0.9));
        assert_eq!(
            serde_json::to_value(&r.similar).unwrap(),
            json!({"date": "2026-10-08", "region": "eu", "distance": 0.12, "needle": -38, "dir": "SHORT", "chg_adr": -0.4, "human_side": "SHORT"})
        );
        assert_eq!(r.lesson_candidates, 4);
        assert!(r.enough_history);
        // the minimum: a file with an empty list is a file, anything else is not
        let m = normalize_remember(&json!({"items": []}), D, Region::Us).unwrap();
        assert_eq!(
            serde_json::to_value(&m).unwrap(),
            json!({"date": D, "region": "us", "asof": "", "items": [], "conditions": null, "similar": null, "lesson_candidates": 0, "enough_history": false})
        );
        assert_eq!(normalize_remember(&json!({"date": D}), D, Region::Us), None);
        assert_eq!(normalize_remember(&json!([]), D, Region::Us), None);
        assert_eq!(normalize_remember(&json!("x"), D, Region::Us), None);
        assert_eq!(normalize_remember(&Value::Null, D, Region::Us), None);
    }

    #[test]
    fn remember_conditions_leave_out_what_is_absent() {
        let r = normalize_remember(&json!({"items": [], "conditions": {"tags": ["eu"], "weekday": 5}, "lesson_candidates": -3.6}), D, Region::Eu).unwrap();
        assert_eq!(
            serde_json::to_value(&r.conditions).unwrap(),
            json!({"night_adr": null, "tags": ["eu"]})
        );
        assert_eq!(r.lesson_candidates, 0);
        let r = normalize_remember(
            &json!({"items": [], "similar": {"region": "eu"}}),
            D,
            Region::Eu,
        )
        .unwrap();
        assert_eq!(r.similar, None);
    }

    #[test]
    fn drift_keeps_only_signals_with_text() {
        let d = normalize_drift(
            &json!({"date": D, "region": "eu", "days": 20, "enough_data": true, "signals": [{"kind": "phase", "text": "фаза: тренд → пила", "severity": "warn", "data": {"from": "тренд"}}, {"kind": "adr", "text": "ADR сжался на 30 %"}, {"kind": "factor", "text": "", "severity": "warn"}, {"severity": "warn"}, 5]}),
            D,
            Region::Eu,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&d.signals).unwrap(),
            json!([
                {"kind": "phase", "text": "фаза: тренд → пила", "severity": "warn", "data": {"from": "тренд"}},
                {"kind": "adr", "text": "ADR сжался на 30 %", "severity": "info", "data": {}}
            ])
        );
        assert_eq!(d.days, 20);
        let e = normalize_drift(&json!({"signals": []}), D, Region::Us).unwrap();
        assert_eq!(
            (e.region, e.days, e.enough_data, e.signals.len()),
            (Region::Us, 0, false, 0)
        );
        assert_eq!(normalize_drift(&json!({"date": D}), D, Region::Us), None);
        assert_eq!(normalize_drift(&Value::Null, D, Region::Us), None);
        // A signal without a kind is a factor signal.
        let k = normalize_drift(&json!({"signals": [{"text": "x"}]}), D, Region::Us).unwrap();
        assert_eq!(k.signals[0].kind, "factor");
    }

    #[test]
    fn week_lines_trader_and_share_proposal() {
        let w = normalize_week(&json!({"week_start": "2026-09-14", "week_end": "2026-09-18", "days": 5, "enough_data": true, "lines": ["рынок: пила", "сработало: сетап 3", "", 3, "урок повторился"], "stats": {"hits": 3}, "trader": ["4 стопа из 5 — вход до закрытия IB", ""], "share_proposal": {"factor": "Точка A", "from": 12, "to": 10, "why": "3 промаха из 5."}})).unwrap();
        assert_eq!(
            w.lines,
            ["рынок: пила", "сработало: сетап 3", "урок повторился"]
        );
        assert_eq!(w.trader, ["4 стопа из 5 — вход до закрытия IB"]);
        assert_eq!(
            serde_json::to_value(&w.share_proposal).unwrap(),
            json!({"factor": "Точка A", "from": 12, "to": 10, "why": "3 промаха из 5."})
        );
        assert_eq!(serde_json::to_value(&w.stats).unwrap(), json!({"hits": 3}));
        assert!(normalize_week(&json!({"lines": []}))
            .unwrap()
            .lines
            .is_empty());
        assert_eq!(
            normalize_week(&json!({"lines": ["x"], "share_proposal": {"why": "no factor"}}))
                .unwrap()
                .share_proposal,
            None
        );
        assert_eq!(
            serde_json::to_value(
                normalize_week(&json!({"lines": ["x"], "share_proposal": {"factor": "f"}}))
                    .unwrap()
                    .share_proposal
            )
            .unwrap(),
            json!({"factor": "f", "from": null, "to": null, "why": ""})
        );
        assert_eq!(normalize_week(&json!({"week_start": "2026-09-14"})), None);
        assert_eq!(normalize_week(&Value::Null), None);
    }

    fn idea(i: u32, status: &str) -> Value {
        let date = format!("2026-10-{i:02}");
        json!({"id": format!("i{i}"), "date": date, "text": format!("идея {i}"), "side": "LONG", "region": "eu", "source": "chat", "confirmed": i, "refuted": 1, "status": status,
               "backtest": if i % 2 == 1 { json!({"days": 30, "hit": 18, "rate": 0.6}) } else { Value::Null }, "last_check": date})
    }

    #[test]
    fn ideas_show_only_open_and_proposed_newest_first_five_at_most() {
        let ideas = normalize_ideas(&json!({"asof": D, "items": [
            idea(1, "open"), idea(2, "rule"), idea(3, "proposed"), idea(4, "dropped"), idea(5, "open"), idea(6, "open"), idea(7, "open"), idea(8, "open"),
            {"status": "open", "text": ""}, {"status": "maybe", "text": "x"}, null
        ]}))
        .unwrap();
        assert_eq!(ideas.items.len(), 8);
        assert_eq!(
            serde_json::to_value(&ideas.items[0].backtest).unwrap(),
            json!({"days": 30, "hit": 18, "rate": 0.6})
        );
        assert_eq!(ideas.items[1].backtest, None);
        assert_eq!(
            visible_ideas(Some(&ideas))
                .iter()
                .map(|x| x.id.as_str())
                .collect::<Vec<_>>(),
            ["i8", "i7", "i6", "i5", "i3"]
        );
        assert!(visible_ideas(None).is_empty());
        let one =
            normalize_ideas(&json!({"items": [{"text": "без id", "status": "open"}]})).unwrap();
        let i = &one.items[0];
        assert_eq!(
            (
                i.id.as_str(),
                i.confirmed,
                i.refuted,
                i.date.as_str(),
                &i.backtest
            ),
            ("idea-0", 0, 0, "", &None)
        );
        assert_eq!(normalize_ideas(&json!({"asof": "x"})), None);
        assert_eq!(normalize_ideas(&json!([])), None);
    }

    #[test]
    fn idea_ids_count_the_survivors() {
        let ideas = normalize_ideas(&json!({"items": [{"text": "", "status": "open"}, {"text": "a", "status": "open"}, {"text": "b", "status": "open", "id": 7}]})).unwrap();
        assert_eq!(ideas.items[0].id, "idea-0");
        assert_eq!(ideas.items[1].id, "7");
    }

    // ---- ported: older scale files

    #[test]
    fn v1_scale_gets_the_percentages_it_never_had() {
        let v1 = json!({
            "region": "us", "date": "2026-10-08", "asof": "", "sym": "ES", "version": "v1",
            "factors": [
                { "id": "a", "name": "A", "group": "техника", "weight": 3, "side": "LONG", "strength": 1, "points": 3 },
                { "id": "b", "name": "B", "group": "сессия", "weight": 1, "side": "SHORT", "strength": 0.5, "points": 0.5 },
                { "id": "c", "name": "C", "group": "сессия", "weight": 1, "side": "", "strength": 0, "points": 0 }
            ],
            "result": { "long": 3, "short": 0.5, "max": 5, "needle": 71, "zone": "сильный лонг", "side": "LONG", "groups": { "техника": { "long": 3, "short": 0, "needle": 100 }, "сессия": { "long": 0, "short": 0.5, "needle": -100 } } },
            "light": { "color": "red", "label": "" }
        });
        let s = normalize_scale(&v1).unwrap();
        let sp: Vec<(f64, f64)> = s.factors.iter().map(|f| (f.share(), f.points())).collect();
        assert_eq!(sp, [(60.0, 60.0), (20.0, 10.0), (20.0, 0.0)]);
        let r = &s.raw()["result"];
        assert_eq!(
            (&r["long"], &r["short"], &r["undecided"], &r["needle"]),
            (&json!(60), &json!(10), &json!(30), &json!(71))
        );
        assert_eq!(
            r["groups"]["техника"],
            json!({"share": 60, "long": 60, "short": 0, "needle": 100})
        );
        assert_eq!(r["groups"]["сессия"]["share"], json!(40));
        assert!(s.raw().get("after").is_none());
    }

    #[test]
    fn v3_scale_is_left_alone() {
        let v3 = json!({"factors": [{"id": "a", "name": "A", "group": "g", "side": "LONG", "strength": 1, "share": 12, "points": 12}], "result": {"long": 12, "short": 0, "undecided": 88, "needle": 12, "zone": "", "side": null}});
        assert_eq!(normalize_scale(&v3).unwrap().raw(), &v3);
        // after: null stays null, in place.
        let mut with_after = v3.clone();
        with_after["after"] = Value::Null;
        assert_eq!(normalize_scale(&with_after).unwrap().raw(), &with_after);
    }

    #[test]
    fn scale_without_a_result_is_not_yet_a_scale() {
        assert!(normalize_scale(&json!({"factors": []})).is_none());
        assert!(normalize_scale(&json!({"result": 0})).is_none());
        assert!(normalize_scale(&json!([])).is_none());
        assert!(normalize_scale(&Value::Null).is_none());
    }

    #[test]
    fn v3_result_without_undecided_is_completed() {
        let s = normalize_scale(&json!({"factors": [], "result": {"long": 20.04, "short": 30, "needle": "x"}, "after": {"long": 90, "short": 30, "undecided": 0}})).unwrap();
        assert_eq!(
            s.raw()["result"],
            json!({"long": 20, "short": 30, "needle": 0, "undecided": 50})
        );
        // After is v3-shaped already: untouched (even though long + short > 100).
        assert_eq!(
            s.raw()["after"],
            json!({"long": 90, "short": 30, "undecided": 0})
        );
        // Factors missing entirely: an empty list is added, as `{...s, factors}` does.
        assert_eq!(s.raw()["factors"], json!([]));
    }

    #[test]
    fn legacy_scale_with_junk_inside_does_not_panic() {
        let s = normalize_scale(&json!({"factors": [null, 5, {"weight": "x", "side": "LONG"}], "result": "x", "after": 0})).unwrap();
        // total = 0 → max = 0 → 1, so every share is 0
        assert_eq!(
            s.raw()["factors"],
            json!([{"share": 0, "strength": 0, "points": 0}, {"share": 0, "strength": 0, "points": 0}, {"weight": "x", "side": "LONG", "share": 0, "strength": 0, "points": 0}])
        );
        assert_eq!(
            s.raw()["result"],
            json!({"long": 0, "short": 0, "undecided": 100, "needle": 0})
        );
        assert_eq!(s.raw()["after"], json!(0));
    }

    // ---- ported from macro-state.ts: trimCollect

    #[test]
    fn collect_is_trimmed_to_what_the_desk_draws() {
        let c = trim_collect(&json!({"asof_eet": "09:31", "headlines": {"count": 284, "relevant": 138, "tone_sum": -5, "tags": {}, "feeds": [1, 2, 3]}, "markets": [{"symbol": "X", "label": "FDAX", "chg_pct": -0.5, "main": true, "last": 1}], "verdict": {"verdict": "NEUTRAL"}})).unwrap();
        assert_eq!(
            c.raw(),
            &json!({"asof_eet": "09:31", "calendar": null, "markets": [{"label": "FDAX", "chg_pct": -0.5, "main": true}], "headlines": {"count": 284, "relevant": 138, "tone_sum": -5, "tags": {}}, "tilt": null, "regime": null, "prev_day_check": [], "previous_verdict": null})
        );
        assert_eq!(c.markets[0].label.as_deref(), Some("FDAX"));
    }

    #[test]
    fn collect_labels_fall_back_to_the_symbol_and_any_value_prints() {
        let c = trim_collect(&json!({"markets": [{"symbol": "ES"}, {"label": null, "symbol": 5}, {}, null, {"label": [1, 2], "main": "yes"}]})).unwrap();
        let labels: Vec<_> = c.raw()["markets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["label"].clone())
            .collect();
        assert_eq!(
            labels,
            [json!("ES"), json!("5"), json!(""), json!(""), json!("1,2")]
        );
        assert_eq!(c.raw()["markets"][4]["main"], json!(true));
        assert!(trim_collect(&json!(0)).is_none());
        assert!(trim_collect(&json!("")).is_none());
        // A truthy non-object is read as {}.
        assert_eq!(
            trim_collect(&json!([1])).unwrap().raw()["markets"],
            json!([])
        );
        assert_eq!(
            trim_collect(&json!({"headlines": "x"})).unwrap().raw()["headlines"],
            json!({})
        );
    }

    #[test]
    fn lessons_newest_first() {
        assert_eq!(
            lessons_tail("# Уроки\n\n## 2026-10-08\n- 2026-10-08: один\n- 2026-10-08: два\n* 2026-10-09: три\n", 2),
            ["2026-10-09: три", "2026-10-08: два"]
        );
        assert_eq!(
            lessons_tail("\u{FEFF}- первый\r\n-без пробела\r\n  -  второй  \r\n", 15),
            ["второй", "первый"]
        );
        assert!(lessons_tail("", 15).is_empty());
        assert_eq!(lessons_tail("- x\n- y\n", 0), ["y", "x"]);
    }

    // ---- «Похожие дни» (new file, no TS twin)

    #[test]
    fn similar_days_are_read_with_the_usual_tolerance() {
        let s = normalize_similar(
            &json!({
                "date": D, "region": "us", "asof": "2026-10-12 08:30",
                "days": [
                    {"date": "2026-09-14", "score": 0.82, "match": ["гэп вниз 0.3 ADR", "", 5, " пила "],
                     "day_move": {"side": "SHORT", "pct_adr": -0.42}, "ib_move": {"side": "SHORT", "reach": "IB1"},
                     "verdict": "  план сработал ", "factors_hit": {"bias_a": true, "room": false, "odd": "yes"}},
                    {"date": "2026-09-01", "score": 1.7, "day_move": {"side": "UP"}, "ib_move": {"reach": "none"}, "verdict": "  "},
                    {"score": 0.5},
                    {"date": 20260901},
                    "junk",
                    {"date": "2026-08-01", "score": "x", "day_move": 3},
                    {"date": "2026-07-01"}, {"date": "2026-06-01"}, {"date": "2026-05-01"}
                ]
            }),
            D,
            Region::Eu,
        )
        .unwrap();
        assert_eq!(s.region, Region::Us);
        assert_eq!(
            s.days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(),
            [
                "2026-09-14",
                "2026-09-01",
                "2026-08-01",
                "2026-07-01",
                "2026-06-01"
            ]
        );
        let first = &s.days[0];
        assert_eq!(first.matched, ["гэп вниз 0.3 ADR", "пила"]);
        assert_eq!(
            first.day_move,
            Some(DayMove {
                side: Some(MoveSide::Short),
                pct_adr: Some(-0.42)
            })
        );
        assert_eq!(
            first.ib_move,
            Some(IbMove {
                side: Some(MoveSide::Short),
                reach: Some(IbReach::Ib1)
            })
        );
        assert_eq!(first.verdict.as_deref(), Some("план сработал"));
        assert_eq!(first.factors_hit.len(), 2);
        assert_eq!(first.factors_hit.get("room"), Some(&false));
        let second = &s.days[1];
        assert_eq!(second.score, Some(1.0));
        assert_eq!(second.day_move.as_ref().unwrap().side, None);
        assert_eq!(
            second.ib_move.as_ref().unwrap().reach,
            Some(IbReach::NoReach)
        );
        assert_eq!(second.verdict, None);
        assert_eq!((s.days[2].score, &s.days[2].day_move), (None, &None));
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            json!({"date": "2026-09-14", "score": 0.82, "match": ["гэп вниз 0.3 ADR", "пила"],
                   "day_move": {"side": "SHORT", "pct_adr": -0.42}, "ib_move": {"side": "SHORT", "reach": "IB1"},
                   "verdict": "план сработал", "factors_hit": {"bias_a": true, "room": false}})
        );
        let back: Similar = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn similar_days_lie_strictly_before_the_file_date() {
        let mk = |file: Value, days: &[&str]| {
            let ds: Vec<Value> = days.iter().map(|d| json!({"date": d})).collect();
            let mut raw = json!({"days": ds});
            if !file.is_null() {
                raw["date"] = file;
            }
            normalize_similar(&raw, "2026-09-14", Region::Eu)
                .unwrap()
                .days
                .into_iter()
                .map(|d| d.date)
                .collect::<Vec<_>>()
        };
        let days = [
            "2026-09-14",
            "2026-09-15",
            "2026-09-13",
            "14.09.2026",
            "2026-02-30",
            "",
            "2026-09-01",
            "2026-08-31",
            "2026-08-30",
            "2026-08-29",
            "2026-08-28",
            "2026-08-27",
        ];
        // The same date and later ones go, so do unparsable ones; then the cap of 5.
        assert_eq!(
            mk(json!("2026-09-14"), &days),
            [
                "2026-09-13",
                "2026-09-01",
                "2026-08-31",
                "2026-08-30",
                "2026-08-29"
            ]
        );
        // No date in the file: the day being loaded is the bound.
        assert_eq!(
            mk(Value::Null, &["2026-09-14", "2026-09-10"]),
            ["2026-09-10"]
        );
        // A junk file date falls back to the day being loaded too.
        assert_eq!(
            mk(json!("сегодня"), &["2026-09-14", "2026-09-10"]),
            ["2026-09-10"]
        );
        // The file's own date wins over the day being loaded when both are real.
        assert_eq!(
            mk(json!("2026-09-11"), &["2026-09-12", "2026-09-10"]),
            ["2026-09-10"]
        );
        // Nothing to compare against at all → nothing shown.
        let none = normalize_similar(
            &json!({"date": "x", "days": [{"date": "2026-01-01"}]}),
            "y",
            Region::Eu,
        )
        .unwrap();
        assert!(none.days.is_empty());
    }

    #[test]
    fn similar_without_days_is_not_a_file() {
        assert_eq!(normalize_similar(&json!({"date": D}), D, Region::Eu), None);
        assert_eq!(
            normalize_similar(&json!({"days": "x"}), D, Region::Eu),
            None
        );
        assert_eq!(normalize_similar(&json!([]), D, Region::Eu), None);
        assert_eq!(normalize_similar(&Value::Null, D, Region::Eu), None);
        let e = normalize_similar(&json!({"days": []}), D, Region::Us).unwrap();
        assert_eq!(
            (e.date.as_str(), e.region, e.asof.as_str(), e.days.len()),
            (D, Region::Us, "", 0)
        );
    }
}
