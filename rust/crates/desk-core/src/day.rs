//! The pure part of `loadDay()` / `listDays()` (`src/main/macro-state.ts`):
//! bytes in, a [`Day`] out. `serde_json::to_string(&day)` is the JSON that
//! `scripts/preview/dump.ts` writes for the same vault — same field names,
//! same `null`s, same array order (and, with `serde_json/preserve_order`,
//! the same key order) — which is what the golden tests check.
//!
//! Deviations from the TS, on purpose:
//!
//! - **No screenshots inside.** `loadDay` itself never carried pixels (the
//!   renderer asked `readShot` for a base64 `data:` URL per picture); in Rust
//!   the picture is a [`crate::vault::ShotRef`] (`{"$shot": path, "bytes": N}`,
//!   see [`crate::shots::shot_ref`]) and the UI decodes the PNG off the frame.
//! - **Note paths use `/`.** `path.join` gave `Макро-обзоры\…` on Windows and
//!   `Макро-обзоры/…` on Linux, where the golden files are made; Rust always
//!   gives the vault-relative form with `/`.
//! - Where the TS would throw on junk (`null` inside a list it maps over), the
//!   junk is read as an empty object, so a broken file never fails the day.

use crate::js;
use crate::names::{self, StateKind};
use crate::normalize::{self, LESSONS_TAIL};
use crate::stats;
use crate::vault::*;
use crate::zones::Zones;
use chrono::NaiveDate;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// What `loadDay` reads, handed in by whoever did the reading.
///
/// Keys are **vault-relative paths with `/`** (`indices-kb/state/scale_….json`,
/// `indices-kb/journal/lessons.md`, `indices-kb/config/scale.json`,
/// `Макро-обзоры/2026-10-08 Европа.md`). A missing key is a missing file.
/// [`day_files`] lists every path a day may read; for the two notes only
/// their presence matters, so their bytes may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DayInput {
    pub files: BTreeMap<String, Vec<u8>>,
}

impl DayInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add (or replace) a file.
    pub fn insert(&mut self, rel: impl Into<String>, bytes: impl Into<Vec<u8>>) {
        self.files.insert(rel.into(), bytes.into());
    }

    /// Forget a file (it was deleted).
    pub fn remove(&mut self, rel: &str) {
        self.files.remove(rel);
    }

    pub fn contains(&self, rel: &str) -> bool {
        self.files.contains_key(rel)
    }

    /// `readJson`: parsed, BOM-tolerant; `None` when missing or broken.
    fn json(&self, rel: &str) -> Option<Value> {
        self.files.get(rel).and_then(|b| js::parse_json(b))
    }

    fn state_json(&self, kind: StateKind, date: NaiveDate) -> Option<Value> {
        self.json(&names::state_path(&names::state_file(kind, date)))
    }

    /// `readText`: UTF-8 with replacement characters, BOM kept.
    fn text(&self, rel: &str) -> Option<String> {
        self.files
            .get(rel)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
}

/// Every vault-relative path [`load_day`] looks at for `date`.
pub fn day_files(date: NaiveDate) -> Vec<String> {
    let mut v = Vec::with_capacity(24);
    for r in Region::ALL {
        for k in [
            StateKind::Scale(r),
            StateKind::Verdict(r),
            StateKind::Macro(r),
            StateKind::Shots(r),
            StateKind::HumanRead(r),
            StateKind::Remember(r),
            StateKind::Drift(r),
            StateKind::ShotsReview(r),
        ] {
            v.push(names::state_path(&names::state_file(k, date)));
        }
        v.push(names::region_note(date, r));
    }
    for k in [
        StateKind::Review,
        StateKind::ReviewAgent,
        StateKind::DayAnswers,
        StateKind::Week,
    ] {
        v.push(names::state_path(&names::state_file(k, date)));
    }
    v.push(names::state_path(names::IDEAS_FILE));
    v.push(names::journal_path(names::LESSONS_FILE));
    v.push(names::journal_path(names::SCORES_FILE));
    v.push(names::config_path());
    v.push(names::review_note(date));
    v
}

/// `RegionSnapshot`: one window of the day as the files have it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionSnapshot {
    pub region: Region,
    /// Older scale files (v1) get the percentages the screen draws; a file
    /// without a result at all is half-written and is `None` («not yet»).
    pub scale: Option<Raw<ScaleFile>>,
    /// The agent's verdict: the verdict file, else the copy in the collect file.
    pub verdict: Option<Raw<MacroVerdict>>,
    pub collect: Option<Raw<MacroFile>>,
    /// Vault-relative path of the day's note, when it exists.
    pub note: Option<String>,
    /// The chart screenshots of this window; `None` when the skill wrote none.
    pub shots: Option<Shots>,
    /// The owner's own read of the window, when he gave one.
    pub human: Option<HumanRead>,
    /// «Сегодня помни» — `None` when the skill wrote nothing for this window.
    pub remember: Option<Remember>,
    /// «Что изменилось» — `None` when the skill wrote nothing; drawn only with signals.
    pub drift: Option<Drift>,
}

/// The 20:00 pictures of each window: IB, IB1/IB2, the first break, the morning levels.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewShots {
    pub eu: Option<Shots>,
    pub us: Option<Shots>,
}

/// `MacroDay.review`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewDay {
    pub mech: Option<Raw<ReviewFile>>,
    pub agent: Option<Raw<ReviewAgent>>,
    /// Vault-relative path of «<D> Итог дня.md», when it exists.
    pub note: Option<String>,
    pub shots: ReviewShots,
}

/// `MacroDay`: everything the macro-review screen shows for one date.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    pub eu: RegionSnapshot,
    pub us: RegionSnapshot,
    pub review: ReviewDay,
    /// The owner's evening answers, when he gave them.
    pub answers: Option<DayAnswers>,
    /// Newest lessons first, at most [`LESSONS_TAIL`].
    pub lessons: Vec<String>,
    /// Hit rate per factor over the last 30 days before (and including) this date.
    pub stats: Vec<FactorStat>,
    /// Factor id → readable name, from `config/scale.json` (values as written there).
    pub names: Map<String, Value>,
    pub zones: Zones,
    /// «Неделя в пять строк»: `week_<D>.json` of this date (written on a Friday).
    pub week: Option<Week>,
    /// «Идеи на проверке»: the one dateless list.
    pub ideas: Option<Ideas>,
    /// «О тебе» from the day's review json; empty when there is none.
    pub trader: Vec<String>,
}

/// `MacroDay`, by its TS name.
pub type MacroDay = Day;

impl Day {
    pub fn region(&self, r: Region) -> &RegionSnapshot {
        match r {
            Region::Eu => &self.eu,
            Region::Us => &self.us,
        }
    }

    /// The readable name of a factor id, when the config gives one as a string.
    pub fn factor_name(&self, id: &str) -> Option<&str> {
        self.names.get(id).and_then(Value::as_str)
    }

    /// `traderLines`: the lines «О тебе» shows in the evening — the week's on
    /// a Friday (when it has any), else the review's.
    pub fn trader_lines(&self) -> &[String] {
        let date = self.date.format("%Y-%m-%d").to_string();
        match &self.week {
            Some(w) if names::is_friday(&date) && !w.trader.is_empty() => &w.trader,
            _ => &self.trader,
        }
    }

    /// Every screenshot path the day mentions (windows, then the evening), so
    /// the app can decode them ahead of the frame.
    pub fn shot_paths(&self) -> Vec<&str> {
        [
            &self.eu.shots,
            &self.us.shots,
            &self.review.shots.eu,
            &self.review.shots.us,
        ]
        .into_iter()
        .flatten()
        .flat_map(|s| s.shots.iter().map(|x| x.path.as_str()))
        .collect()
    }
}

fn region_snapshot(input: &DayInput, date: NaiveDate, ds: &str, region: Region) -> RegionSnapshot {
    let scale = input.state_json(StateKind::Scale(region), date);
    let verdict_file = input.state_json(StateKind::Verdict(region), date);
    let collect_raw = input.state_json(StateKind::Macro(region), date);
    let shots = input.state_json(StateKind::Shots(region), date);
    let human = input.state_json(StateKind::HumanRead(region), date);
    let remember = input.state_json(StateKind::Remember(region), date);
    let drift = input.state_json(StateKind::Drift(region), date);
    // `note` copies the verdict into the collect file too; the verdict file is
    // the one the agent writes, so it wins when both exist.
    let verdict = match verdict_file.filter(|v| !v.is_null()) {
        Some(v) => Some(v),
        None => js::prop(collect_raw.as_ref(), "verdict")
            .filter(|v| js::truthy(v))
            .cloned(),
    };
    let note = names::region_note(date, region);
    let null = Value::Null;
    RegionSnapshot {
        region,
        scale: scale.as_ref().and_then(normalize::normalize_scale),
        verdict: verdict.map(Raw::new),
        collect: collect_raw.as_ref().and_then(normalize::trim_collect),
        note: input.contains(&note).then_some(note),
        shots: normalize::normalize_shots(shots.as_ref().unwrap_or(&null), region),
        human: normalize::normalize_human_read(human.as_ref().unwrap_or(&null), ds, region),
        // Note 23 §2–3: what the skill picked out of its memory for this window. Read-only here.
        remember: normalize::normalize_remember(remember.as_ref().unwrap_or(&null), ds, region),
        drift: normalize::normalize_drift(drift.as_ref().unwrap_or(&null), ds, region),
    }
}

/// Factor id → name from `config/scale.json`: `for (r of Object.values(cfg.regions))
/// for (f of r.factors ?? []) names[f.id] = f.name`. Later regions win.
fn factor_names(cfg: Option<&Value>) -> Map<String, Value> {
    let mut names = Map::new();
    let regions: Vec<&Value> = match js::prop(cfg, "regions") {
        Some(Value::Object(m)) => m.values().collect(),
        Some(Value::Array(a)) => a.iter().collect(),
        _ => Vec::new(),
    };
    for r in regions {
        let Some(Value::Array(factors)) = js::nullish(js::prop(Some(r), "factors")) else {
            continue;
        };
        // A `null` in the list makes the TS throw; here it is skipped.
        for f in factors.iter().filter(|f| f.is_object()) {
            // `names[f.id]`: the key is String(f.id) ("undefined" when absent).
            let id = match js::prop(Some(f), "id") {
                Some(v) => js::to_string(v),
                None => "undefined".into(),
            };
            match js::prop(Some(f), "name") {
                Some(n) => {
                    names.insert(id, n.clone());
                }
                // `= undefined`: JSON.stringify leaves the key out.
                None => {
                    names.shift_remove(&id);
                }
            }
        }
    }
    js::order_keys(&mut names);
    names
}

/// `loadDay(vault, date)` without the disk: the day as `input` has it.
pub fn load_day(input: &DayInput, date: NaiveDate) -> Day {
    let ds = date.format("%Y-%m-%d").to_string();
    let null = Value::Null;
    let mech = input.state_json(StateKind::Review, date);
    let agent = input.state_json(StateKind::ReviewAgent, date);
    let answers = input.state_json(StateKind::DayAnswers, date);
    let week = input.state_json(StateKind::Week, date);
    let ideas = input.json(&names::state_path(names::IDEAS_FILE));
    let lessons = input.text(&names::journal_path(names::LESSONS_FILE));
    let scores = input.text(&names::journal_path(names::SCORES_FILE));
    let cfg = input.json(&names::config_path());
    let review_note = names::review_note(date);
    let review_shots = |r: Region| {
        normalize::normalize_shots(
            input
                .state_json(StateKind::ShotsReview(r), date)
                .as_ref()
                .unwrap_or(&null),
            r,
        )
    };
    let trader = match js::prop(mech.as_ref(), "trader") {
        // Blank lines are dropped; the kept ones are not trimmed (as in the TS).
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| !js::trim(s).is_empty())
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    Day {
        date,
        eu: region_snapshot(input, date, &ds, Region::Eu),
        us: region_snapshot(input, date, &ds, Region::Us),
        review: ReviewDay {
            mech: mech.filter(|v| !v.is_null()).map(Raw::new),
            agent: agent.filter(|v| !v.is_null()).map(Raw::new),
            note: input.contains(&review_note).then_some(review_note),
            shots: ReviewShots {
                eu: review_shots(Region::Eu),
                us: review_shots(Region::Us),
            },
        },
        answers: normalize::normalize_day_answers(answers.as_ref().unwrap_or(&null), &ds),
        lessons: lessons
            .filter(|s| !s.is_empty())
            .map(|s| normalize::lessons_tail(&s, LESSONS_TAIL))
            .unwrap_or_default(),
        stats: scores
            .filter(|s| !s.is_empty())
            .map(|s| stats::factor_stats(&s, date))
            .unwrap_or_default(),
        names: factor_names(cfg.as_ref()),
        zones: Zones::from_config(cfg.as_ref()),
        week: normalize::normalize_week(week.as_ref().unwrap_or(&null)),
        ideas: normalize::normalize_ideas(ideas.as_ref().unwrap_or(&null)),
        trader,
    }
}

/// `listDays`: every date with a scale, macro or review file, newest first.
/// The dates are only shaped like dates, as in the TS.
pub fn list_days<'a>(state_names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut days: Vec<String> = state_names
        .into_iter()
        .filter(|n| n.starts_with("scale_") || n.starts_with("macro_") || n.starts_with("review_"))
        .filter_map(names::date_of_state_file)
        .map(str::to_owned)
        .collect();
    days.sort_unstable_by(|a, b| b.cmp(a));
    days.dedup();
    days
}

/// `MacroIndex`: the days of the vault the desk is reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MacroIndex {
    /// Vault path the desk is reading; empty when not configured.
    pub vault: String,
    pub ok: bool,
    /// `no-vault` | `no-state-dir`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Dates that have any macro file, newest first.
    pub days: Vec<String>,
    /// Reading the demo vault: invented days; nothing may be sent to the agent then.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demo: Option<bool>,
}

impl MacroIndex {
    /// The index of `vault` given the names in its `indices-kb/state/`, or
    /// `None` when that folder could not be listed.
    pub fn new<'a>(vault: &str, state_names: Option<impl IntoIterator<Item = &'a str>>) -> Self {
        let fail = |e: &str| MacroIndex {
            vault: vault.to_owned(),
            ok: false,
            error: Some(e.into()),
            days: Vec::new(),
            demo: None,
        };
        if js::trim(vault).is_empty() {
            return fail("no-vault");
        }
        match state_names {
            None => fail("no-state-dir"),
            Some(n) => MacroIndex {
                vault: vault.to_owned(),
                ok: true,
                error: None,
                days: list_days(n),
                demo: None,
            },
        }
    }

    /// The days that are real calendar dates (a file named `scale_2026-99-99_…` is listed but cannot be opened).
    pub fn dates(&self) -> Vec<NaiveDate> {
        self.days
            .iter()
            .filter_map(|d| names::parse_date(d).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn d(s: &str) -> NaiveDate {
        names::parse_date(s).unwrap()
    }

    const STATE: &str = "indices-kb/state/";

    /// The vault of macro-state.test.ts, as bytes.
    fn vault() -> DayInput {
        let mut v = DayInput::new();
        let st = |v: &mut DayInput, name: &str, body: String| {
            v.insert(format!("{STATE}{name}"), body.into_bytes())
        };
        let scale = json!({
            "region": "eu", "date": "2026-10-08", "asof": "2026-10-08 09:35", "sym": "FDAX", "version": "v3.1",
            "factors": [{"id": "bias_a", "name": "Точка A / BIAS", "group": "контекст дня", "side": "SHORT", "strength": 1, "share": 12.4, "points": 12.4}],
            "result": {"long": 7.4, "short": 65.5, "undecided": 27.1, "needle": -58, "zone": "сильный шорт", "side": "SHORT"},
            "light": {"color": "yellow", "label": "🟡 осторожно", "risk_pct": 0.5}
        });
        st(
            &mut v,
            "scale_2026-10-08_eu.json",
            format!("\u{FEFF}{scale}"),
        );
        st(&mut v, "macro_2026-10-08_eu.json", json!({"asof_eet": "09:31", "headlines": {"count": 284, "relevant": 138, "tone_sum": -5, "tags": {}, "feeds": [1, 2, 3]}, "markets": [{"symbol": "X", "label": "FDAX", "chg_pct": -0.5, "main": true, "last": 1}], "verdict": {"verdict": "NEUTRAL"}}).to_string());
        st(
            &mut v,
            "macro_2026-10-08_eu_verdict.json",
            json!({"verdict": "SHORT", "confidence": "medium"}).to_string(),
        );
        st(
            &mut v,
            "review_2026-10-07.json",
            json!({"date": "2026-10-07", "regions": {}}).to_string(),
        );
        st(&mut v, "scale_backtest_eu.json", "{}".into());
        st(&mut v, "bias_2026-10-09.json", "{}".into());
        st(&mut v, "scale_2026-10-09_us.json", "{not json".into());
        v.insert("Макро-обзоры/2026-10-08 Европа.md", "# note");
        let shots = json!({
            "date": "2026-10-08", "region": "eu", "asof": "2026-10-08 09:40", "ok": true, "error": null,
            "shots": [{"name": "m15", "tf": "M15", "title": "M15 — ночь", "path": "indices-kb/shots/2026-10-08/macro_eu/m15.png", "verify_ok": true,
                "legend": [{"key": "NYM", "label": "полночь Нью-Йорка", "kind": "open", "color": "#9e9e9e"}]}]
        });
        st(
            &mut v,
            "shots_2026-10-08_eu.json",
            format!("\u{FEFF}{shots}"),
        );
        st(&mut v, "shots_2026-10-08_us.json", "{\"ok\": tru".into());
        st(&mut v, "shots_2026-10-07_eu.json", json!({"date": "2026-10-07", "region": "eu", "asof": "", "ok": false, "error": "dry-run", "shots": []}).to_string());
        st(
            &mut v,
            "remember_2026-10-08_eu.json",
            format!(
                "\u{FEFF}{}",
                json!({"date": "2026-10-08", "region": "eu", "asof": "2026-10-08 09:31", "items": [{"kind": "lesson", "text": "В пилу ждать вторую манипуляцию", "why": "те же теги", "ref": "lessons.md 01.10", "tags": ["eu", "пила"]}], "conditions": {"tags": ["eu"]}, "similar": null, "lesson_candidates": 2, "enough_history": true})
            ),
        );
        st(
            &mut v,
            "remember_2026-10-08_us.json",
            "{\"items\": [".into(),
        );
        st(&mut v, "drift_2026-10-08_eu.json", json!({"date": "2026-10-08", "region": "eu", "days": 20, "enough_data": true, "signals": [{"kind": "phase", "text": "фаза сменилась: тренд → пила", "severity": "warn", "data": {}}]}).to_string());
        st(&mut v, "drift_2026-10-08_us.json", json!({"date": "2026-10-08", "region": "us", "days": 20, "enough_data": true, "signals": []}).to_string());
        st(&mut v, "week_2026-10-09.json", json!({"week_start": "2026-10-05", "week_end": "2026-10-09", "days": 5, "enough_data": true, "lines": ["1", "2", "3", "4", "5"], "stats": {}, "trader": ["из недели"], "share_proposal": null}).to_string());
        st(
            &mut v,
            "review_2026-10-09.json",
            json!({"date": "2026-10-09", "regions": {}, "trader": ["из дня", 7, ""]}).to_string(),
        );
        st(&mut v, "ideas.json", json!({"asof": "2026-10-09 20:10", "items": [{"id": "a", "date": "2026-10-01", "text": "идея", "side": "LONG", "region": "eu", "source": "chat", "confirmed": 3, "refuted": 0, "status": "proposed", "backtest": null, "last_check": "2026-10-08"}]}).to_string());
        v.insert(
            "indices-kb/journal/lessons.md",
            "- 2026-10-07: старый\n- 2026-10-08: новый\n",
        );
        v.insert("indices-kb/journal/scale_factor_scores.csv", "date,region,factor,group,share,side,strength,dir,hit\n2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1\n");
        v.insert("indices-kb/config/scale.json", json!({"zones": {"neutral": 15, "strong": 40}, "regions": {"eu": {"factors": [{"id": "bias_a", "name": "Точка A / BIAS"}]}}}).to_string());
        v
    }

    fn state_names(v: &DayInput) -> Vec<&str> {
        v.files
            .keys()
            .filter_map(|k| k.strip_prefix(STATE))
            .collect()
    }

    // ---- ported from macro-state.test.ts

    #[test]
    fn lists_macro_days_newest_first_and_ignores_backtests_and_other_files() {
        let v = vault();
        let idx = MacroIndex::new("/v", Some(state_names(&v)));
        assert_eq!(
            serde_json::to_value(&idx).unwrap(),
            json!({"vault": "/v", "ok": true, "days": ["2026-10-09", "2026-10-08", "2026-10-07"]})
        );
        assert_eq!(
            MacroIndex::new("  ", Some(state_names(&v)))
                .error
                .as_deref(),
            Some("no-vault")
        );
        assert_eq!(
            MacroIndex::new("/v", None::<Vec<&str>>).error.as_deref(),
            Some("no-state-dir")
        );
        // shots_/remember_/… files alone do not make a day.
        assert_eq!(
            list_days(["shots_2026-01-01_eu.json", "human_read_2026-01-02_eu.json"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn loads_a_day_scale_with_a_bom_the_verdict_file_over_the_copy_trimmed_headlines_the_note_journal(
    ) {
        let day = load_day(&vault(), d("2026-10-08"));
        assert_eq!(day.eu.scale.as_ref().unwrap().result.needle, Some(-58.0));
        assert_eq!(
            day.eu.verdict.as_ref().unwrap().verdict.as_deref(),
            Some("SHORT")
        );
        let c = day.eu.collect.as_ref().unwrap().raw();
        assert_eq!(
            c["headlines"],
            json!({"count": 284, "relevant": 138, "tone_sum": -5, "tags": {}})
        );
        // `kind: undefined` is left out of the JSON.
        assert_eq!(
            c["markets"],
            json!([{"label": "FDAX", "chg_pct": -0.5, "main": true}])
        );
        assert_eq!(
            day.eu.note.as_deref(),
            Some("Макро-обзоры/2026-10-08 Европа.md")
        );
        assert!(day.us.scale.is_none());
        assert!(day.review.mech.is_none());
        assert_eq!(day.lessons, ["2026-10-08: новый", "2026-10-07: старый"]);
        assert_eq!(
            serde_json::to_value(&day.stats).unwrap(),
            json!([{"region": "eu", "factor": "bias_a", "hits": 1, "total": 1}])
        );
        assert_eq!(day.factor_name("bias_a"), Some("Точка A / BIAS"));
    }

    #[test]
    fn reads_the_memory_cards_and_shows_nothing_where_there_is_nothing() {
        let v = vault();
        let day = load_day(&v, d("2026-10-08"));
        assert_eq!(
            serde_json::to_value(&day.eu.remember.as_ref().unwrap().items).unwrap(),
            json!([{"kind": "lesson", "text": "В пилу ждать вторую манипуляцию", "why": "те же теги", "ref": "lessons.md 01.10", "tags": ["eu", "пила"]}])
        );
        assert!(day.us.remember.is_none());
        let s = &day.eu.drift.as_ref().unwrap().signals[0];
        assert_eq!((s.kind.as_str(), s.severity.as_str()), ("phase", "warn"));
        assert!(day.us.drift.as_ref().unwrap().signals.is_empty());
        assert!(day.week.is_none());
        assert!(day.trader.is_empty());
        let idea = &day.ideas.as_ref().unwrap().items[0];
        assert_eq!(
            (idea.id.as_str(), idea.status, idea.confirmed),
            ("a", IdeaStatus::Proposed, 3)
        );
        let fri = load_day(&v, d("2026-10-09"));
        assert_eq!(fri.week.as_ref().unwrap().lines, ["1", "2", "3", "4", "5"]);
        assert_eq!(fri.week.as_ref().unwrap().trader, ["из недели"]);
        assert_eq!(fri.trader, ["из дня"]);
        assert_eq!(fri.trader_lines(), ["из недели"]);
        assert!(fri.eu.remember.is_none());
        assert!(fri.eu.drift.is_none());
    }

    #[test]
    fn survives_a_half_written_file() {
        let day = load_day(&vault(), d("2026-10-09"));
        assert!(day.us.scale.is_none());
    }

    #[test]
    fn reads_the_screenshots_file_of_a_window_and_tolerates_a_bom_or_garbage() {
        let v = vault();
        let day = load_day(&v, d("2026-10-08"));
        let s = day.eu.shots.as_ref().unwrap();
        assert!(s.ok);
        assert_eq!(s.shots[0].legend[0].label, "полночь Нью-Йорка");
        assert!(day.us.shots.is_none());
        assert_eq!(
            day.shot_paths(),
            ["indices-kb/shots/2026-10-08/macro_eu/m15.png"]
        );
        let d7 = load_day(&v, d("2026-10-07"));
        let s7 = d7.eu.shots.as_ref().unwrap();
        assert!(!s7.ok);
        assert_eq!(s7.error.as_deref(), Some("dry-run"));
        assert!(d7.us.shots.is_none());
    }

    #[test]
    fn reads_the_owners_files_back_with_the_day() {
        let mut v = vault();
        v.insert(format!("{STATE}human_read_2026-10-08_eu.json"), json!({"date": "2026-10-08", "region": "eu", "side": "SHORT", "at": "2026-10-08 09:31", "before_scale": true}).to_string());
        v.insert(format!("{STATE}day_answers_2026-10-08.json"), json!({"date": "2026-10-08", "at": "2026-10-08 20:15", "regions": {"eu": {"trade": "short", "by_rule": "yes"}, "us": {"trade": "none"}}, "note": "ждал вторую манипуляцию"}).to_string());
        let day = load_day(&v, d("2026-10-08"));
        assert_eq!(day.eu.human.as_ref().unwrap().side, HumanSide::Short);
        assert!(day.us.human.is_none());
        assert_eq!(
            serde_json::to_value(&day.answers).unwrap()["regions"],
            json!({"eu": {"trade": "short", "by_rule": "yes"}, "us": {"trade": "none"}})
        );
    }

    // ---- the shape of the whole day

    #[test]
    fn an_empty_vault_gives_an_empty_day_in_the_ts_shape() {
        let day = load_day(&DayInput::new(), d("2026-10-12"));
        let region = |r: &str| json!({"region": r, "scale": null, "verdict": null, "collect": null, "note": null, "shots": null, "human": null, "remember": null, "drift": null});
        let expect = json!({
            "date": "2026-10-12", "eu": region("eu"), "us": region("us"),
            "review": {"mech": null, "agent": null, "note": null, "shots": {"eu": null, "us": null}},
            "answers": null, "lessons": [], "stats": [], "names": {}, "zones": {"neutral": 15, "strong": 40},
            "week": null, "ideas": null, "trader": []
        });
        assert_eq!(serde_json::to_value(&day).unwrap(), expect);
        // Key order too, as JSON.stringify writes it.
        assert_eq!(serde_json::to_string(&day).unwrap(), expect.to_string());
    }

    #[test]
    fn broken_truncated_and_empty_files_read_as_missing() {
        let mut v = DayInput::new();
        let date = d("2026-10-12");
        for (k, body) in [
            (
                StateKind::Scale(Region::Eu),
                &b"{\"result\": {\"needle\": 1"[..],
            ),
            (StateKind::Verdict(Region::Eu), b""),
            (StateKind::Macro(Region::Eu), b"\xEF\xBB\xBF"),
            (StateKind::Review, b"null"),
            (StateKind::ReviewAgent, b"   "),
            (StateKind::Week, b"[1,2"),
            (StateKind::DayAnswers, b"{\"regions\": {}}"),
        ] {
            v.insert(
                names::state_path(&names::state_file(k, date)),
                body.to_vec(),
            );
        }
        v.insert("indices-kb/journal/lessons.md", "");
        v.insert("indices-kb/journal/scale_factor_scores.csv", "\u{FEFF}");
        v.insert("indices-kb/config/scale.json", "{");
        let day = load_day(&v, date);
        assert_eq!(
            serde_json::to_value(&day).unwrap(),
            serde_json::to_value(load_day(&DayInput::new(), date)).unwrap()
        );
    }

    #[test]
    fn unknown_fields_and_odd_values_pass_through_as_written() {
        let mut v = DayInput::new();
        let date = d("2026-10-12");
        let scale = json!({"x-new": {"a": [1.0, 2.5]}, "factors": [], "result": {"long": 1.0, "short": 2, "undecided": 97, "needle": 1, "future": true}, "light": "not an object"});
        v.insert(
            names::state_path(&names::state_file(StateKind::Scale(Region::Us), date)),
            scale.to_string(),
        );
        v.insert(
            names::state_path(&names::state_file(StateKind::Verdict(Region::Us), date)),
            "7",
        );
        v.insert(names::state_path(&names::state_file(StateKind::Review, date)), json!({"regions": {"eu": {"facts": {"dir": "LONG", "new_fact": 1}}}, "trader": ["  есть  ", "  "]}).to_string());
        let day = load_day(&v, date);
        let j = serde_json::to_value(&day).unwrap();
        assert_eq!(
            j["us"]["scale"],
            json!({"x-new": {"a": [1, 2.5]}, "factors": [], "result": {"long": 1, "short": 2, "undecided": 97, "needle": 1, "future": true}, "light": "not an object"})
        );
        assert_eq!(j["us"]["verdict"], json!(7));
        assert_eq!(day.us.verdict.as_ref().unwrap().verdict, None);
        assert_eq!(
            j["review"]["mech"]["regions"]["eu"]["facts"]["new_fact"],
            json!(1)
        );
        assert_eq!(
            day.review
                .mech
                .as_ref()
                .unwrap()
                .regions
                .eu
                .as_ref()
                .unwrap()
                .facts
                .as_ref()
                .unwrap()
                .dir
                .as_deref(),
            Some("LONG")
        );
        assert_eq!(day.trader, ["  есть  "]);
    }

    #[test]
    fn the_collect_verdict_stands_in_when_there_is_no_verdict_file() {
        let mut v = DayInput::new();
        let date = d("2026-10-12");
        v.insert(
            names::state_path(&names::state_file(StateKind::Macro(Region::Eu), date)),
            json!({"verdict": {"verdict": "LONG"}}).to_string(),
        );
        v.insert(
            names::state_path(&names::state_file(StateKind::Macro(Region::Us), date)),
            json!({"verdict": ""}).to_string(),
        );
        let day = load_day(&v, date);
        assert_eq!(
            day.eu.verdict.as_ref().unwrap().verdict.as_deref(),
            Some("LONG")
        );
        assert!(day.us.verdict.is_none());
        // The collect itself does not carry the verdict.
        assert!(day
            .eu
            .collect
            .as_ref()
            .unwrap()
            .raw()
            .get("verdict")
            .is_none());
    }

    #[test]
    fn names_come_from_every_region_of_the_config() {
        let cfg = json!({"regions": {"eu": {"factors": [{"id": "a", "name": "A"}, {"id": 5, "name": "Five"}, {"name": "no id"}, null]}, "us": {"factors": [{"id": "a", "name": "A (US)"}, {"id": "b"}]}, "xx": null}});
        let names = factor_names(Some(&cfg));
        assert_eq!(
            serde_json::to_string(&names).unwrap(),
            r#"{"5":"Five","a":"A (US)","undefined":"no id"}"#
        );
        assert!(factor_names(Some(
            &json!({"regions": [{"factors": [{"id": "z", "name": "Z"}]}]})
        ))
        .contains_key("z"));
        assert!(factor_names(None).is_empty());
    }

    #[test]
    fn day_files_cover_what_load_day_reads() {
        let date = d("2026-10-08");
        let files = day_files(date);
        assert_eq!(files.len(), 27);
        let mut v = vault();
        // Drop everything day_files does not name: the day must not change.
        v.files.retain(|k, _| files.contains(k));
        assert_eq!(
            serde_json::to_value(load_day(&v, date)).unwrap(),
            serde_json::to_value(load_day(&vault(), date)).unwrap()
        );
    }

    #[test]
    fn index_dates_skip_impossible_ones() {
        let idx = MacroIndex::new(
            "/v",
            Some([
                "scale_2026-99-99_eu.json",
                "macro_2026-10-08_eu.json",
                "macro_2026-10-08_us.json",
            ]),
        );
        assert_eq!(idx.days, ["2026-99-99", "2026-10-08"]);
        assert_eq!(idx.dates(), [d("2026-10-08")]);
    }
}
