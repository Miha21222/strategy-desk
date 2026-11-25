//! Reading a day's files out of the vault (the I/O half of `loadDay`).
//!
//! The pure half — turning bytes into a `Day` — belongs to desk-core
//! (`DayInput → Day`, CONTRACTS.md). Until it lands, [`build`] wraps the raw
//! files into the [`DayData`] placeholder.
//!
//! [`DayInput`] keys are vault-relative paths with `/` separators, e.g.
//! `indices-kb/state/scale_2026-11-25_eu.json`. Note files are existence
//! only (empty bytes): `loadDay` never read their content.
//! TODO(core): agree the key spelling with desk-core's `DayInput`.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde_json::{Map, Value};

use crate::core_stub::{DayData, Region};
use crate::vault_write::{iso, state_dir, STATE_DIR};

/// Vault-relative path → bytes.
pub type DayInput = BTreeMap<String, Vec<u8>>;

/// The one dateless state file; a change to it concerns whatever day is open.
pub const IDEAS_FILE: &str = "ideas.json";
const JOURNAL: [&str; 2] = ["indices-kb", "journal"];
const JOURNAL_FILES: [&str; 2] = ["lessons.md", "scale_factor_scores.csv"];
const CONFIG: [&str; 3] = ["indices-kb", "config", "scale.json"];
const NOTES_DIR: &str = "Макро-обзоры";

/// Prefixes `dateOfStateFile` knows (src/shared/macro.ts).
const DATED_PREFIXES: [&str; 10] = [
    "scale",
    "macro",
    "review",
    "regime",
    "shots",
    "human_read",
    "day_answers",
    "remember",
    "drift",
    "week",
];

/// `dateOfStateFile`: `scale_2026-10-09_us.json` → 2026-10-09.
/// TODO(core): `desk_core::names` owns this; use it once it exists.
pub fn date_of_state_file(name: &str) -> Option<NaiveDate> {
    DATED_PREFIXES.iter().find_map(|p| {
        let rest = name.strip_prefix(p)?.strip_prefix('_')?;
        let (date, tail) = (rest.get(..10)?, rest.get(10..)?);
        if !(tail.starts_with('_') || tail.starts_with('.')) {
            return None;
        }
        let b = date.as_bytes();
        let shape = b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
        if !shape {
            return None;
        }
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()
    })
}

/// Does a changed state file concern the day `date`?
pub fn concerns(name: &str, date: NaiveDate) -> bool {
    name == IDEAS_FILE || date_of_state_file(name) == Some(date)
}

/// Key of a state file in [`DayInput`].
pub fn state_key(name: &str) -> String {
    format!("{}/{}/{name}", STATE_DIR[0], STATE_DIR[1])
}

/// Read every file the day needs. Missing files are simply absent; an
/// unreadable `state/` is an error only when the folder itself is unreadable.
pub fn read_day(vault: &Path, date: NaiveDate) -> io::Result<DayInput> {
    let mut input = DayInput::new();
    let sdir = state_dir(vault);
    match std::fs::read_dir(&sdir) {
        Ok(rd) => {
            for e in rd.filter_map(Result::ok) {
                let Ok(name) = e.file_name().into_string() else {
                    continue;
                };
                if concerns(&name, date) && e.file_type().is_ok_and(|t| t.is_file()) {
                    if let Some(bytes) = read_opt(&e.path()) {
                        input.insert(state_key(&name), bytes);
                    }
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    for f in JOURNAL_FILES {
        let rel = format!("{}/{}/{f}", JOURNAL[0], JOURNAL[1]);
        if let Some(b) = read_opt(&join(vault, &rel)) {
            input.insert(rel, b);
        }
    }
    let cfg = CONFIG.join("/");
    if let Some(b) = read_opt(&join(vault, &cfg)) {
        input.insert(cfg, b);
    }
    let d = iso(date).map_err(io::Error::other)?;
    let mut notes: Vec<String> = Region::ALL
        .iter()
        .map(|r| format!("{d} {}.md", region_note(*r)))
        .collect();
    notes.push(format!("{d} Итог дня.md"));
    for n in notes {
        let rel = format!("{NOTES_DIR}/{n}");
        if join(vault, &rel).is_file() {
            input.insert(rel, Vec::new());
        }
    }
    Ok(input)
}

/// Re-read one state file into `input`. Returns whether anything changed
/// (a touch that leaves the bytes alone is no change).
pub fn reread_state_file(vault: &Path, name: &str, input: &mut DayInput) -> bool {
    let key = state_key(name);
    match read_opt(&state_dir(vault).join(name)) {
        Some(bytes) if input.get(&key) == Some(&bytes) => false,
        Some(bytes) => {
            input.insert(key, bytes);
            true
        }
        None => input.remove(&key).is_some(),
    }
}

/// TODO(core): `desk_core::day::load(date, input)`. Placeholder shape:
/// `{"date": "YYYY-MM-DD", "files": {key: JSON | text | null}}`.
pub fn build(date: NaiveDate, input: &DayInput) -> DayData {
    let mut files = Map::new();
    for (k, bytes) in input {
        let text = String::from_utf8_lossy(bytes);
        // The Python side writes UTF-8, PowerShell sometimes with a BOM.
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let v = if k.ends_with(".json") {
            serde_json::from_str(text).unwrap_or(Value::Null)
        } else {
            Value::String(text.to_string())
        };
        files.insert(k.clone(), v);
    }
    serde_json::json!({ "date": date.format("%Y-%m-%d").to_string(), "files": files })
}

/// Dates with a window review (`scale_`/`macro_`) or a day review (`review_`),
/// newest first. Names only; nothing is read.
pub fn list_days(vault: &Path) -> io::Result<Vec<(NaiveDate, bool, bool)>> {
    let mut by_date: BTreeMap<NaiveDate, (bool, bool)> = BTreeMap::new();
    let rd = match std::fs::read_dir(state_dir(vault)) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    for e in rd.filter_map(Result::ok) {
        let Ok(name) = e.file_name().into_string() else {
            continue;
        };
        let window = name.starts_with("scale_") || name.starts_with("macro_");
        let review = name.starts_with("review_");
        if !(window || review) {
            continue;
        }
        if let Some(d) = date_of_state_file(&name) {
            let m = by_date.entry(d).or_default();
            m.0 |= window;
            m.1 |= review;
        }
    }
    Ok(by_date
        .into_iter()
        .rev()
        .map(|(d, (w, r))| (d, w, r))
        .collect())
}

fn region_note(r: Region) -> &'static str {
    match r {
        Region::Eu => "Европа",
        Region::Us => "США",
    }
}

fn join(vault: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(vault.to_path_buf(), |p, s| p.join(s))
}

/// Missing, a directory or unreadable mid-write: treated alike as «not there
/// yet»; the watcher reports the file again when the writer is done.
fn read_opt(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn date_of_state_file_matches_the_ts_regex() {
        assert_eq!(
            date_of_state_file("scale_2026-10-09_us.json"),
            Some(d("2026-10-09"))
        );
        assert_eq!(
            date_of_state_file("review_2026-10-09.json"),
            Some(d("2026-10-09"))
        );
        assert_eq!(
            date_of_state_file("human_read_2026-10-09_eu.json"),
            Some(d("2026-10-09"))
        );
        assert_eq!(
            date_of_state_file("week_2026-10-09.json"),
            Some(d("2026-10-09"))
        );
        assert_eq!(date_of_state_file("ideas.json"), None);
        assert_eq!(date_of_state_file("scale_2026-10-09us.json"), None);
        assert_eq!(date_of_state_file("other_2026-10-09_us.json"), None);
        assert_eq!(date_of_state_file("scale_2026-13-09_us.json"), None);
        assert_eq!(date_of_state_file("scale_2026-1-09_us.json"), None);
        assert_eq!(date_of_state_file("scale_2026-10-09"), None);
        assert_eq!(date_of_state_file("scale_２０２６-10-09_us.json"), None);
    }

    fn vault() -> tempfile::TempDir {
        let v = tempfile::tempdir().unwrap();
        let s = state_dir(v.path());
        fs::create_dir_all(&s).unwrap();
        fs::write(
            s.join("scale_2026-11-25_eu.json"),
            "\u{feff}{\"result\":{\"score\":12}}",
        )
        .unwrap();
        fs::write(s.join("macro_2026-11-25_eu.json"), "{broken").unwrap();
        fs::write(s.join("review_2026-11-24.json"), "{}").unwrap();
        fs::write(s.join("ideas.json"), "[]").unwrap();
        fs::write(s.join("notes.txt"), "x").unwrap();
        let j = v.path().join("indices-kb").join("journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("lessons.md"), "# уроки").unwrap();
        let n = v.path().join(NOTES_DIR);
        fs::create_dir_all(&n).unwrap();
        fs::write(n.join("2026-11-25 Европа.md"), "заметка").unwrap();
        v
    }

    #[test]
    fn reads_the_day_and_nothing_else() {
        let v = vault();
        let input = read_day(v.path(), d("2026-11-25")).unwrap();
        let keys: Vec<&str> = input.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "indices-kb/journal/lessons.md",
                "indices-kb/state/ideas.json",
                "indices-kb/state/macro_2026-11-25_eu.json",
                "indices-kb/state/scale_2026-11-25_eu.json",
                "Макро-обзоры/2026-11-25 Европа.md",
            ]
        );
        let day = build(d("2026-11-25"), &input);
        assert_eq!(day["date"], "2026-11-25");
        assert_eq!(
            day["files"]["indices-kb/state/scale_2026-11-25_eu.json"]["result"]["score"],
            12
        );
        assert!(day["files"]["indices-kb/state/macro_2026-11-25_eu.json"].is_null());
        assert_eq!(day["files"]["indices-kb/journal/lessons.md"], "# уроки");
    }

    #[test]
    fn rereads_only_what_changed() {
        let v = vault();
        let mut input = read_day(v.path(), d("2026-11-25")).unwrap();
        assert!(
            !reread_state_file(v.path(), "ideas.json", &mut input),
            "same bytes = no change"
        );
        fs::write(state_dir(v.path()).join("ideas.json"), "[1]").unwrap();
        assert!(reread_state_file(v.path(), "ideas.json", &mut input));
        assert_eq!(input[&state_key("ideas.json")], b"[1]");
        fs::remove_file(state_dir(v.path()).join("ideas.json")).unwrap();
        assert!(reread_state_file(v.path(), "ideas.json", &mut input));
        assert!(!input.contains_key(&state_key("ideas.json")));
        assert!(!reread_state_file(v.path(), "ideas.json", &mut input));
    }

    #[test]
    fn lists_days_newest_first() {
        let v = vault();
        assert_eq!(
            list_days(v.path()).unwrap(),
            vec![
                (d("2026-11-25"), true, false),
                (d("2026-11-24"), false, true)
            ]
        );
        let empty = tempfile::tempdir().unwrap();
        assert!(list_days(empty.path()).unwrap().is_empty());
        assert!(read_day(empty.path(), d("2026-11-25")).unwrap().is_empty());
    }
}
