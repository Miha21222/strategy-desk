//! Synthetic vault generator: the same file shapes as `scripts/make-demo-vault.py`,
//! any number of trading days, deterministic for a seed.
//!
//! It does not invent shapes. The templates under `templates/` are one full
//! demo day (2026-09-14) plus the dateless files, copied by
//! `scripts/rust/refresh-bench-templates.sh`. Each generated day is those
//! templates with the date rewritten and every float nudged by ±15 %, and a
//! seeded choice of which optional files exist that day (a human read, a
//! memory card, an evening review…), so loading a day exercises missing
//! files the way the real vault does.
//!
//! Scale: `days_for_scale(1) = 10` (the demo vault has ten dated days) and
//! `days_for_scale(10) = 501` (two years of weekdays); in between it is
//! `10 · N^1.7`. `real_size` pads every `macro_<D>_<region>.json` with ~250
//! headline items (~2000 lines at indent 1), the size TZ.md quotes for the
//! owner's real collect files; the demo ones are ~60 lines.

use crate::png;
use crate::rng::Rng;
use anyhow::{Context, Result};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// The date every template is written for.
pub const TEMPLATE_DATE: &str = "2026-09-14";
const TEMPLATE_FRIDAY: &str = "2026-09-18";

macro_rules! tpl {
    ($p:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/templates/", $p))
    };
}

/// (kind, region or "", template text). Kind names match the file prefixes.
const DAY_TEMPLATES: &[(&str, &str, &str)] = &[
    ("scale", "eu", tpl!("state/scale_2026-09-14_eu.json")),
    ("scale", "us", tpl!("state/scale_2026-09-14_us.json")),
    ("macro", "eu", tpl!("state/macro_2026-09-14_eu.json")),
    ("macro", "us", tpl!("state/macro_2026-09-14_us.json")),
    (
        "verdict",
        "eu",
        tpl!("state/macro_2026-09-14_eu_verdict.json"),
    ),
    (
        "verdict",
        "us",
        tpl!("state/macro_2026-09-14_us_verdict.json"),
    ),
    ("shots", "eu", tpl!("state/shots_2026-09-14_eu.json")),
    ("shots", "us", tpl!("state/shots_2026-09-14_us.json")),
    (
        "shots_review",
        "eu",
        tpl!("state/shots_2026-09-14_eu_review.json"),
    ),
    (
        "human_read",
        "eu",
        tpl!("state/human_read_2026-09-14_eu.json"),
    ),
    (
        "human_read",
        "us",
        tpl!("state/human_read_2026-09-14_us.json"),
    ),
    ("remember", "eu", tpl!("state/remember_2026-09-14_eu.json")),
    ("remember", "us", tpl!("state/remember_2026-09-14_us.json")),
    ("drift", "eu", tpl!("state/drift_2026-09-14_eu.json")),
    ("drift", "us", tpl!("state/drift_2026-09-14_us.json")),
    ("review", "", tpl!("state/review_2026-09-14.json")),
    (
        "review_agent",
        "",
        tpl!("state/review_2026-09-14_agent.json"),
    ),
    ("day_answers", "", tpl!("state/day_answers_2026-09-14.json")),
];
const WEEK_TEMPLATE: &str = tpl!("state/week_2026-09-18.json");
const IDEAS_TEMPLATE: &str = tpl!("state/ideas.json");
const SCALE_CONFIG: &str = tpl!("config/scale.json");
const LESSONS_HEADER: &str = tpl!("journal/lessons.header.md");
const SCORES_HEADER: &str = tpl!("journal/scale_factor_scores.header.csv");

#[derive(Debug, Clone)]
pub struct GenOptions {
    pub days: usize,
    /// The newest trading day (the vault's `.today`). Always written complete.
    pub end: NaiveDate,
    pub seed: u64,
    pub real_size: bool,
    /// Placeholder chart size; `None` writes the shot JSON but no PNGs.
    pub png: Option<(u32, u32)>,
}

impl Default for GenOptions {
    fn default() -> Self {
        GenOptions {
            days: days_for_scale(1.0),
            // 2026-11-25, the day the rewrite started: fixed, so a default run
            // is reproducible next year too.
            end: NaiveDate::from_ymd_opt(2026, 11, 25).unwrap_or_default(),
            seed: 23,
            real_size: false,
            png: Some((64, 40)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GenSummary {
    pub days: usize,
    pub first: String,
    pub last: String,
    pub files: usize,
    pub bytes: u64,
    pub state_files: usize,
}

pub fn days_for_scale(scale: f64) -> usize {
    (10.0 * scale.max(0.0).powf(1.7)).round().max(1.0) as usize
}

/// `n` weekdays ending at `end` (inclusive if `end` is a weekday), oldest first.
pub fn trading_days(end: NaiveDate, n: usize) -> Vec<NaiveDate> {
    let mut out = Vec::with_capacity(n);
    let mut d = end;
    while out.len() < n {
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            out.push(d);
        }
        d -= Duration::days(1);
    }
    out.reverse();
    out
}

struct Writer {
    root: PathBuf,
    files: usize,
    bytes: u64,
    state_files: usize,
}

impl Writer {
    fn bytes(&mut self, rel: &str, data: &[u8]) -> Result<()> {
        let p = self.root.join(rel);
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).with_context(|| format!("mkdir {}", dir.display()))?;
        }
        fs::write(&p, data).with_context(|| format!("write {}", p.display()))?;
        self.files += 1;
        self.bytes += data.len() as u64;
        if rel.starts_with("indices-kb/state/") {
            self.state_files += 1;
        }
        Ok(())
    }

    /// Python's `json.dumps(indent=1, ensure_ascii=False)`, like the skill.
    fn json(&mut self, rel: &str, v: &Value) -> Result<()> {
        let mut buf = Vec::with_capacity(4096);
        let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
        v.serialize(&mut ser)?;
        self.bytes(rel, &buf)
    }
}

/// Rewrite template dates and nudge floats. Integers stay: they are needles,
/// counts and maxima whose exact values some normalizers check.
fn retarget(v: &mut Value, dates: &[(&str, String)], rng: &mut Rng) {
    match v {
        Value::String(s) => {
            for (from, to) in dates {
                if s.contains(from) {
                    *s = s.replace(from, to);
                }
            }
        }
        Value::Number(n) => {
            if n.is_f64() {
                if let Some(x) = n.as_f64() {
                    let y = x * rng.range_f64(0.85, 1.15);
                    let digits = if y.abs() < 1.0 { 100.0 } else { 10.0 };
                    if let Some(m) = serde_json::Number::from_f64((y * digits).round() / digits) {
                        *n = m;
                    }
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| retarget(x, dates, rng)),
        Value::Object(o) => o.values_mut().for_each(|x| retarget(x, dates, rng)),
        _ => {}
    }
}

fn parse_template(text: &str) -> Result<Value> {
    serde_json::from_str(text)
        .context("bad template JSON (run scripts/rust/refresh-bench-templates.sh)")
}

const HEADLINE_WORDS: &[&str] = &[
    "ЕЦБ",
    "ставки",
    "инфляция",
    "DAX",
    "нефть",
    "доходности",
    "ФРС",
    "отчёт",
    "прогноз",
    "рынок",
    "Bund",
    "евро",
    "доллар",
    "индекс",
    "заявки",
    "PMI",
    "Ifo",
    "ZEW",
    "энергия",
    "Китай",
];

fn pad_collect(v: &mut Value, rng: &mut Rng, date: &str) {
    let items: Vec<Value> = (0..250)
        .map(|i| {
            let source = ["Reuters", "Bloomberg", "Handelsblatt", "Investing.com"][i % 4];
            let words: Vec<&str> = (0..9)
                .filter_map(|_| rng.pick(HEADLINE_WORDS).copied())
                .collect();
            json!({
                "title": format!("[синтетика] {}", words.join(" ")),
                "source": source,
                "published_eet": format!("{date} {:02}:{:02}", (i / 12) % 24, (i * 7) % 60),
                "tone": (rng.below(11) as i64) - 5,
                "tags": [HEADLINE_WORDS[i % HEADLINE_WORDS.len()]],
                "url": format!("https://example.invalid/news/{date}/{i}"),
            })
        })
        .collect();
    if let Some(h) = v.get_mut("headlines").and_then(Value::as_object_mut) {
        h.insert("count".into(), json!(items.len()));
        h.insert("items".into(), Value::Array(items));
    }
}

fn file_name(kind: &str, region: &str, d: &str) -> String {
    match kind {
        "verdict" => format!("macro_{d}_{region}_verdict.json"),
        "shots_review" => format!("shots_{d}_{region}_review.json"),
        "review" => format!("review_{d}.json"),
        "review_agent" => format!("review_{d}_agent.json"),
        "day_answers" => format!("day_answers_{d}.json"),
        _ => format!("{kind}_{d}_{region}.json"),
    }
}

/// Which optional files a given day has. The last day has all of them.
fn present(kind: &str, region: &str, rng: &mut Rng, full: bool) -> bool {
    if full {
        return true;
    }
    let p = match (kind, region) {
        ("scale" | "macro" | "verdict", "eu") => 0.97,
        ("scale" | "macro" | "verdict", _) => 0.9,
        ("shots", _) => 0.85,
        ("shots_review", _) => 0.4,
        ("human_read", _) => 0.7,
        ("remember", _) => 0.6,
        ("drift", _) => 0.5,
        ("review", _) => 0.9,
        ("review_agent", _) => 0.75,
        ("day_answers", _) => 0.5,
        _ => 1.0,
    };
    rng.chance(p)
}

const LESSONS: &[&str] = &[
    "В пилу первая манипуляция у открытия дня не держит — ждать вторую.",
    "Если премаркет уже снял цель по направлению — место до цели исчерпано.",
    "Уровень ближе 0.1 ADR — не цель, а шум.",
    "День экспирации — пропуск первых 30 минут.",
    "Широкая ночь — окно чаще идёт против ночи.",
];

pub fn generate(out: &Path, o: &GenOptions) -> Result<GenSummary> {
    let days = trading_days(o.end, o.days.max(1));
    let mut w = Writer {
        root: out.to_path_buf(),
        files: 0,
        bytes: 0,
        state_files: 0,
    };
    let templates: Vec<(&str, &str, Value)> = DAY_TEMPLATES
        .iter()
        .map(|(k, r, t)| parse_template(t).map(|v| (*k, *r, v)))
        .collect::<Result<_>>()?;
    let week = parse_template(WEEK_TEMPLATE)?;

    let mut lessons = String::from(LESSONS_HEADER);
    let mut scores = String::from(SCORES_HEADER);
    for (i, day) in days.iter().enumerate() {
        let d = day.format("%Y-%m-%d").to_string();
        let full = i + 1 == days.len();
        let mut rng = Rng::fork(o.seed, &d);
        let dates = [(TEMPLATE_DATE, d.clone())];
        for (kind, region, t) in &templates {
            // The demo has one review-shots template (eu); the us twin is the
            // same shape with the region swapped.
            let regions: &[&str] = if *kind == "shots_review" {
                &["eu", "us"]
            } else {
                &[region]
            };
            for r in regions {
                if !present(kind, r, &mut rng, full) {
                    continue;
                }
                let mut v = t.clone();
                if *r != *region {
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("region".into(), json!(r));
                    }
                }
                let reg_dates = [
                    dates[0].clone(),
                    (
                        "macro_eu/",
                        if *r == "us" {
                            "macro_us/".into()
                        } else {
                            "macro_eu/".into()
                        },
                    ),
                ];
                retarget(&mut v, &reg_dates, &mut rng);
                if *kind == "macro" && o.real_size {
                    pad_collect(&mut v, &mut rng, &d);
                }
                if *kind == "shots" || *kind == "shots_review" {
                    if let Some((pw, ph)) = o.png {
                        for s in v["shots"].as_array().into_iter().flatten() {
                            if let Some(rel) = s["path"].as_str() {
                                w.bytes(rel, &png::placeholder(pw, ph, rng.below(256) as u8))?;
                            }
                        }
                    }
                }
                if *kind == "scale" {
                    for f in v["factors"].as_array().into_iter().flatten() {
                        let side = f["side"].as_str().unwrap_or("");
                        if side.is_empty() {
                            continue;
                        }
                        let dir = if rng.chance(0.5) { "LONG" } else { "SHORT" };
                        scores.push_str(&format!(
                            "{d},{r},{},{},{},{side},{},{dir},{}\n",
                            f["id"].as_str().unwrap_or(""),
                            f["group"].as_str().unwrap_or(""),
                            f["share"],
                            f["strength"],
                            u8::from(side == dir),
                        ));
                    }
                }
                if *kind == "verdict" && rng.chance(0.8) {
                    let name = if *r == "eu" { "Европа" } else { "США" };
                    let note =
                        format!("# [синтетика] {d} {name}\n\nСинтетическая заметка desk-bench.\n");
                    w.bytes(&format!("Макро-обзоры/{d} {name}.md"), note.as_bytes())?;
                }
                if *kind == "review" {
                    let note = format!(
                        "# [синтетика] {d} Итог дня\n\nСинтетическая заметка desk-bench.\n"
                    );
                    w.bytes(&format!("Макро-обзоры/{d} Итог дня.md"), note.as_bytes())?;
                }
                w.json(&format!("indices-kb/state/{}", file_name(kind, r, &d)), &v)?;
            }
        }
        if day.weekday() == Weekday::Fri {
            let monday = (*day - Duration::days(4)).format("%Y-%m-%d").to_string();
            let mut v = week.clone();
            retarget(
                &mut v,
                &[(TEMPLATE_DATE, monday), (TEMPLATE_FRIDAY, d.clone())],
                &mut rng,
            );
            w.json(&format!("indices-kb/state/week_{d}.json"), &v)?;
        }
        if rng.chance(0.5) || full {
            if let Some(l) = rng.pick(LESSONS) {
                lessons.push_str(&format!("- {d}: [синтетика] {l}\n"));
            }
        }
    }

    let ideas = parse_template(IDEAS_TEMPLATE)?;
    w.json("indices-kb/state/ideas.json", &ideas)?;
    w.bytes("indices-kb/config/scale.json", SCALE_CONFIG.as_bytes())?;
    w.bytes("indices-kb/journal/lessons.md", lessons.as_bytes())?;
    w.bytes(
        "indices-kb/journal/scale_factor_scores.csv",
        scores.as_bytes(),
    )?;
    let last = days
        .last()
        .copied()
        .unwrap_or(o.end)
        .format("%Y-%m-%d")
        .to_string();
    let first = days
        .first()
        .copied()
        .unwrap_or(o.end)
        .format("%Y-%m-%d")
        .to_string();
    w.bytes(".today", last.as_bytes())?;
    w.bytes(
        "README.md",
        "# Синтетический vault desk-bench\n\nСгенерирован `gen-vault`. Все числа выдуманы; это не реальные данные.\n"
            .as_bytes(),
    )?;
    let summary = GenSummary {
        days: days.len(),
        first,
        last,
        files: w.files + 1,
        bytes: w.bytes,
        state_files: w.state_files,
    };
    let manifest = json!({
        "generator": "desk-bench gen-vault",
        "template_date": TEMPLATE_DATE,
        "seed": o.seed,
        "real_size": o.real_size,
        "png": o.png.map(|(a, b)| format!("{a}x{b}")),
        "summary": summary,
    });
    w.json(".desk-bench.json", &manifest)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("desk-bench-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn scale_anchors() {
        assert_eq!(days_for_scale(1.0), 10);
        let x10 = days_for_scale(10.0);
        assert!((480..=520).contains(&x10), "{x10}");
    }

    #[test]
    fn weekdays_only() {
        let end = NaiveDate::from_ymd_opt(2026, 11, 23).unwrap_or_default(); // Monday
        let d = trading_days(end, 3);
        assert_eq!(d.len(), 3);
        assert_eq!(d[2], end);
        assert_eq!(d[1].weekday(), Weekday::Fri);
    }

    #[test]
    fn deterministic_and_shaped() {
        let (a, b) = (tmp("a"), tmp("b"));
        let o = GenOptions {
            days: 6,
            ..GenOptions::default()
        };
        let sa = generate(&a, &o).expect("gen a");
        let sb = generate(&b, &o).expect("gen b");
        assert_eq!(sa.bytes, sb.bytes);
        let f = "indices-kb/state/scale_2026-11-25_eu.json";
        assert_eq!(fs::read(a.join(f)).ok(), fs::read(b.join(f)).ok());
        // Full last day: every template kind exists, dates rewritten.
        let v: Value =
            serde_json::from_slice(&fs::read(a.join(f)).unwrap_or_default()).unwrap_or_default();
        assert_eq!(v["date"], "2026-11-25");
        assert!(v["factors"]
            .as_array()
            .map(|x| x.len() > 10)
            .unwrap_or(false));
        assert!(a
            .join("indices-kb/state/shots_2026-11-25_us_review.json")
            .exists());
        assert!(a
            .join("indices-kb/shots/2026-11-25/macro_us/m15.png")
            .exists());
        // 2026-11-20 is a Friday inside the 6 days → a week file.
        assert!(a.join("indices-kb/state/week_2026-11-20.json").exists());
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }

    #[test]
    fn real_size_collect_is_about_2000_lines() {
        let p = tmp("fat");
        let o = GenOptions {
            days: 1,
            real_size: true,
            png: None,
            ..GenOptions::default()
        };
        generate(&p, &o).expect("gen");
        let text = fs::read_to_string(p.join("indices-kb/state/macro_2026-11-25_eu.json"))
            .unwrap_or_default();
        let lines = text.lines().count();
        assert!((1500..=3000).contains(&lines), "{lines}");
        let _ = fs::remove_dir_all(&p);
    }
}
