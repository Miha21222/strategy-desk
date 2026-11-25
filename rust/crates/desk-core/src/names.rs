//! File names of the vault contract, built only from a checked date
//! (`NaiveDate`), and the reverse: which date a state file belongs to.
//! From `src/shared/macro.ts` (`humanReadFile`, `dateOfStateFile`,
//! `isShotPath`, …) and `src/main/macro-state.ts` (the names `loadDay` reads).

use crate::js;
use crate::vault::{paths, Region};
use chrono::NaiveDate;

/// `DATELESS_CHANGE`: the one state file without a date (`ideas.json`); a
/// change to it concerns whatever day is on screen.
pub const DATELESS_CHANGE: &str = "*";

/// The state files of a day, by what they hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StateKind {
    /// `scale_<D>_<r>.json` — the needle.
    Scale(Region),
    /// `macro_<D>_<r>.json` — the collect (news, calendar, markets).
    Macro(Region),
    /// `macro_<D>_<r>_verdict.json` — the agent's verdict.
    Verdict(Region),
    /// `shots_<D>_<r>.json` — screenshots of the window.
    Shots(Region),
    /// `shots_<D>_<r>_review.json` — the 20:00 screenshots.
    ShotsReview(Region),
    /// `human_read_<D>_<r>.json` — the owner's read (written by the desk).
    HumanRead(Region),
    /// `remember_<D>_<r>.json` — «Сегодня помни».
    Remember(Region),
    /// `drift_<D>_<r>.json` — «Что изменилось».
    Drift(Region),
    /// `similar_<D>_<r>.json` — «Похожие дни» (`scale.py similar`).
    Similar(Region),
    /// `review_<D>.json` — the mechanical evening review.
    Review,
    /// `review_<D>_agent.json` — the agent's evening words.
    ReviewAgent,
    /// `day_answers_<D>.json` — the owner's evening answers (written by the desk).
    DayAnswers,
    /// `week_<D>.json` — the week in five lines (D = the Friday).
    Week,
}

/// The name of a state file (inside `indices-kb/state/`). The contract's
/// `state_file(kind, date, region)`, with the region carried by the kind,
/// because half of the kinds have none.
pub fn state_file(kind: StateKind, date: NaiveDate) -> String {
    let d = date.format("%Y-%m-%d");
    match kind {
        StateKind::Scale(r) => format!("scale_{d}_{r}.json"),
        StateKind::Macro(r) => format!("macro_{d}_{r}.json"),
        StateKind::Verdict(r) => format!("macro_{d}_{r}_verdict.json"),
        StateKind::Shots(r) => format!("shots_{d}_{r}.json"),
        StateKind::ShotsReview(r) => format!("shots_{d}_{r}_review.json"),
        StateKind::HumanRead(r) => format!("human_read_{d}_{r}.json"),
        StateKind::Remember(r) => format!("remember_{d}_{r}.json"),
        StateKind::Drift(r) => format!("drift_{d}_{r}.json"),
        StateKind::Similar(r) => format!("similar_{d}_{r}.json"),
        StateKind::Review => format!("review_{d}.json"),
        StateKind::ReviewAgent => format!("review_{d}_agent.json"),
        StateKind::DayAnswers => format!("day_answers_{d}.json"),
        StateKind::Week => format!("week_{d}.json"),
    }
}

/// `humanReadFile`.
pub fn human_read_file(date: NaiveDate, region: Region) -> String {
    state_file(StateKind::HumanRead(region), date)
}

/// `dayAnswersFile`.
pub fn day_answers_file(date: NaiveDate) -> String {
    state_file(StateKind::DayAnswers, date)
}

/// `rememberFile`.
pub fn remember_file(date: NaiveDate, region: Region) -> String {
    state_file(StateKind::Remember(region), date)
}

/// `driftFile`.
pub fn drift_file(date: NaiveDate, region: Region) -> String {
    state_file(StateKind::Drift(region), date)
}

/// `weekFile`.
pub fn week_file(date: NaiveDate) -> String {
    state_file(StateKind::Week, date)
}

/// `ideasFile`: the dateless list.
pub const IDEAS_FILE: &str = "ideas.json";

/// The journal files `loadDay` reads (inside `indices-kb/journal/`).
pub const LESSONS_FILE: &str = "lessons.md";
pub const SCORES_FILE: &str = "scale_factor_scores.csv";

/// The prefixes `dateOfStateFile` knows.
const DATED_PREFIXES: [&str; 11] = [
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
    // Not in the TS list (the file is new); the watcher must see it change.
    "similar",
];

/// `dateOfStateFile`: which date a state file belongs to —
/// `scale_2026-10-09_us.json` → `2026-10-09`. The date is only shaped like
/// one (`\d{4}-\d{2}-\d{2}`), as in the TS; it is not checked to exist.
pub fn date_of_state_file(name: &str) -> Option<&str> {
    for p in DATED_PREFIXES {
        let Some(rest) = name.strip_prefix(p).and_then(|r| r.strip_prefix('_')) else {
            continue;
        };
        let b = rest.as_bytes();
        let shaped = b.len() >= 11
            && b[..10].iter().enumerate().all(|(i, c)| {
                if i == 4 || i == 7 {
                    *c == b'-'
                } else {
                    c.is_ascii_digit()
                }
            })
            && (b[10] == b'_' || b[10] == b'.');
        if shaped {
            return Some(&rest[..10]);
        }
    }
    None
}

/// What the watcher reports for a changed state file: its date, or
/// [`DATELESS_CHANGE`] for `ideas.json`, or nothing (not a desk file).
pub fn change_of_state_file(name: &str) -> Option<&str> {
    if name == IDEAS_FILE {
        Some(DATELESS_CHANGE)
    } else {
        date_of_state_file(name)
    }
}

/// `isFriday`: dates in the vault are Kyiv calendar days, so the weekday
/// needs no time zone. Like V8, `2026-02-30` is read as 2 March.
pub fn is_friday(date: &str) -> bool {
    let b = date.as_bytes();
    let shaped = b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    // 1970-01-01 was a Thursday, so day 1 is a Friday.
    shaped && js::iso_day_number(date).is_some_and(|d| d.rem_euclid(7) == 1)
}

/// `isShotPath`: a vault-relative screenshot path the UI may ask for — `.png`
/// under `indices-kb/shots/`, no `..`, not absolute.
pub fn is_shot_path(rel: &str) -> bool {
    if rel.is_empty() || js::utf16_len(rel) > 400 {
        return false;
    }
    let p = rel.replace('\\', "/");
    let drive = p.as_bytes().get(1) == Some(&b':') && p.as_bytes()[0].is_ascii_alphabetic();
    if p.starts_with('/') || drive || p.contains('\0') {
        return false;
    }
    let parts: Vec<&str> = p.split('/').collect();
    if parts
        .iter()
        .any(|x| *x == ".." || *x == "." || x.is_empty())
    {
        return false;
    }
    parts.len() > 3
        && parts[0] == paths::SHOTS[0]
        && parts[1] == paths::SHOTS[1]
        && p.len() >= 4
        && p.is_char_boundary(p.len() - 4)
        && p[p.len() - 4..].eq_ignore_ascii_case(".png")
}

/// The pure half of `resolveNote`: a note path the UI may ask to open — a
/// markdown file inside the vault's notes folder, `..` resolved lexically.
/// Returns the normalized vault-relative path (forward slashes).
pub fn note_path(rel: &str) -> Option<String> {
    if !rel.ends_with(".md") {
        return None;
    }
    let p = rel.replace('\\', "/");
    let drive = p.as_bytes().get(1) == Some(&b':') && p.as_bytes()[0].is_ascii_alphabetic();
    if p.starts_with('/') || drive || p.contains('\0') {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            x => out.push(x),
        }
    }
    (out.len() >= 2 && out[0] == paths::NOTES[0]).then(|| out.join("/"))
}

/// The vault-relative path of a window's note: `Макро-обзоры/<D> Европа.md`.
pub fn region_note(date: NaiveDate, region: Region) -> String {
    format!(
        "{}/{} {}.md",
        paths::NOTES[0],
        date.format("%Y-%m-%d"),
        region.note_word()
    )
}

/// The vault-relative path of the evening note: `Макро-обзоры/<D> Итог дня.md`.
pub fn review_note(date: NaiveDate) -> String {
    format!(
        "{}/{} Итог дня.md",
        paths::NOTES[0],
        date.format("%Y-%m-%d")
    )
}

/// `<folder>/<name>` with the folder given as path segments.
pub fn join(dir: &[&str], name: &str) -> String {
    let mut s = dir.join("/");
    s.push('/');
    s.push_str(name);
    s
}

/// Vault-relative path of a state file.
pub fn state_path(name: &str) -> String {
    join(&paths::STATE, name)
}

/// Vault-relative path of a journal file.
pub fn journal_path(name: &str) -> String {
    join(&paths::JOURNAL, name)
}

/// Vault-relative path of `config/scale.json`.
pub fn config_path() -> String {
    paths::CONFIG.join("/")
}

/// A date the desk may build an owner's file name from: `%Y-%m-%d` of it
/// must be exactly ten characters (year 1000–9999), or `date_of_state_file`
/// (`_\d{4}-\d{2}-\d{2}`) would not recognise the name the desk wrote —
/// chrono happily formats year 10000 as `+10000-01-01`.
pub fn checked_file_date(date: NaiveDate) -> Result<NaiveDate, crate::CoreError> {
    use chrono::Datelike;
    if (1000..=9999).contains(&date.year()) {
        Ok(date)
    } else {
        Err(crate::CoreError::BadDate(
            date.format("%Y-%m-%d").to_string(),
        ))
    }
}

/// Parse an ISO date the way the owner's two files require it (`/^\d{4}-\d{2}-\d{2}$/`
/// and a real calendar day).
pub fn parse_date(s: &str) -> Result<NaiveDate, crate::CoreError> {
    let b = s.as_bytes();
    let shaped = b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        });
    if !shaped {
        return Err(crate::CoreError::BadDate(s.to_owned()));
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| crate::CoreError::BadDate(s.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        parse_date(s).unwrap()
    }

    #[test]
    fn owner_file_dates_have_four_digit_years() {
        let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(
            checked_file_date(ymd(2026, 10, 8)).unwrap(),
            ymd(2026, 10, 8)
        );
        assert!(checked_file_date(ymd(1000, 1, 1)).is_ok());
        assert!(checked_file_date(ymd(9999, 12, 31)).is_ok());
        assert!(checked_file_date(ymd(999, 12, 31)).is_err());
        assert!(checked_file_date(ymd(10000, 1, 1)).is_err());
        assert!(checked_file_date(ymd(-1, 1, 1)).is_err());
        // Every accepted date makes a name the reverse lookup reads back.
        let name = human_read_file(checked_file_date(ymd(1000, 1, 1)).unwrap(), Region::Eu);
        assert_eq!(date_of_state_file(&name), Some("1000-01-01"));
    }

    #[test]
    fn knows_which_date_a_state_file_belongs_to() {
        assert_eq!(
            date_of_state_file("scale_2026-10-09_us.json"),
            Some("2026-10-09")
        );
        assert_eq!(
            date_of_state_file("review_2026-10-08.json"),
            Some("2026-10-08")
        );
        assert_eq!(
            date_of_state_file("review_2026-10-08_agent.json"),
            Some("2026-10-08")
        );
        assert_eq!(
            date_of_state_file("macro_2026-10-07_eu_verdict.json"),
            Some("2026-10-07")
        );
        assert_eq!(date_of_state_file("scale_backtest_us.json"), None);
        assert_eq!(date_of_state_file("bias_2026-10-05.json"), None);
        assert_eq!(
            date_of_state_file("shots_2026-10-09_us.json"),
            Some("2026-10-09")
        );
        assert_eq!(
            date_of_state_file("human_read_2026-10-12_us.json"),
            Some("2026-10-12")
        );
        assert_eq!(
            date_of_state_file("day_answers_2026-10-12.json"),
            Some("2026-10-12")
        );
        assert_eq!(
            date_of_state_file("remember_2026-10-12_eu.json"),
            Some("2026-10-12")
        );
        assert_eq!(
            date_of_state_file("drift_2026-10-12_us.json"),
            Some("2026-10-12")
        );
        assert_eq!(
            date_of_state_file("week_2026-10-16.json"),
            Some("2026-10-16")
        );
        assert_eq!(date_of_state_file("ideas.json"), None);
        // Shaped like a date is enough, as in the TS; a temp file is not one.
        assert_eq!(
            date_of_state_file("regime_2026-99-99.json"),
            Some("2026-99-99")
        );
        assert_eq!(date_of_state_file("scale_2026-10-09"), None);
        assert_eq!(date_of_state_file("scale_2026-10-09x.json"), None);
        assert_eq!(date_of_state_file("Scale_2026-10-09_us.json"), None);
        assert_eq!(change_of_state_file("ideas.json"), Some(DATELESS_CHANGE));
        assert_eq!(DATELESS_CHANGE, "*");
    }

    #[test]
    fn names_the_files() {
        let day = d("2026-10-12");
        assert_eq!(
            human_read_file(day, Region::Us),
            "human_read_2026-10-12_us.json"
        );
        assert_eq!(day_answers_file(day), "day_answers_2026-10-12.json");
        assert_eq!(
            remember_file(day, Region::Eu),
            "remember_2026-10-12_eu.json"
        );
        assert_eq!(drift_file(day, Region::Us), "drift_2026-10-12_us.json");
        assert_eq!(week_file(d("2026-10-16")), "week_2026-10-16.json");
        assert_eq!(
            state_file(StateKind::Verdict(Region::Eu), day),
            "macro_2026-10-12_eu_verdict.json"
        );
        assert_eq!(
            state_file(StateKind::ShotsReview(Region::Us), day),
            "shots_2026-10-12_us_review.json"
        );
        assert_eq!(
            region_note(day, Region::Eu),
            "Макро-обзоры/2026-10-12 Европа.md"
        );
        assert_eq!(review_note(day), "Макро-обзоры/2026-10-12 Итог дня.md");
        assert_eq!(state_path("ideas.json"), "indices-kb/state/ideas.json");
        assert_eq!(config_path(), "indices-kb/config/scale.json");
        // Every dated name reads back to its date.
        for k in [
            StateKind::Scale(Region::Eu),
            StateKind::Review,
            StateKind::Week,
            StateKind::HumanRead(Region::Us),
        ] {
            assert_eq!(date_of_state_file(&state_file(k, day)), Some("2026-10-12"));
        }
    }

    #[test]
    fn knows_a_friday() {
        assert!(is_friday("2026-09-18"));
        assert!(is_friday("2026-10-09"));
        assert!(!is_friday("2026-09-14"));
        assert!(!is_friday("2026-10-12"));
        assert!(!is_friday("nope"));
        assert!(!is_friday(""));
        // V8 rolls 2026-02-30 over to Monday 2 March; 2026-02-27 is a Friday.
        assert!(is_friday("2026-02-27"));
        assert!(!is_friday("2026-02-30"));
        assert!(!is_friday("2026-02-32"));
        assert!(is_friday("1970-01-02"));
    }

    #[test]
    fn accepts_only_pngs_under_indices_kb_shots() {
        assert!(is_shot_path("indices-kb/shots/2026-10-09/macro_us/m15.png"));
        assert!(is_shot_path("indices-kb\\shots\\2026-10-09\\m15.png"));
        assert!(is_shot_path("indices-kb/shots/a/B.PNG"));
        assert!(!is_shot_path("indices-kb/shots/../state/x.png"));
        assert!(!is_shot_path("indices-kb/shots/2026-10-09/m15.jpg"));
        assert!(!is_shot_path("indices-kb/state/m15.png"));
        assert!(!is_shot_path("/etc/indices-kb/shots/a/b.png"));
        assert!(!is_shot_path("C:/indices-kb/shots/a/b.png"));
        assert!(!is_shot_path("indices-kb/shots/m15.png"));
        assert!(!is_shot_path("indices-kb/shots//a.png"));
        assert!(!is_shot_path(""));
        assert!(!is_shot_path(&format!(
            "indices-kb/shots/{}/a.png",
            "x".repeat(400)
        )));
        assert!(!is_shot_path("indices-kb/shots/a/я.пнг"));
    }

    #[test]
    fn opens_only_markdown_notes_inside_the_notes_folder() {
        assert_eq!(
            note_path("Макро-обзоры/2026-10-08 Европа.md").as_deref(),
            Some("Макро-обзоры/2026-10-08 Европа.md")
        );
        assert_eq!(
            note_path("Макро-обзоры\\2026-10-08 Европа.md").as_deref(),
            Some("Макро-обзоры/2026-10-08 Европа.md")
        );
        assert_eq!(note_path("Макро-обзоры/../AGENTS.md"), None);
        assert_eq!(note_path("Макро-обзоры/x.exe"), None);
        assert_eq!(note_path("../Макро-обзоры/x.md"), None);
        assert_eq!(note_path("/Макро-обзоры/x.md"), None);
        assert_eq!(note_path("Макро-обзоры.md"), None);
        assert_eq!(
            note_path("x/../Макро-обзоры/a.md").as_deref(),
            Some("Макро-обзоры/a.md")
        );
    }

    #[test]
    fn dates_are_checked() {
        assert!(parse_date("2026-10-08").is_ok());
        assert!(parse_date("2026-10-08x").is_err());
        assert!(parse_date("../2026-10-08").is_err());
        assert!(parse_date("2026-02-30").is_err());
        assert!(parse_date("2026-1-08").is_err());
    }
}
