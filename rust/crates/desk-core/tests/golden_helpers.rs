#![cfg(feature = "golden")]
//! Golden: the pure helpers of desk-core answer what the TS functions of
//! `src/shared/macro.ts` answer (TEAM.md rule 4), case by case from
//! rust/fixtures/golden/expected/helpers/*.json. The one place where the TZ
//! changes the behaviour on purpose is the window clock (TZ №21): windows.json
//! carries both answers and these tests take the `tz` one.
//!
//!   cd rust && cargo test -p desk-core --features golden --test golden_helpers
//!
//! Every call into desk-core is in `mod api`. Section A follows CONTRACTS.md
//! (`zones`, `gauge`, `windows`, `stats`, `names`, `normalize`); section B
//! is a guess for helpers CONTRACTS.md does not place yet — point them at
//! wherever they land, or delete the test that uses one.

mod common;

use chrono::{DateTime, Utc};
use common::*;
use serde_json::{json, Value};

fn helpers(name: &str) -> Value {
    read_json(&golden().join("expected/helpers").join(name))
}

fn rows<'a>(v: &'a Value, table: &str) -> &'a Vec<Value> {
    v[table]
        .as_array()
        .unwrap_or_else(|| panic!("table {table} missing"))
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("RFC 3339 in the golden file")
        .with_timezone(&Utc)
}

mod api {
    use chrono::{DateTime, NaiveDate, Utc};
    use serde_json::Value;

    use super::kebab;

    pub fn date(d: &str) -> NaiveDate {
        NaiveDate::parse_from_str(d, "%Y-%m-%d").expect("ISO date")
    }

    fn region(r: &str) -> desk_core::vault::Region {
        match r {
            "eu" => desk_core::vault::Region::Eu,
            "us" => desk_core::vault::Region::Us,
            other => panic!("region {other}"),
        }
    }

    fn json<T: serde::Serialize>(v: T) -> Value {
        serde_json::to_value(v).expect("serializes")
    }

    // ------------------------------------------------ A. CONTRACTS.md

    fn zones_cfg(neutral: f64, strong: f64) -> desk_core::zones::Zones {
        desk_core::zones::Zones::new(neutral, strong)
    }

    pub fn zone_of(needle: f64, neutral: f64, strong: f64) -> String {
        kebab(&desk_core::zones::zone_of(
            needle,
            &zones_cfg(neutral, strong),
        ))
    }

    pub fn clamp_needle(n: f64) -> f64 {
        desk_core::zones::clamp_needle(n)
    }

    /// (key, from, to) per segment, left to right.
    pub fn zone_segments(neutral: f64, strong: f64) -> Vec<(String, f64, f64)> {
        desk_core::zones::zone_segments(&zones_cfg(neutral, strong))
            .into_iter()
            .map(|s| (kebab(&s.key), s.from, s.to))
            .collect()
    }

    pub fn needle_angle(needle: f64) -> f64 {
        desk_core::gauge::needle_angle(needle)
    }

    pub fn polar(cx: f64, cy: f64, r: f64, deg: f64) -> (f64, f64) {
        desk_core::gauge::polar(cx, cy, r, deg)
    }

    /// A Day from its dump JSON (the golden day files ARE that JSON).
    pub fn day_from_json(v: Value) -> desk_core::day::Day {
        serde_json::from_value(v).expect("Day deserializes from the dump JSON")
    }

    fn window(id: &str) -> desk_core::windows::WindowId {
        match id {
            "eu" => desk_core::windows::WindowId::Eu,
            "us" => desk_core::windows::WindowId::Us,
            "review" => desk_core::windows::WindowId::Review,
            other => panic!("window {other}"),
        }
    }

    /// `ready` / `partial` / `waiting` / `due` / `missed`.
    pub fn window_state(id: &str, now: DateTime<Utc>, day: &desk_core::day::Day) -> String {
        kebab(&desk_core::windows::window_state(window(id), now, day))
    }

    /// Rows of the day's factor statistics, serialized like `Day.stats`.
    pub fn factor_stats(csv: &str, d: &str, days: u32) -> Value {
        let rows = desk_core::stats::parse_csv(csv);
        json(desk_core::stats::factor_stats_rows(
            &rows,
            date(d),
            i64::from(days),
        ))
    }

    /// kind: scale, macro, macro_verdict, shots, shots_review, review, review_agent,
    /// human_read, day_answers, remember, drift, week, ideas, note, review_note.
    /// desk-core carries the region inside `StateKind`; the dateless `ideas.json`
    /// and the two notes (vault-relative paths) have functions of their own.
    pub fn state_file(kind: &str, d: Option<&str>, r: Option<&str>) -> String {
        use desk_core::names::{self as n, StateKind as K};
        let day = || date(d.expect("date for a dated file"));
        let reg = || region(r.expect("region for a per-window file"));
        let k = match kind {
            "ideas" => return n::IDEAS_FILE.to_owned(),
            "note" => return n::region_note(day(), reg()),
            "review_note" => return n::review_note(day()),
            "scale" => K::Scale(reg()),
            "macro" => K::Macro(reg()),
            "macro_verdict" => K::Verdict(reg()),
            "shots" => K::Shots(reg()),
            "shots_review" => K::ShotsReview(reg()),
            "human_read" => K::HumanRead(reg()),
            "remember" => K::Remember(reg()),
            "drift" => K::Drift(reg()),
            "review" => K::Review,
            "review_agent" => K::ReviewAgent,
            "day_answers" => K::DayAnswers,
            "week" => K::Week,
            other => panic!("kind {other}"),
        };
        n::state_file(k, day())
    }

    /// desk-core answers with the date as written (as TS does); the NaiveDate
    /// view of it — what a caller can open — keeps only real calendar dates.
    pub fn date_of_state_file(name: &str) -> Option<String> {
        desk_core::names::date_of_state_file(name)
            .and_then(|d| desk_core::names::parse_date(d).ok())
            .map(|d| d.format("%Y-%m-%d").to_string())
    }

    /// Same NaiveDate view: `MacroIndex::dates()` over the listed names.
    pub fn list_days(names: &[&str]) -> Vec<String> {
        desk_core::day::MacroIndex::new("vault", Some(names.iter().copied()))
            .dates()
            .into_iter()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .collect()
    }

    /// `normalize*` by its TS name, TS argument order; the answer as JSON (`null` = not this kind of file).
    pub fn normalize(f: &str, args: &[Value]) -> Value {
        use desk_core::normalize as n;
        let raw = &args[0];
        let s = |i: usize| args[i].as_str().expect("string arg");
        match f {
            "normalizeHumanRead" => json(n::normalize_human_read(raw, s(1), region(s(2)))),
            "normalizeDayAnswers" => json(n::normalize_day_answers(raw, s(1))),
            "normalizeRemember" => json(n::normalize_remember(raw, s(1), region(s(2)))),
            "normalizeDrift" => json(n::normalize_drift(raw, s(1), region(s(2)))),
            "normalizeWeek" => json(n::normalize_week(raw)),
            "normalizeIdeas" => json(n::normalize_ideas(raw)),
            "normalizeShots" => json(n::normalize_shots(raw, region(s(1)))),
            // The TS function itself (no «has a result» gate, which sits in regionSnapshot).
            "normalizeScale" => n::normalize_scale_value(raw),
            // B: visibleIdeas(normalizeIdeas(raw)) → ids
            "visibleIdeas" => {
                let ideas = n::normalize_ideas(raw);
                json(
                    n::visible_ideas(ideas.as_ref())
                        .into_iter()
                        .map(|x| x.id.clone())
                        .collect::<Vec<_>>(),
                )
            }
            other => panic!("normalize fn {other}"),
        }
    }

    // ------------------------------------------------ B. not placed by CONTRACTS.md yet

    /// Kyiv wall clock of an instant: (date, minutes since midnight).
    pub fn kyiv_now(now: DateTime<Utc>) -> (String, u32) {
        let k = desk_core::windows::kyiv_now(now);
        (k.date.format("%Y-%m-%d").to_string(), k.minutes)
    }

    pub fn default_window(day: &desk_core::day::Day, now: DateTime<Utc>) -> String {
        kebab(&desk_core::windows::default_window(day, now))
    }

    /// Rows as objects header → cell, like the TS parseCsv.
    pub fn parse_csv(text: &str) -> Value {
        json(desk_core::stats::parse_csv(text))
    }

    pub fn lessons_tail(md: &str, n: usize) -> Vec<String> {
        desk_core::normalize::lessons_tail(md, n)
    }

    pub fn is_shot_path(rel: &str) -> bool {
        desk_core::names::is_shot_path(rel)
    }

    pub fn run_command(id: &str, d: &str) -> String {
        desk_core::prompts::run_command(window(id), date(d))
    }

    pub fn discuss_prompt(r: &str, side: &str, share: f64, text: &str, d: &str) -> String {
        let side = match side {
            "LONG" => desk_core::vault::Side::Long,
            _ => desk_core::vault::Side::Short,
        };
        desk_core::prompts::discuss_prompt(region(r), side, share, text, date(d))
    }

    pub fn macro_chat_title(d: &str) -> String {
        desk_core::prompts::macro_chat_title(date(d))
    }
}

// ==================================================================== zones / gauge

#[test]
fn zones_match_scale_py_thresholds() {
    let g = helpers("zones.json");
    let mut r = Report::default();
    for c in rows(&g, "zone_of") {
        let (n, z) = (f64_of(&c["needle"]), &c["zones"]);
        let got = api::zone_of(n, f64_of(&z["neutral"]), f64_of(&z["strong"]));
        r.eq(
            format!("zone_of({n}, {z})"),
            str_of(&c["out"]).to_owned(),
            got,
        );
    }
    for c in rows(&g, "zone_segments") {
        let z = &c["zones"];
        let got: Vec<Value> = api::zone_segments(f64_of(&z["neutral"]), f64_of(&z["strong"]))
            .into_iter()
            .map(|(k, f, t)| json!({"key": k, "from": f, "to": t}))
            .collect();
        r.json(
            format!("zone_segments({z})"),
            &c["out"],
            &Value::Array(got),
            Cmp::default(),
        );
    }
    r.finish("zones.json");
}

#[test]
fn gauge_geometry_matches() {
    let z = helpers("zones.json");
    let g = helpers("gauge.json");
    let mut r = Report::default();
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    for c in rows(&z, "clamp_needle") {
        let (input, want) = (f64_of(&c["in"]), f64_of(&c["out"]));
        r.case();
        if !close(api::clamp_needle(input), want) {
            r.fail(
                format!("clamp_needle({input})"),
                vec![format!("expected {want}, got {}", api::clamp_needle(input))],
            );
        }
    }
    for c in rows(&g, "needle_angle") {
        let (n, want) = (f64_of(&c["needle"]), f64_of(&c["out"]));
        r.case();
        if !close(api::needle_angle(n), want) {
            r.fail(
                format!("needle_angle({n})"),
                vec![format!("expected {want}, got {}", api::needle_angle(n))],
            );
        }
    }
    for c in rows(&g, "polar") {
        let (x, y) = api::polar(
            f64_of(&c["cx"]),
            f64_of(&c["cy"]),
            f64_of(&c["r"]),
            f64_of(&c["deg"]),
        );
        r.json(
            format!("polar(deg {})", c["deg"]),
            &c["out"],
            &json!([x, y]),
            Cmp::default(),
        );
    }
    // The four corners of every zone segment, built from polar + needle_angle like arcPath does.
    let geo = &g["geometry"];
    let (cx, cy, r0, r1) = (
        f64_of(&geo["cx"]),
        f64_of(&geo["cy"]),
        f64_of(&geo["r0"]),
        f64_of(&geo["r1"]),
    );
    for c in rows(&g, "arc") {
        let (from, to) = (f64_of(&c["from"]), f64_of(&c["to"]));
        let p = |r: f64, n: f64| {
            let (x, y) = api::polar(cx, cy, r, api::needle_angle(n));
            json!([x, y])
        };
        let got = json!([p(r1, from), p(r1, to), p(r0, to), p(r0, from)]);
        r.json(
            format!("arc corners {} {from}..{to}", c["key"]),
            &c["corners"],
            &got,
            Cmp::default(),
        );
    }
    r.finish("gauge.json");
}

// ==================================================================== windows (TZ №21)

/// The day of a windows.json case: the base day with `date` = shown and the
/// files that `presence` says are absent set to null.
fn case_day(base: &Value, c: &Value) -> Value {
    let mut d = base.clone();
    d["date"] = c["shown"].clone();
    let p = &c["presence"];
    for reg in ["eu", "us"] {
        let has = str_of(&p[reg]);
        if !matches!(has, "scale" | "both") {
            d[reg]["scale"] = Value::Null;
        }
        if !matches!(has, "verdict" | "both") {
            d[reg]["verdict"] = Value::Null;
        }
    }
    let rv = str_of(&p["review"]);
    if !matches!(rv, "mech" | "both") {
        d["review"]["mech"] = Value::Null;
    }
    if !matches!(rv, "agent" | "both") {
        d["review"]["agent"] = Value::Null;
    }
    d
}

#[test]
fn window_states_follow_the_exchange_clocks() {
    let w = helpers("windows.json");
    let base = read_json(&golden().join(str_of(&w["base_day"])));
    let mut r = Report::default();
    for c in rows(&w, "cases") {
        let day = api::day_from_json(case_day(&base, c));
        let now = utc(str_of(&c["utc"]));
        for id in ["eu", "us", "review"] {
            let label = format!(
                "{id} at {} (Kyiv {} {:02}:{:02}), shown {}, presence {}{}",
                c["utc"],
                c["kyiv"]["date"],
                c["kyiv"]["minutes"].as_u64().unwrap_or(0) / 60,
                c["kyiv"]["minutes"].as_u64().unwrap_or(0) % 60,
                c["shown"],
                c["presence"],
                if c["tz_differs"] == Value::Bool(true) {
                    " [DST mismatch: TZ differs from the old app]"
                } else {
                    ""
                }
            );
            r.eq(
                label,
                str_of(&c["tz"][id]).to_owned(),
                api::window_state(id, now, &day),
            );
        }
    }
    r.finish("windows.json (tz)");
}

#[test]
fn kyiv_clock_and_default_window_match() {
    let w = helpers("windows.json");
    let m = helpers("misc.json");
    let base = read_json(&golden().join(str_of(&w["base_day"])));
    let mut r = Report::default();
    for c in rows(&m, "kyiv_now") {
        let (d, min) = api::kyiv_now(utc(str_of(&c["utc"])));
        r.json(
            format!("kyiv_now({})", c["utc"]),
            &c["out"],
            &json!({"date": d, "minutes": min}),
            Cmp::default(),
        );
    }
    for c in rows(&w, "cases") {
        let now = utc(str_of(&c["utc"]));
        r.json(
            format!("kyiv_now({})", c["utc"]),
            &c["kyiv"],
            &json!({"date": api::kyiv_now(now).0, "minutes": api::kyiv_now(now).1}),
            Cmp::default(),
        );
        let day = api::day_from_json(case_day(&base, c));
        r.eq(
            format!(
                "default_window at {} shown {} presence {}",
                c["utc"], c["shown"], c["presence"]
            ),
            str_of(&c["default_window_ts"]).to_owned(),
            api::default_window(&day, now),
        );
    }
    r.finish("kyiv_now / default_window");
}

// ==================================================================== stats

fn csv_text(s: &Value, c: &Value) -> String {
    if let Some(f) = c["csv_file"].as_str() {
        std::fs::read_to_string(golden().join(f)).expect("csv fixture")
    } else {
        str_of(&s["csv_inline"][str_of(&c["csv_inline"])]).to_owned()
    }
}

#[test]
fn factor_stats_match() {
    let s = helpers("stats.json");
    let mut r = Report::default();
    for c in rows(&s, "factor_stats") {
        let csv = csv_text(&s, c);
        let days = c["days"].as_u64().expect("days") as u32;
        let label = format!(
            "factor_stats({}, {}, {days})",
            c.get("csv_file")
                .or(c.get("csv_inline"))
                .unwrap_or(&Value::Null),
            c["date"]
        );
        r.json(
            label,
            &c["out"],
            &api::factor_stats(&csv, str_of(&c["date"]), days),
            Cmp::default(),
        );
    }
    r.finish("stats.json factor_stats (see sort_ties: ICU collation, not byte order)");
}

#[test]
fn csv_and_lessons_readers_match() {
    let s = helpers("stats.json");
    let mut r = Report::default();
    for c in rows(&s, "parse_csv") {
        r.json(
            format!("parse_csv({})", c["text"]),
            &c["out"],
            &api::parse_csv(str_of(&c["text"])),
            Cmp::default(),
        );
    }
    for c in rows(&s, "lessons_tail") {
        let want: Vec<String> = c["out"]
            .as_array()
            .expect("out")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect();
        r.eq(
            format!("lessons_tail({}, {})", c["md"], c["n"]),
            want,
            api::lessons_tail(str_of(&c["md"]), c["n"].as_u64().expect("n") as usize),
        );
    }
    r.finish("stats.json parse_csv / lessons_tail");
}

// ==================================================================== names

/// A TS answer that is not a real calendar date (`2026-13-45`): TS keeps it,
/// desk-core works in NaiveDate and drops it (CONTRACTS.md «только из
/// проверенной даты») — the one deliberate difference in names.json.
fn real_date(d: &str) -> bool {
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
}

#[test]
fn file_names_match() {
    let n = helpers("names.json");
    let mut r = Report::default();
    for c in rows(&n, "state_file") {
        let got = api::state_file(str_of(&c["kind"]), c["date"].as_str(), c["region"].as_str());
        r.eq(
            format!("state_file({}, {}, {})", c["kind"], c["date"], c["region"]),
            str_of(&c["out"]).to_owned(),
            got,
        );
    }
    for c in rows(&n, "date_of_state_file") {
        let want = c["out"]
            .as_str()
            .filter(|d| real_date(d))
            .map(str::to_owned);
        r.eq(
            format!("date_of_state_file({})", c["name"]),
            want,
            api::date_of_state_file(str_of(&c["name"])),
        );
    }
    for c in rows(&n, "list_days") {
        let names: Vec<&str> = c["names"]
            .as_array()
            .expect("names")
            .iter()
            .map(str_of)
            .collect();
        let want: Vec<String> = c["out"]
            .as_array()
            .expect("out")
            .iter()
            .map(str_of)
            .filter(|d| real_date(d))
            .map(str::to_owned)
            .collect();
        r.eq("list_days(names.json)", want, api::list_days(&names));
    }
    for c in rows(&n, "is_shot_path") {
        // A non-string in TS is simply «no»; Rust takes &str, so those rows are skipped.
        if let Some(rel) = c["rel"].as_str() {
            r.eq(
                format!("is_shot_path({rel:?})"),
                c["out"].as_bool().expect("bool"),
                api::is_shot_path(rel),
            );
        }
    }
    r.finish("names.json");
}

// ==================================================================== normalize

#[test]
fn normalize_matches_the_ts_tolerance() {
    let n = helpers("normalize.json");
    let mut r = Report::default();
    for c in rows(&n, "cases") {
        let f = str_of(&c["fn"]);
        let args = c["args"].as_array().expect("args");
        let got = std::panic::catch_unwind(|| api::normalize(f, args));
        match got {
            Ok(v) => r.json(
                format!("{f}({})", Value::Array(args.clone())),
                &c["out"],
                &v,
                Cmp::default(),
            ),
            Err(_) => {
                r.case();
                r.fail(
                    format!("{f}({})", Value::Array(args.clone())),
                    vec!["panicked".into()],
                );
            }
        }
    }
    r.finish("normalize.json");
}

// ==================================================================== prompts (B)

#[test]
fn prompts_match_word_for_word() {
    let p = helpers("prompts.json");
    let mut r = Report::default();
    for c in rows(&p, "run_command") {
        r.eq(
            format!("run_command({}, {})", c["id"], c["date"]),
            str_of(&c["out"]).to_owned(),
            api::run_command(str_of(&c["id"]), str_of(&c["date"])),
        );
    }
    for c in rows(&p, "discuss_prompt") {
        let share = f64_of(&c["share"]);
        if share.is_nan() {
            continue; // TS prints «Доля: NaN %»; the UI never sends NaN (slider), Rust may clamp it — not a contract.
        }
        let got = api::discuss_prompt(
            str_of(&c["region"]),
            str_of(&c["side"]),
            share,
            str_of(&c["text"]),
            str_of(&c["date"]),
        );
        r.eq(
            format!("discuss_prompt(share {share}, text {})", c["text"]),
            str_of(&c["out"]).to_owned(),
            got,
        );
    }
    for c in rows(&p, "macro_chat_title") {
        r.eq(
            format!("macro_chat_title({})", c["date"]),
            str_of(&c["out"]).to_owned(),
            api::macro_chat_title(str_of(&c["date"])),
        );
    }
    r.finish("prompts.json");
}
