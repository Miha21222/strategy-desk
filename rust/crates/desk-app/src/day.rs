//! The I/O half of `loadDay` / `listDays`: read the files a day needs into a
//! `desk_core::DayInput` and the state folder's names into a `MacroIndex`.
//! Everything else — what the files mean — is desk-core's.
//!
//! Keys are exactly `desk_core::day::day_files(date)`: vault-relative paths
//! with `/`. The two note files count by presence only (empty bytes).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use desk_core::day::day_files;
use desk_core::names;
use desk_core::vault::paths;
use desk_core::{DayInput, MacroIndex};

use crate::vault_write::state_dir;

/// One day in the day strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayMark {
    pub date: NaiveDate,
    /// A window review exists (`scale_` / `macro_` file).
    pub window_review: bool,
    /// The 20:00 day review exists (`review_` file).
    pub day_review: bool,
}

/// `<vault>/<rel>` for a `/`-separated vault-relative path.
pub fn full_path(vault: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(vault.to_path_buf(), |p, s| p.join(s))
}

fn is_note(rel: &str) -> bool {
    rel.strip_prefix(paths::NOTES[0])
        .is_some_and(|r| r.starts_with('/'))
}

/// Read one key of a day. `None` = missing, a directory, or unreadable
/// mid-write: all read as «not there yet», as in the TS (`readJson` → null);
/// the watcher reports the file again when its writer is done.
fn read_key(vault: &Path, rel: &str) -> Option<Vec<u8>> {
    let p = full_path(vault, rel);
    if is_note(rel) {
        return p.is_file().then(Vec::new);
    }
    std::fs::read(p).ok()
}

/// Everything `load_day(date)` looks at. Never fails: a missing file is a
/// missing key.
pub fn read_day(vault: &Path, date: NaiveDate) -> DayInput {
    let mut input = DayInput::new();
    for rel in day_files(date) {
        if let Some(bytes) = read_key(vault, &rel) {
            input.insert(rel, bytes);
        }
    }
    input
}

/// The keys of `date` as a set, for «does this change concern the open day».
pub fn day_keys(date: NaiveDate) -> HashSet<String> {
    day_files(date).into_iter().collect()
}

/// Re-read one key into `input`. Returns whether anything changed (a touch
/// that leaves the bytes alone is no change).
pub fn reread(vault: &Path, rel: &str, input: &mut DayInput) -> bool {
    match read_key(vault, rel) {
        Some(bytes) if input.files.get(rel) == Some(&bytes) => false,
        Some(bytes) => {
            input.insert(rel, bytes);
            true
        }
        None if input.contains(rel) => {
            input.remove(rel);
            true
        }
        None => false,
    }
}

/// The vault-relative key of a file the watcher named.
pub fn state_key(name: &str) -> String {
    names::state_path(name)
}

/// The names in `state/`, or `None` when the folder cannot be listed.
pub fn state_names(vault: &Path) -> Option<Vec<String>> {
    let rd = std::fs::read_dir(state_dir(vault)).ok()?;
    Some(
        rd.filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
    )
}

/// `listDays` through desk-core, plus the two marks of the day strip.
pub fn read_index(vault: &Path) -> (MacroIndex, Vec<DayMark>) {
    let names = state_names(vault);
    let index = MacroIndex::new(
        &vault.to_string_lossy(),
        names.as_ref().map(|n| n.iter().map(String::as_str)),
    );
    let names = names.unwrap_or_default();
    let marks = index
        .dates()
        .into_iter()
        .map(|date| {
            let d = date.format("%Y-%m-%d").to_string();
            let has = |prefix: &str| {
                names.iter().any(|n| {
                    n.starts_with(prefix) && names::date_of_state_file(n) == Some(d.as_str())
                })
            };
            DayMark {
                date,
                window_review: has("scale_") || has("macro_"),
                day_review: has("review_"),
            }
        })
        .collect();
    (index, marks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn vault() -> tempfile::TempDir {
        let v = tempfile::tempdir().unwrap();
        let s = state_dir(v.path());
        fs::create_dir_all(&s).unwrap();
        fs::write(s.join("scale_2026-11-25_eu.json"), "{}").unwrap();
        fs::write(s.join("review_2026-11-24.json"), "{}").unwrap();
        fs::write(s.join("regime_2026-11-25.json"), "{}").unwrap();
        fs::write(s.join("ideas.json"), "[]").unwrap();
        let j = v.path().join("indices-kb").join("journal");
        fs::create_dir_all(&j).unwrap();
        fs::write(j.join("lessons.md"), "# уроки").unwrap();
        let n = v.path().join(paths::NOTES[0]);
        fs::create_dir_all(&n).unwrap();
        fs::write(n.join("2026-11-25 Европа.md"), "заметка").unwrap();
        v
    }

    #[test]
    fn reads_exactly_the_day_files() {
        let v = vault();
        let input = read_day(v.path(), d("2026-11-25"));
        let keys: Vec<&str> = input.files.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            [
                "indices-kb/journal/lessons.md",
                "indices-kb/state/ideas.json",
                "indices-kb/state/scale_2026-11-25_eu.json",
                "Макро-обзоры/2026-11-25 Европа.md",
            ]
        );
        assert!(
            input.files["Макро-обзоры/2026-11-25 Европа.md"].is_empty(),
            "notes by presence only"
        );
        assert!(keys.iter().all(|k| day_keys(d("2026-11-25")).contains(*k)));
    }

    #[test]
    fn rereads_only_what_changed() {
        let v = vault();
        let mut input = read_day(v.path(), d("2026-11-25"));
        let key = state_key("ideas.json");
        assert!(
            !reread(v.path(), &key, &mut input),
            "same bytes = no change"
        );
        fs::write(state_dir(v.path()).join("ideas.json"), "[1]").unwrap();
        assert!(reread(v.path(), &key, &mut input));
        assert_eq!(input.files[&key], b"[1]");
        fs::remove_file(state_dir(v.path()).join("ideas.json")).unwrap();
        assert!(reread(v.path(), &key, &mut input));
        assert!(!input.contains(&key));
        assert!(!reread(v.path(), &key, &mut input));
    }

    #[test]
    fn index_and_marks_newest_first() {
        let v = vault();
        let (index, marks) = read_index(v.path());
        assert!(index.ok);
        assert_eq!(index.days, ["2026-11-25", "2026-11-24"]);
        assert_eq!(
            marks,
            vec![
                DayMark {
                    date: d("2026-11-25"),
                    window_review: true,
                    day_review: false
                },
                DayMark {
                    date: d("2026-11-24"),
                    window_review: false,
                    day_review: true
                },
            ]
        );
        let empty = tempfile::tempdir().unwrap();
        let (index, marks) = read_index(empty.path());
        assert_eq!(index.error.as_deref(), Some("no-state-dir"));
        assert!(marks.is_empty());
        assert!(read_day(empty.path(), d("2026-11-25")).files.is_empty());
    }
}
