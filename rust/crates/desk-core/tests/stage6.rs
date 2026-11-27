// GATE: compiled out until desk-core's stage-6 modules (settings, preflight,
// schedule, wizard — docs/rust/CONTRACTS.md «этап 6») land. Delete the next
// line then and fix the names in `mod api`. (`any()` is always false; a named
// feature would need a Cargo.toml entry, and `golden` is on in CI.)
#![cfg(any())]
//! Stage 6, the pure half, against the OLD app's answers
//! (rust/fixtures/golden/expected/stage6/, made by
//! rust/fixtures/tools/capture-stage6.ts):
//!
//! * checks.json — `buildChecks` over 42 machines → Rust `build_checks`, compared
//!   on the MVP checks by (id, state, fix kind, task) through the id map below;
//! * wizard.json — page order, page per check, done / skip / limitation rules,
//!   which tasks a page starts by itself;
//! * settings_import.json — old config.json → `import_electron`;
//! * schedule.json — NOT the old app (it had fixed times): TZ №21 + CONTRACTS,
//!   `planned_runs` and `due_window`.
//!
//! Proposed where the contract is silent (see the report and TEST_PLAN «этап 6»):
//! `preflight::Facts` deserializes from the old `PreflightFacts` JSON (camelCase,
//! unknown fields ignored), so a case's facts feed both apps unchanged; the four
//! old tvmcp checks (tvmcpClone / tvmcpRuntime / tvmcpBoot / toolsPython) fold
//! into one `tvmcp` check = the worst of them, with the fix of the first one in
//! old order that has a fix; the old `explain` fix has no Rust `Fix` (the note
//! is the detail), so it maps to «no fix».

mod common;

use common::*;
use serde_json::{json, Value};

fn stage6(name: &str) -> Value {
    read_json(&golden().join("expected/stage6").join(name))
}

fn rows<'a>(v: &'a Value, t: &str) -> &'a Vec<Value> {
    v[t].as_array().unwrap_or_else(|| panic!("table {t}"))
}

/// base deep-merged with patch: objects key by key, everything else replaces.
fn merge(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            let mut out = b.clone();
            for (k, v) in p {
                let merged = out.get(k).map_or_else(|| v.clone(), |old| merge(old, v));
                out.insert(k.clone(), merged);
            }
            Value::Object(out)
        }
        (_, p) => p.clone(),
    }
}

mod api {
    use chrono::{DateTime, NaiveDate, Utc};
    use serde_json::Value;

    use super::kebab;
    use desk_core::preflight::{Check, Facts, Fix};
    use desk_core::settings::Settings;

    pub fn facts(v: Value) -> Facts {
        serde_json::from_value(v).expect("preflight::Facts reads the old PreflightFacts JSON")
    }

    /// The old settings the old checks ran with, as Rust settings.
    pub fn settings_from_old(v: &Value) -> Settings {
        desk_core::settings::import_electron(v)
    }

    pub fn build_checks(f: &Facts, s: &Settings) -> Vec<Check> {
        desk_core::preflight::build_checks(f, s)
    }

    /// (id, state, fix kind, task) with the Rust spelling: ids snake_case,
    /// states ok|warn|fail|skipped, fix kinds task|copy|launch_tv|pick_folder|set_cdp_port.
    pub fn view(c: &Check) -> (String, String, Option<String>, Option<String>) {
        let id = snake(&kebab(&c.id));
        let state = kebab(&c.state);
        let (kind, task) = match &c.fix {
            None => (None, None),
            Some(Fix::Task(t)) => (Some("task".into()), Some(snake(&kebab(t)))),
            Some(Fix::Copy { .. }) => (Some("copy".into()), None),
            Some(Fix::LaunchTradingView) => (Some("launch_tv".into()), None),
            Some(Fix::PickFolder) => (Some("pick_folder".into()), None),
            Some(Fix::SetCdpPort(_)) => (Some("set_cdp_port".into()), None),
        };
        (id, state, kind, task)
    }

    pub fn port_of(c: &Check) -> Option<u16> {
        match &c.fix {
            Some(Fix::SetCdpPort(p)) => Some(*p),
            _ => None,
        }
    }

    pub fn snake(s: &str) -> String {
        s.replace('-', "_")
    }

    // wizard
    pub fn step_ids() -> Vec<String> {
        desk_core::wizard::STEPS.iter().map(kebab).collect()
    }
    pub fn visible_steps(checks: &[Check]) -> Vec<String> {
        desk_core::wizard::visible_steps(checks)
            .into_iter()
            .map(|s| kebab(&s))
            .collect()
    }
    pub fn step_state(step: &str, checks: &[Check]) -> String {
        kebab(&desk_core::wizard::step_state(step_id(step), checks))
    }
    pub fn step_done(step: &str, checks: &[Check]) -> bool {
        desk_core::wizard::step_done(step_id(step), checks)
    }
    pub fn apply_skip(skipped: &[String], step: &str, on: bool) -> Vec<String> {
        desk_core::wizard::apply_skip(skipped, step_id(step), on)
    }
    pub fn limitation_keys(skipped: &[String]) -> Vec<String> {
        desk_core::wizard::limitation_keys(skipped)
    }
    /// The tasks a page starts by itself (old `autoFixes` with an empty task table).
    pub fn auto_tasks(step: &str, checks: &[Check], started: &[String]) -> Vec<String> {
        desk_core::wizard::auto_tasks(step_id(step), checks, started)
            .into_iter()
            .map(|t| snake(&kebab(&t)))
            .collect()
    }
    fn step_id(s: &str) -> desk_core::wizard::StepId {
        desk_core::wizard::StepId::parse(s).unwrap_or_else(|| panic!("step {s}"))
    }

    // settings
    pub fn import(raw: &Value) -> Value {
        serde_json::to_value(desk_core::settings::import_electron(raw))
            .expect("Settings serializes")
    }

    // schedule
    pub fn planned(d: &str) -> Vec<(String, String)> {
        let date = NaiveDate::parse_from_str(d, "%Y-%m-%d").expect("date");
        desk_core::schedule::planned_runs(date, &Settings::default())
            .into_iter()
            .map(|p| (kebab(&p.window), p.at_local.format("%H:%M").to_string()))
            .collect()
    }
    pub fn due(now: DateTime<Utc>) -> Option<String> {
        desk_core::schedule::due_window(now, &Settings::default()).map(|w| kebab(&w))
    }
}

/// Old check id → Rust check id (CONTRACTS «этап 6»); None = dropped in the MVP.
fn rust_id(old: &str) -> Option<&'static str> {
    Some(match old {
        "vault" => "vault",
        "claude" => "claude",
        "uv" => "uv",
        "tvmcpClone" | "tvmcpRuntime" | "tvmcpBoot" | "toolsPython" => "tvmcp",
        "tvDesktop" => "tradingview",
        "cdpPort" => "cdp",
        "tvSession" => "tv_session",
        "scheduler" => "scheduler",
        "timezone" => "timezone",
        _ => return None,
    })
}

fn rust_state(old: &str) -> &'static str {
    match old {
        "ok" => "ok",
        "warn" => "warn",
        "fail" => "fail",
        _ => "skipped",
    }
}

fn rust_fix(fix: &Value) -> (Option<String>, Option<String>) {
    match fix["kind"].as_str() {
        Some("run-task") => (
            Some("task".into()),
            fix["task"].as_str().map(|t| t.replace('-', "_")),
        ),
        Some("copy") => (Some("copy".into()), None),
        Some("launch-tv") => (Some("launch_tv".into()), None),
        Some("pick-file") => (Some("pick_folder".into()), None),
        Some("set-port") => (Some("set_cdp_port".into()), None),
        _ => (None, None), // explain, set-flag (knowledge base only), none
    }
}

fn severity(s: &str) -> u8 {
    match s {
        "fail" => 3,
        "warn" => 2,
        "skipped" => 1,
        _ => 0,
    }
}

/// The old checks of a case, in the Rust shape: one row per Rust id, the tvmcp
/// rows folded (worst state, first fix in old order).
fn expected_mvp(old: &[Value]) -> Vec<(String, String, Option<String>, Option<String>)> {
    let mut out: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
    for c in old {
        let Some(id) = rust_id(str_of(&c["id"])) else {
            continue;
        };
        let state = rust_state(str_of(&c["status"])).to_owned();
        let (kind, task) = rust_fix(&c["fix"]);
        match out.iter_mut().find(|r| r.0 == id) {
            Some(r) => {
                if severity(&state) > severity(&r.1) {
                    r.1 = state;
                }
                if r.2.is_none() && kind.is_some() {
                    r.2 = kind;
                    r.3 = task;
                }
            }
            None => out.push((id.to_owned(), state, kind, task)),
        }
    }
    out
}

#[test]
fn checks_match_the_old_diagnosis() {
    let g = stage6("checks.json");
    let base_facts = &g["base_facts"];
    let base_settings = &g["base_settings"];
    let mut r = Report::default();
    for c in rows(&g, "cases") {
        let name = str_of(&c["name"]);
        let facts = api::facts(merge(base_facts, &c["facts_patch"]));
        let settings = api::settings_from_old(&merge(base_settings, &c["settings_patch"]));
        let got: Vec<_> = api::build_checks(&facts, &settings)
            .iter()
            .map(api::view)
            .filter(|v| v.0 != "claude_auth") // new in Rust, no old twin
            .collect();
        let want = expected_mvp(c["checks"].as_array().expect("checks"));
        r.eq(format!("{name}: {}", str_of(&c["about"])), want, got);
    }
    r.finish("stage6/checks.json");
}

/// Order: the old WIZARD_STEPS order (vault → claude → tvmcp → tradingview →
/// session → schedule, checks in page order), which is also CONTRACTS' CheckId
/// order. The old Settings → «Диагностика» list was runtime-first (tvmcp …,
/// claude, tradingview, cdp, then vault …) — `order` in checks.json; QA
/// proposes one order for both (quirk S5 in TEST_PLAN).
#[test]
fn check_order_is_the_wizard_order() {
    let g = stage6("checks.json");
    let w = stage6("wizard.json");
    let c = rows(&g, "cases")
        .iter()
        .find(|c| c["name"] == "external_dev_ok")
        .expect("case");
    let facts = api::facts(merge(&g["base_facts"], &c["facts_patch"]));
    let s = api::settings_from_old(&g["base_settings"]);
    let got: Vec<String> = api::build_checks(&facts, &s)
        .iter()
        .map(|c| api::view(c).0)
        .filter(|id| id != "claude_auth")
        .collect();
    let present: Vec<&str> = c["order"]
        .as_array()
        .expect("order")
        .iter()
        .filter_map(|x| rust_id(str_of(x)))
        .collect();
    let mut want: Vec<String> = Vec::new();
    for step in w["wizard_steps"].as_array().expect("steps") {
        for id in step["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .filter_map(|x| rust_id(str_of(x)))
        {
            if present.contains(&id) && !want.iter().any(|x| x == id) {
                want.push(id.to_owned());
            }
        }
    }
    assert_eq!(got, want);
}

/// The old non-negotiable rule: nothing network or destructive without a
/// press. A check's fix is data — building checks starts nothing — and the
/// only fixes that act are a task (button), launching TradingView, picking a
/// folder and switching the port. `copy` never runs.
#[test]
fn fixes_are_the_old_kinds_and_the_port_is_the_free_one() {
    let g = stage6("checks.json");
    let allowed = ["task", "copy", "launch_tv", "pick_folder", "set_cdp_port"];
    for c in rows(&g, "cases") {
        let facts = api::facts(merge(&g["base_facts"], &c["facts_patch"]));
        let s = api::settings_from_old(&merge(&g["base_settings"], &c["settings_patch"]));
        for ch in api::build_checks(&facts, &s) {
            let v = api::view(&ch);
            if let Some(k) = &v.2 {
                assert!(allowed.contains(&k.as_str()), "{}: {k}", c["name"]);
            }
            if let Some(p) = api::port_of(&ch) {
                assert_eq!(
                    Some(p as u64),
                    merge(&g["base_facts"], &c["facts_patch"])["freePort"].as_u64(),
                    "set-port offers the free port"
                );
            }
        }
    }
}

#[test]
fn wizard_pages_and_skips_match_the_old_wizard() {
    let w = stage6("wizard.json");
    let g = stage6("checks.json");
    assert_eq!(
        api::step_ids(),
        w["step_ids_mvp"]
            .as_array()
            .expect("ids")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect::<Vec<_>>(),
        "vault → claude → tvmcp → tradingview → session → schedule"
    );
    let mut r = Report::default();
    for p in rows(&w, "pages") {
        let machine = str_of(&p["machine"]);
        let c = rows(&g, "cases")
            .iter()
            .find(|c| c["name"] == machine)
            .expect("machine");
        let facts = api::facts(merge(&g["base_facts"], &c["facts_patch"]));
        let s = api::settings_from_old(&merge(&g["base_settings"], &c["settings_patch"]));
        let checks = api::build_checks(&facts, &s);
        let vis: Vec<String> = p["visible"]
            .as_array()
            .expect("visible")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect();
        r.eq(
            format!("{machine}: visible pages"),
            vis,
            api::visible_steps(&checks),
        );
        for st in p["steps"].as_array().expect("steps") {
            let step = str_of(&st["step"]);
            r.eq(
                format!("{machine}/{step}: state"),
                rust_state(str_of(&st["status"])).to_owned(),
                api::step_state(step, &checks),
            );
            r.eq(
                format!("{machine}/{step}: done"),
                st["done"].as_bool().expect("done"),
                api::step_done(step, &checks),
            );
        }
    }
    for a in rows(&w, "apply_skip") {
        let input: Vec<String> = a["skipped_in"]
            .as_array()
            .expect("in")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect();
        let want: Vec<String> = a["out"]
            .as_array()
            .expect("out")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect();
        r.eq(
            format!("apply_skip({input:?}, {}, {})", a["id"], a["on"]),
            want,
            api::apply_skip(&input, str_of(&a["id"]), a["on"].as_bool().expect("on")),
        );
    }
    for (k, v) in w["limitation_keys"].as_object().expect("keys") {
        let skipped: Vec<String> = match k.as_str() {
            "session_schedule" => vec!["schedule".into(), "session".into()],
            "junk" => vec!["nope".into(), "tvmcp".into()],
            _ => vec![],
        };
        let want: Vec<String> = v
            .as_array()
            .expect("v")
            .iter()
            .map(|x| str_of(x).to_owned())
            .collect();
        r.eq(
            format!("limitation_keys({k})"),
            want,
            api::limitation_keys(&skipped),
        );
    }
    r.finish("stage6/wizard.json");
}

/// Which tasks a page starts by itself: provision_runtime (tvmcp page) and
/// session_check (session page), once per wizard session; never vault_create,
/// claude_login, claude_install or scheduler_register.
#[test]
fn auto_tasks_are_the_old_ones() {
    let w = stage6("wizard.json");
    let g = stage6("checks.json");
    for a in rows(&w, "auto_fixes") {
        if a["tasks"].as_object().is_some_and(|t| !t.is_empty())
            || a.get("commands").is_some_and(|c| !c.is_null())
        {
            continue; // running / finished / refused-plan states are desk-app's (the task table lives there)
        }
        let c = rows(&g, "cases")
            .iter()
            .find(|c| c["name"] == a["machine"])
            .expect("machine");
        let facts = api::facts(merge(&g["base_facts"], &c["facts_patch"]));
        let s = api::settings_from_old(&merge(&g["base_settings"], &c["settings_patch"]));
        let checks = api::build_checks(&facts, &s);
        let started: Vec<String> = a["started"]
            .as_array()
            .expect("started")
            .iter()
            .map(|x| str_of(x).replace('-', "_"))
            .collect();
        let want: Vec<String> = a["out"]
            .as_array()
            .expect("out")
            .iter()
            .map(|x| str_of(x).replace('-', "_"))
            .collect();
        assert_eq!(
            api::auto_tasks(str_of(&a["step"]), &checks, &started),
            want,
            "{}",
            a["about"]
        );
    }
}

#[test]
fn electron_config_imports_like_the_old_reader() {
    let g = stage6("settings_import.json");
    let mut r = Report::default();
    for c in rows(&g, "cases") {
        let got = api::import(&c["raw"]);
        let want = &c["rust"];
        let pick = |v: &Value| {
            json!({
                "vault": v["vault"], "tvmcp_path": v["tvmcp_path"], "cdp_port": v["cdp_port"], "theme": v["theme"],
                "scale": v["scale"], "macro_mcp": v["macro_mcp"], "setup_skipped": v["setup_skipped"],
            })
        };
        r.json(
            format!("{}: {}", str_of(&c["name"]), str_of(&c["about"])),
            &pick(want),
            &pick(&got),
            Cmp::default(),
        );
        // Secrets never travel: not the encrypted blobs, not a flag that one exists.
        let text = got.to_string();
        r.case();
        if text.contains("djEw") || text.contains("Enc\"") {
            r.fail(
                format!("{}: a secret reached the settings", c["name"]),
                vec![text],
            );
        }
    }
    r.finish("stage6/settings_import.json");
}

#[test]
fn planned_runs_follow_the_exchanges() {
    let s = stage6("schedule.json");
    let mut r = Report::default();
    for c in rows(&s, "planned_runs") {
        let want: Vec<(String, String)> = c["out"]
            .as_array()
            .expect("out")
            .iter()
            .map(|p| {
                (
                    str_of(&p["window"]).to_owned(),
                    str_of(&p["at_local"]).to_owned(),
                )
            })
            .collect();
        r.eq(
            format!("planned_runs({})", c["date"]),
            want,
            api::planned(str_of(&c["date"])),
        );
    }
    // The Task Scheduler is registered on the union: every planned minute of 2026–2027 is a trigger.
    let triggers: Vec<String> = s["triggers"]
        .as_array()
        .expect("t")
        .iter()
        .map(|x| str_of(x).to_owned())
        .collect();
    let mut d = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("d");
    while d < chrono::NaiveDate::from_ymd_opt(2028, 1, 1).expect("d") {
        for (w, at) in api::planned(&d.format("%Y-%m-%d").to_string()) {
            r.eq(
                format!("{d}: {w} {at} is a registered trigger"),
                true,
                triggers.contains(&format!("{w} {at}")),
            );
        }
        d = d.succ_opt().expect("d");
    }
    r.finish("stage6/schedule.json planned_runs");
}

/// «due_window decides»: the 16:00 trigger in a 15:00 week (2026-10-26) is not
/// due → `run` exits 0 without work; the 15:00 one is.
#[test]
fn due_window_decides() {
    let s = stage6("schedule.json");
    let mut r = Report::default();
    for c in rows(&s, "due_window") {
        let now = chrono::DateTime::parse_from_rfc3339(str_of(&c["utc"]))
            .expect("utc")
            .with_timezone(&chrono::Utc);
        r.eq(
            format!("due_window({})", c["kyiv"]),
            c["due_window"].as_str().map(str::to_owned),
            api::due(now),
        );
    }
    r.finish("stage6/schedule.json due_window");
}
