//! The only writes Desk makes into the vault: the owner's two files (note 23).
//! Port of `writeOwnerFile` / `writeHumanRead` / `writeDayAnswers` in
//! `src/main/macro-state.ts`.
//!
//! * The file name is built here from a [`NaiveDate`] — never taken from a
//!   caller-provided string — so nothing but these two names can be written.
//! * Atomic: a temp file in the same directory, fsync, rename over the target,
//!   so the watcher and the skill never see half a file.
//! * Windows: a rename over a file someone has open without delete sharing
//!   fails with a sharing/access error for a moment; it is retried.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;

use crate::core_stub::{DayAnswers, HumanRead, Region};

/// `VAULT_PATHS.state`.
pub const STATE_DIR: [&str; 2] = ["indices-kb", "state"];
/// The note is cut to one short line (`writeDayAnswers`).
pub const NOTE_MAX_CHARS: usize = 300;

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// A date whose `%Y-%m-%d` is not exactly ten characters (year outside
    /// 1000–9999) would make a name the skill's `_(\d{4}-\d{2}-\d{2})` misses.
    #[error("date out of range for a state file name")]
    BadDate,
    #[error("vault path is empty")]
    NoVault,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl WriteError {
    /// i18n key for the UI's error list.
    pub fn key(&self) -> &'static str {
        match self {
            WriteError::BadDate => "error.vault.bad_date",
            WriteError::NoVault => "error.vault.not_set",
            WriteError::Io(_) | WriteError::Json(_) => "error.vault.write_failed",
        }
    }
}

/// The two files, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerFile {
    HumanRead { date: NaiveDate, region: Region },
    DayAnswers { date: NaiveDate },
}

impl OwnerFile {
    pub fn file_name(&self) -> Result<String, WriteError> {
        Ok(match *self {
            OwnerFile::HumanRead { date, region } => {
                format!("human_read_{}_{}.json", iso(date)?, region.as_str())
            }
            OwnerFile::DayAnswers { date } => format!("day_answers_{}.json", iso(date)?),
        })
    }
}

/// `YYYY-MM-DD`, only for four-digit years.
pub fn iso(date: NaiveDate) -> Result<String, WriteError> {
    if !(1000..=9999).contains(&date.year()) {
        return Err(WriteError::BadDate);
    }
    Ok(date.format("%Y-%m-%d").to_string())
}

pub fn state_dir(vault: &Path) -> PathBuf {
    STATE_DIR.iter().fold(vault.to_path_buf(), |p, s| p.join(s))
}

/// Kyiv wall clock for the `at` fields: «2026-10-12 09:31».
pub fn kyiv_stamp(now: DateTime<Utc>) -> String {
    now.with_timezone(&chrono_tz::Europe::Kyiv)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

/// The owner's read of a window. `date`, `region` and `at` of the record are
/// set here, so the content always agrees with the file name.
pub fn write_human_read(
    vault: &Path,
    date: NaiveDate,
    region: Region,
    read: &HumanRead,
    now: DateTime<Utc>,
) -> Result<HumanRead, WriteError> {
    let rec = HumanRead {
        date: iso(date)?,
        region,
        side: read.side,
        at: kyiv_stamp(now),
        before_scale: read.before_scale,
    };
    write_owner_file(vault, OwnerFile::HumanRead { date, region }, &rec)?;
    Ok(rec)
}

/// The owner's evening answers; the note is collapsed to one line of at most
/// [`NOTE_MAX_CHARS`] characters.
pub fn write_day_answers(
    vault: &Path,
    date: NaiveDate,
    answers: &DayAnswers,
    now: DateTime<Utc>,
) -> Result<DayAnswers, WriteError> {
    let note: String = answers
        .note
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NOTE_MAX_CHARS)
        .collect();
    let rec = DayAnswers {
        date: iso(date)?,
        at: kyiv_stamp(now),
        regions: answers.regions.clone(),
        note,
    };
    write_owner_file(vault, OwnerFile::DayAnswers { date }, &rec)?;
    Ok(rec)
}

fn write_owner_file(
    vault: &Path,
    file: OwnerFile,
    data: &impl Serialize,
) -> Result<PathBuf, WriteError> {
    if vault.as_os_str().is_empty() {
        return Err(WriteError::NoVault);
    }
    let name = file.file_name()?;
    // One-space indent: what `JSON.stringify(data, null, 1)` wrote, so a
    // diff of the vault between the two apps stays quiet.
    let mut bytes = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut bytes,
        serde_json::ser::PrettyFormatter::with_indent(b" "),
    );
    data.serialize(&mut ser)?;
    let dir = state_dir(vault);
    fs::create_dir_all(&dir)?;
    Ok(atomic_write(&dir, &name, &bytes)?)
}

static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `dir/name` atomically. The temp file starts with a dot and
/// ends with `.tmp`, which the watcher ignores.
pub fn atomic_write(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let target = dir.join(name);
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = dir.join(format!(".{name}.{}.{seq}.tmp", std::process::id()));
    let result = (|| {
        let mut f = File::create_new(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        retry(|| fs::rename(&tmp, &target), is_transient, RETRY_DELAYS)
    })();
    if result.is_err() {
        // Best effort: a leftover temp file is harmless and ignored by everyone.
        let _ = fs::remove_file(&tmp);
    }
    result.map(|()| target)
}

/// ~1.3 s in all: long enough for an antivirus scan or an editor's read.
const RETRY_DELAYS: &[u64] = &[10, 20, 40, 80, 150, 250, 350, 400];

fn retry(
    mut op: impl FnMut() -> io::Result<()>,
    transient: impl Fn(&io::Error) -> bool,
    delays_ms: &[u64],
) -> io::Result<()> {
    let mut delays = delays_ms.iter();
    loop {
        match op() {
            Ok(()) => return Ok(()),
            Err(e) if transient(&e) => match delays.next() {
                Some(ms) => std::thread::sleep(Duration::from_millis(*ms)),
                None => return Err(e),
            },
            Err(e) => return Err(e),
        }
    }
}

/// ERROR_ACCESS_DENIED (5), ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
fn is_transient(e: &io::Error) -> bool {
    cfg!(windows) && matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_stub::{ByRule, DayAnswerRegion, HumanSide, TradeTaken};
    use chrono::TimeZone;
    use std::cell::Cell;
    use std::collections::BTreeMap;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn now() -> DateTime<Utc> {
        // 06:31 UTC = 08:31 Kyiv in November (EET, +2).
        Utc.with_ymd_and_hms(2026, 11, 25, 6, 31, 10).unwrap()
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn names_come_from_the_date_only() {
        assert_eq!(
            OwnerFile::HumanRead {
                date: d(2026, 11, 5),
                region: Region::Us
            }
            .file_name()
            .unwrap(),
            "human_read_2026-11-05_us.json"
        );
        assert_eq!(
            OwnerFile::DayAnswers {
                date: d(2026, 1, 9)
            }
            .file_name()
            .unwrap(),
            "day_answers_2026-01-09.json"
        );
        assert!(matches!(
            OwnerFile::DayAnswers {
                date: d(10000, 1, 1)
            }
            .file_name(),
            Err(WriteError::BadDate)
        ));
        assert!(matches!(
            OwnerFile::DayAnswers { date: d(999, 1, 1) }.file_name(),
            Err(WriteError::BadDate)
        ));
    }

    #[test]
    fn kyiv_stamp_follows_dst() {
        assert_eq!(kyiv_stamp(now()), "2026-11-25 08:31");
        // Summer: EEST, +3.
        assert_eq!(
            kyiv_stamp(Utc.with_ymd_and_hms(2026, 7, 1, 21, 5, 0).unwrap()),
            "2026-07-02 00:05"
        );
    }

    #[test]
    fn writes_the_human_read_with_forced_date_region_and_stamp() {
        let vault = tempfile::tempdir().unwrap();
        let read = HumanRead {
            date: "1999-01-01".into(),
            region: Region::Us,
            side: HumanSide::NoTrade,
            at: "junk".into(),
            before_scale: true,
        };
        let rec =
            write_human_read(vault.path(), d(2026, 11, 25), Region::Eu, &read, now()).unwrap();
        let file = state_dir(vault.path()).join("human_read_2026-11-25_eu.json");
        let text = fs::read_to_string(&file).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "date": "2026-11-25", "region": "eu", "side": "NO_TRADE", "at": "2026-11-25 08:31", "before_scale": true })
        );
        assert_eq!(rec.date, "2026-11-25");
        assert!(
            text.starts_with("{\n \"date\""),
            "one-space indent like JSON.stringify(x, null, 1): {text}"
        );
        assert!(leftovers(&state_dir(vault.path())).is_empty());
    }

    #[test]
    fn writes_day_answers_with_a_one_line_note() {
        let vault = tempfile::tempdir().unwrap();
        let mut regions = BTreeMap::new();
        regions.insert(
            Region::Eu,
            DayAnswerRegion {
                trade: TradeTaken::Long,
                by_rule: Some(ByRule::Yes),
            },
        );
        regions.insert(
            Region::Us,
            DayAnswerRegion {
                trade: TradeTaken::None,
                by_rule: None,
            },
        );
        let long = format!("  помогло:\n терпение\t{}", "я".repeat(400));
        let a = DayAnswers {
            date: String::new(),
            at: String::new(),
            regions,
            note: long,
        };
        let rec = write_day_answers(vault.path(), d(2026, 11, 25), &a, now()).unwrap();
        assert!(rec.note.starts_with("помогло: терпение я"));
        assert_eq!(rec.note.chars().count(), NOTE_MAX_CHARS);
        let v: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(state_dir(vault.path()).join("day_answers_2026-11-25.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            v["regions"],
            serde_json::json!({ "eu": { "trade": "long", "by_rule": "yes" }, "us": { "trade": "none" } })
        );
        assert_eq!(v["at"], "2026-11-25 08:31");
    }

    #[test]
    fn overwrites_atomically_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        atomic_write(dir.path(), "x.json", b"{\"a\":1}").unwrap();
        atomic_write(dir.path(), "x.json", b"{\"a\":2}").unwrap();
        assert_eq!(fs::read(dir.path().join("x.json")).unwrap(), b"{\"a\":2}");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn a_failed_write_removes_its_temp() {
        let dir = tempfile::tempdir().unwrap();
        // The target is a non-empty directory: rename fails on every OS.
        let target = dir.path().join("busy.json");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"x").unwrap();
        assert!(atomic_write(dir.path(), "busy.json", b"{}").is_err());
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn empty_vault_is_refused() {
        let read = HumanRead {
            date: String::new(),
            region: Region::Eu,
            side: HumanSide::Long,
            at: String::new(),
            before_scale: false,
        };
        assert!(matches!(
            write_human_read(Path::new(""), d(2026, 1, 1), Region::Eu, &read, now()),
            Err(WriteError::NoVault)
        ));
    }

    #[test]
    fn retry_gives_transient_errors_a_few_chances() {
        let calls = Cell::new(0);
        let sharing = || io::Error::from_raw_os_error(32);
        let ok = retry(
            || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err(sharing())
                } else {
                    Ok(())
                }
            },
            |e| e.raw_os_error() == Some(32),
            &[1, 1, 1],
        );
        assert!(ok.is_ok());
        assert_eq!(calls.get(), 3);
        calls.set(0);
        let gave_up = retry(
            || {
                calls.set(calls.get() + 1);
                Err(sharing())
            },
            |e| e.raw_os_error() == Some(32),
            &[1, 1],
        );
        assert!(gave_up.is_err());
        assert_eq!(calls.get(), 3);
        calls.set(0);
        let fatal = retry(
            || {
                calls.set(calls.get() + 1);
                Err(io::Error::from(io::ErrorKind::NotFound))
            },
            |e| e.raw_os_error() == Some(32),
            &[1, 1],
        );
        assert!(fatal.is_err());
        assert_eq!(calls.get(), 1);
    }
}
