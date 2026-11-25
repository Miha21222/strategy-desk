//! The pure half of `writeHumanRead` / `writeDayAnswers` (macro-state.ts):
//! the record the desk writes and its exact text. The atomic temp-file +
//! rename is desk-app's (CONTRACTS «Запись в vault»); the file name comes
//! from [`crate::names`], built only from a checked date.

use crate::js;
use crate::vault::{DayAnswerRegions, DayAnswers, HumanRead, HumanSide, Region};
use crate::windows::kyiv_stamp;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

/// The longest «Что помогло или помешало» kept, in UTF-16 units (as `slice(0, 300)`).
pub const NOTE_MAX: usize = 300;

/// The owner's read of a window, stamped with the Kyiv clock of `now`.
/// `before_scale` is what the screen showed: no gauge yet.
pub fn human_read_record(
    date: NaiveDate,
    region: Region,
    side: HumanSide,
    before_scale: bool,
    now: DateTime<Utc>,
) -> HumanRead {
    HumanRead {
        date: date.format("%Y-%m-%d").to_string(),
        region,
        side,
        at: kyiv_stamp(now),
        before_scale,
    }
}

/// The owner's evening answers, stamped; the note is cut to one short line.
pub fn day_answers_record(
    date: NaiveDate,
    regions: DayAnswerRegions,
    note: &str,
    now: DateTime<Utc>,
) -> DayAnswers {
    let note = js::collapse_spaces(note);
    DayAnswers {
        date: date.format("%Y-%m-%d").to_string(),
        at: kyiv_stamp(now),
        regions,
        note: js::slice16(js::trim(&note), NOTE_MAX).to_owned(),
    }
}

/// `JSON.stringify(record, null, 1)`: the text of an owner's file.
pub fn owner_file_text<T: Serialize>(record: &T) -> Result<String, crate::CoreError> {
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    record.serialize(&mut ser)?;
    // serde_json writes valid UTF-8 by construction.
    Ok(String::from_utf8(out).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::{normalize_day_answers, normalize_human_read};
    use crate::vault::{ByRule, DayAnswerRegion, TradeTaken};

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn writes_the_read_as_the_ts_did_and_reads_it_back() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
        let r = human_read_record(
            d,
            Region::Eu,
            HumanSide::Short,
            true,
            utc("2026-10-08T06:31:00Z"),
        );
        let text = owner_file_text(&r).unwrap();
        assert_eq!(text, "{\n \"date\": \"2026-10-08\",\n \"region\": \"eu\",\n \"side\": \"SHORT\",\n \"at\": \"2026-10-08 09:31\",\n \"before_scale\": true\n}");
        let back = normalize_human_read(
            &serde_json::from_str(&text).unwrap(),
            "2026-10-08",
            Region::Eu,
        )
        .unwrap();
        assert_eq!(back, r);
    }

    #[test]
    fn cuts_the_note_to_one_line() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 8).unwrap();
        let regions = DayAnswerRegions {
            eu: Some(DayAnswerRegion {
                trade: TradeTaken::Short,
                by_rule: Some(ByRule::Yes),
            }),
            us: Some(DayAnswerRegion {
                trade: TradeTaken::None,
                by_rule: None,
            }),
        };
        let a = day_answers_record(
            d,
            regions,
            "  ждал   вторую\nманипуляцию ",
            utc("2026-10-08T17:05:00Z"),
        );
        assert_eq!(a.note, "ждал вторую манипуляцию");
        assert_eq!(a.at, "2026-10-08 20:05");
        let text = owner_file_text(&a).unwrap();
        assert!(text.contains("\"regions\": {\n  \"eu\": {\n   \"trade\": \"short\",\n   \"by_rule\": \"yes\"\n  },\n  \"us\": {\n   \"trade\": \"none\"\n  }\n }"), "{text}");
        let back =
            normalize_day_answers(&serde_json::from_str(&text).unwrap(), "2026-10-08").unwrap();
        assert_eq!(back, a);
        let long = day_answers_record(
            d,
            DayAnswerRegions::default(),
            &"я".repeat(400),
            utc("2026-10-08T17:05:00Z"),
        );
        assert_eq!(long.note.chars().count(), NOTE_MAX);
    }
}
