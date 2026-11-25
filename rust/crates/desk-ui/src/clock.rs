//! The desk's clock. The real one in normal runs; a fixed instant for the
//! demo vault, tests and screenshots (`--now` / `DESK_NOW`), the way the TS
//! preview faked the Kyiv clock with `NOW=`. desk-core never reads the clock
//! itself (CONTRACTS.md), so this is the only place `Utc::now()` is called.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use desk_core::windows::{kyiv_now, KyivNow, KYIV};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    Real,
    Fixed(DateTime<Utc>),
}

impl Clock {
    pub fn now(self) -> DateTime<Utc> {
        match self {
            Clock::Real => Utc::now(),
            Clock::Fixed(t) => t,
        }
    }

    pub fn kyiv(self) -> KyivNow {
        kyiv_now(self.now())
    }

    pub fn today(self) -> NaiveDate {
        self.kyiv().date
    }
}

/// `2026-10-26T10:15` (Kyiv wall clock), `2026-10-26 10:15`, or RFC 3339
/// with an offset (`2026-10-26T07:15:00Z`).
pub fn parse_now(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc));
    }
    ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(s, f).ok())
        .and_then(|local| KYIV.from_local_datetime(&local).earliest())
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kyiv_wall_clock_and_rfc3339() {
        let a = parse_now("2026-10-26T10:15").unwrap();
        // Kyiv is UTC+2 on 26 Oct 2026 (after the switch on the 25th).
        assert_eq!(a, Utc.with_ymd_and_hms(2026, 10, 26, 8, 15, 0).unwrap());
        assert_eq!(parse_now("2026-10-26 10:15"), Some(a));
        assert_eq!(parse_now("2026-10-26T08:15:00Z"), Some(a));
        assert_eq!(parse_now("tomorrow"), None);
        let c = Clock::Fixed(a);
        assert_eq!(c.today(), NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(c.kyiv().minutes, 10 * 60 + 15);
    }
}
