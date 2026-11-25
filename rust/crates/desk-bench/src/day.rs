//! What «load a day» reads, and the lower-bound parser the benches time.
//!
//! `day_files` is the exact list `loadDay()` in `src/main/macro-state.ts`
//! opens for one date (7 per region, 10 per day; missing ones are normal).
//! `parse_lower_bound` turns the bytes into `serde_json::Value`s and splits
//! the CSV / markdown, which is the least any real parser must do. It is the
//! floor the desk-core numbers are compared against: if `desk_core::day` is
//! much slower than this, the cost is in normalization, not in JSON.
//!
//! HOOK(desk-core): build with `--features core-day` once desk-core exports
//! `day` (CONTRACTS.md: `DayInput` = file name → bytes, → `Day`). Only
//! `parse_day` below changes; the benches and perf-sample call that.

use anyhow::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// File name (relative to the vault root, `/`-separated) → bytes.
pub type DayInput = BTreeMap<String, Vec<u8>>;

pub const STATE: &str = "indices-kb/state";

/// Everything `loadDay(vault, date)` reads, relative to the vault.
pub fn day_files(date: &str) -> Vec<String> {
    let mut v = Vec::with_capacity(26);
    for r in ["eu", "us"] {
        for f in [
            format!("scale_{date}_{r}.json"),
            format!("macro_{date}_{r}_verdict.json"),
            format!("macro_{date}_{r}.json"),
            format!("shots_{date}_{r}.json"),
            format!("human_read_{date}_{r}.json"),
            format!("remember_{date}_{r}.json"),
            format!("drift_{date}_{r}.json"),
            format!("shots_{date}_{r}_review.json"),
        ] {
            v.push(format!("{STATE}/{f}"));
        }
    }
    for f in [
        format!("review_{date}.json"),
        format!("review_{date}_agent.json"),
        format!("day_answers_{date}.json"),
        format!("week_{date}.json"),
        "ideas.json".to_string(),
    ] {
        v.push(format!("{STATE}/{f}"));
    }
    v.push("indices-kb/journal/lessons.md".into());
    v.push("indices-kb/journal/scale_factor_scores.csv".into());
    v.push("indices-kb/config/scale.json".into());
    v
}

/// Read whichever of `day_files(date)` exist. A missing file is skipped, the
/// same as `readJson` returning null in TS.
pub fn read_day(vault: &Path, date: &str) -> DayInput {
    day_files(date)
        .into_iter()
        .filter_map(|rel| fs::read(vault.join(&rel)).ok().map(|b| (rel, b)))
        .collect()
}

/// Every dated state file (scale/macro/review) → its date, newest first.
/// Mirrors `listDays()`.
pub fn list_days(vault: &Path) -> Vec<String> {
    let mut days: Vec<String> = fs::read_dir(vault.join(STATE))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("scale_") || n.starts_with("macro_") || n.starts_with("review_"))
        .filter_map(|n| {
            let i = n.find('_')? + 1;
            let d = n.get(i..i + 10)?;
            let ok = d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-';
            ok.then(|| d.to_string())
        })
        .collect();
    days.sort();
    days.dedup();
    days.reverse();
    days
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Parsed {
    pub json_files: usize,
    pub json_bad: usize,
    pub csv_rows: usize,
    pub md_lines: usize,
    pub bytes: usize,
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b)
}

/// serde_json for every `.json`, a line split for `.csv` / `.md`.
/// A bad JSON file counts as `json_bad`, never an error — like the TS reader.
pub fn parse_lower_bound(input: &DayInput) -> Parsed {
    let mut p = Parsed::default();
    for (name, bytes) in input {
        p.bytes += bytes.len();
        let b = strip_bom(bytes);
        if name.ends_with(".json") {
            match serde_json::from_slice::<serde_json::Value>(b) {
                Ok(v) => {
                    std::hint::black_box(&v);
                    p.json_files += 1;
                }
                Err(_) => p.json_bad += 1,
            }
        } else if name.ends_with(".csv") {
            let text = String::from_utf8_lossy(b);
            p.csv_rows += text
                .lines()
                .skip(1)
                .map(|l| l.split(',').count().min(1))
                .sum::<usize>();
        } else {
            p.md_lines += String::from_utf8_lossy(b).lines().count();
        }
    }
    p
}

/// The function the benches time. Lower bound until desk-core lands.
#[cfg(not(feature = "core-day"))]
pub fn parse_day(input: &DayInput) -> Result<usize> {
    let p = parse_lower_bound(input);
    Ok(p.json_files + p.csv_rows)
}

/// HOOK(desk-core): the real parser. The call below follows CONTRACTS.md
/// («`DayInput` (имя файла → байты) → `Day`»); adjust the path/signature to
/// what desk-core actually exports when flipping the feature on.
#[cfg(feature = "core-day")]
pub fn parse_day(input: &DayInput) -> Result<usize> {
    let day = desk_core::day::load_day(input)?;
    std::hint::black_box(&day);
    Ok(input.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault_gen::{generate, GenOptions};

    #[test]
    fn mirrors_load_day_list() {
        let f = day_files("2026-11-25");
        assert_eq!(f.len(), 24);
        assert!(f.contains(&"indices-kb/state/shots_2026-11-25_us_review.json".to_string()));
        assert!(f.contains(&"indices-kb/config/scale.json".to_string()));
    }

    #[test]
    fn parses_a_generated_day() {
        let p = std::env::temp_dir().join(format!("desk-bench-day-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        generate(
            &p,
            &GenOptions {
                days: 3,
                png: None,
                ..GenOptions::default()
            },
        )
        .expect("gen");
        let input = read_day(&p, "2026-11-25");
        let parsed = parse_lower_bound(&input);
        assert!(parsed.json_files >= 18, "{parsed:?}");
        assert_eq!(parsed.json_bad, 0);
        assert!(parsed.csv_rows > 20);
        let days = list_days(&p);
        assert_eq!(days.first().map(String::as_str), Some("2026-11-25"));
        assert_eq!(days.len(), 3);
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn bad_json_is_counted_not_fatal() {
        let mut d = DayInput::new();
        d.insert("a.json".into(), b"\xEF\xBB\xBF{\"x\": 1}".to_vec());
        d.insert("b.json".into(), b"{\"x\": ".to_vec());
        let p = parse_lower_bound(&d);
        assert_eq!((p.json_files, p.json_bad), (1, 1));
    }
}
