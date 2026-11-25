//! The three windows of a day — Europe, US, evening review — and where each
//! stands: `windowState` / `defaultWindow` / `kyivNow` of `src/shared/macro.ts`.
//!
//! TZ item 21 changes one thing on purpose: the window clocks are no longer
//! the fixed `WINDOW_TIMES` table. They come from the exchanges' own zones —
//! Xetra opens 09:00 Europe/Berlin, the NYSE 09:30 America/New_York — turned
//! into Kyiv wall time for the day in question. Each window starts 30 minutes
//! before its open and is "due" until 2 hours after it, which reproduces the
//! old table (09:30 / 10:00 / 12:00 and 16:00 / 16:30 / 18:30) on ordinary
//! days. In the weeks when the US and the EU change clocks on different
//! dates (March and late October), New York is one hour closer and the US
//! window moves to 15:00 / 15:30 / 17:30 by itself. The review stays at
//! 20:00–21:30 Kyiv: it follows the owner's evening, not an exchange.
//!
//! Time is always an argument; nothing here reads the clock.

use crate::day::Day;
use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;

/// The desk's clock.
pub const KYIV: Tz = chrono_tz::Europe::Kyiv;
/// Xetra's zone.
pub const XETRA: Tz = chrono_tz::Europe::Berlin;
/// The NYSE's zone.
pub const NEW_YORK: Tz = chrono_tz::America::New_York;

/// `MacroWindowId`: the three daily review windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowId {
    Eu,
    Us,
    Review,
}

impl WindowId {
    /// `MACRO_WINDOWS`, in the order of the day.
    pub const ALL: [WindowId; 3] = [WindowId::Eu, WindowId::Us, WindowId::Review];

    pub fn as_str(self) -> &'static str {
        match self {
            WindowId::Eu => "eu",
            WindowId::Us => "us",
            WindowId::Review => "review",
        }
    }
}

/// `WindowState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowState {
    /// The skill's files are all there.
    Ready,
    /// The mechanics ran but the agent has not written its part yet (or the reverse).
    Partial,
    /// Not yet time.
    Waiting,
    /// Its time has come and nothing is there.
    Due,
    /// The day (or the window) is over without it.
    Missed,
}

/// Kyiv clock of a window and the cash open it prepares for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowTimes {
    /// When the review of the window runs (and the tile turns «due»).
    pub at: NaiveTime,
    /// The cash open, Kyiv time; `None` for the evening review.
    pub open: Option<NaiveTime>,
    /// Until when the window is «due» rather than «missed».
    pub until: NaiveTime,
}

impl WindowTimes {
    fn from_open(open: NaiveTime) -> Self {
        WindowTimes {
            at: open - chrono::Duration::minutes(30),
            open: Some(open),
            until: open + chrono::Duration::hours(2),
        }
    }
}

fn hm(h: u32, m: u32) -> NaiveTime {
    // Only called with literal clock times below 24:00, which always exist.
    NaiveTime::from_hms_opt(h, m, 0).unwrap_or(NaiveTime::MIN)
}

/// The Kyiv wall time of `h:m` in `tz` on `date`, or `None` if that local
/// time does not exist (never for a 09:xx open: clocks change at night).
fn in_kyiv(tz: Tz, date: NaiveDate, h: u32, m: u32) -> Option<NaiveTime> {
    let local = date.and_hms_opt(h, m, 0)?;
    let t = tz.from_local_datetime(&local).earliest()?;
    Some(t.with_timezone(&KYIV).time())
}

/// The old fixed table (`WINDOW_TIMES` of macro.ts): what ordinary days give.
pub fn standard_times(id: WindowId) -> WindowTimes {
    match id {
        WindowId::Eu => WindowTimes::from_open(hm(10, 0)),
        WindowId::Us => WindowTimes::from_open(hm(16, 30)),
        WindowId::Review => WindowTimes {
            at: hm(20, 0),
            open: None,
            until: hm(21, 30),
        },
    }
}

/// The window's Kyiv clock on `date`, from the exchange's own time zone (TZ item 21).
pub fn window_times(id: WindowId, date: NaiveDate) -> WindowTimes {
    let open = match id {
        WindowId::Eu => in_kyiv(XETRA, date, 9, 0),
        WindowId::Us => in_kyiv(NEW_YORK, date, 9, 30),
        WindowId::Review => return standard_times(WindowId::Review),
    };
    open.map_or_else(|| standard_times(id), WindowTimes::from_open)
}

/// `kyivNow`: Kyiv wall clock as a calendar day and minutes since midnight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KyivNow {
    pub date: NaiveDate,
    pub minutes: u32,
}

impl KyivNow {
    pub fn new(date: NaiveDate, hour: u32, minute: u32) -> Self {
        KyivNow {
            date,
            minutes: hour * 60 + minute,
        }
    }
}

/// `kyivNow(now)`.
pub fn kyiv_now(now: DateTime<Utc>) -> KyivNow {
    let k = now.with_timezone(&KYIV);
    KyivNow {
        date: k.date_naive(),
        minutes: k.hour() * 60 + k.minute(),
    }
}

/// Kyiv wall clock for the `at` fields of the owner's files: «2026-10-12 09:31».
pub fn kyiv_stamp(now: DateTime<Utc>) -> String {
    now.with_timezone(&KYIV)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

fn minutes(t: NaiveTime) -> u32 {
    t.hour() * 60 + t.minute()
}

/// What the files say about a window, before the clock is asked.
fn data_state(id: WindowId, day: &Day) -> Option<WindowState> {
    let (a, b) = match id {
        WindowId::Review => (
            day.review.mech.as_ref().is_some_and(|m| m.is_truthy()),
            day.review.agent.as_ref().is_some_and(|m| m.is_truthy()),
        ),
        WindowId::Eu | WindowId::Us => {
            let r = if id == WindowId::Eu { &day.eu } else { &day.us };
            (
                r.scale.is_some(),
                r.verdict.as_ref().is_some_and(|v| v.is_truthy()),
            )
        }
    };
    match (a, b) {
        (true, true) => Some(WindowState::Ready),
        (true, false) | (false, true) => Some(WindowState::Partial),
        (false, false) => None,
    }
}

/// `windowState(id, day, date, now)`: data present (`Ready`; `Partial` when
/// one half is there), not yet time (`Waiting` — until 5 minutes before the
/// window), its time has come and nothing is there (`Due`, until the window
/// closes), or the day is over without it (`Missed`).
pub fn window_state_at(
    id: WindowId,
    day: Option<&Day>,
    date: NaiveDate,
    now: KyivNow,
) -> WindowState {
    if let Some(s) = day.and_then(|d| data_state(id, d)) {
        return s;
    }
    if date > now.date {
        return WindowState::Waiting;
    }
    if date < now.date {
        return WindowState::Missed;
    }
    let w = window_times(id, date);
    if i64::from(now.minutes) < i64::from(minutes(w.at)) - 5 {
        return WindowState::Waiting;
    }
    if now.minutes <= minutes(w.until) {
        return WindowState::Due;
    }
    WindowState::Missed
}

/// The contract's `window_state(region, now, day)`: where window `id` of
/// `day` stands at the instant `now`. A day with no files is an empty `Day`
/// (what [`crate::day::load_day`] gives for it), which reads like the TS's
/// `day = null`.
pub fn window_state(id: WindowId, now: DateTime<Utc>, day: &Day) -> WindowState {
    window_state_at(id, Some(day), day.date, kyiv_now(now))
}

/// `defaultWindow`: the window worth opening first for a day — the latest one
/// that has something, else the current one.
pub fn default_window_at(day: Option<&Day>, date: NaiveDate, now: KyivNow) -> WindowId {
    let review = |d: &Day| {
        d.review.mech.as_ref().is_some_and(|m| m.is_truthy())
            || d.review.agent.as_ref().is_some_and(|m| m.is_truthy())
    };
    let us = |d: &Day| d.us.scale.is_some() || d.us.verdict.as_ref().is_some_and(|v| v.is_truthy());
    if let Some(d) = day.filter(|_| date == now.date) {
        let evening = now.minutes >= minutes(hm(19, 30));
        let afternoon = now.minutes >= minutes(hm(13, 0));
        if evening && review(d) {
            return WindowId::Review;
        }
        if afternoon && us(d) {
            return WindowId::Us;
        }
        if evening {
            return WindowId::Review;
        }
        if afternoon {
            return WindowId::Us;
        }
        return WindowId::Eu;
    }
    match day {
        Some(d) if review(d) => WindowId::Review,
        Some(d) if us(d) => WindowId::Us,
        _ => WindowId::Eu,
    }
}

/// [`default_window_at`] for a loaded day at the instant `now`.
pub fn default_window(day: &Day, now: DateTime<Utc>) -> WindowId {
    default_window_at(Some(day), day.date, kyiv_now(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::day::{load_day, DayInput};
    use serde_json::json;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn at(date: &str, hhmm: &str) -> KyivNow {
        let (h, m) = hhmm.split_once(':').unwrap();
        KyivNow::new(d(date), h.parse().unwrap(), m.parse().unwrap())
    }

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    fn empty(date: &str) -> Day {
        load_day(&DayInput::default(), d(date))
    }

    fn with(date: &str, files: &[(&str, serde_json::Value)]) -> Day {
        let mut input = DayInput::default();
        for (name, v) in files {
            input.insert(
                crate::names::state_path(name),
                serde_json::to_vec(v).unwrap(),
            );
        }
        load_day(&input, d(date))
    }

    // ---- ported from macro.test.ts

    #[test]
    fn says_when_a_window_is_due_waiting_or_missed() {
        let day = d("2026-10-12");
        assert_eq!(
            window_state_at(WindowId::Eu, None, day, at("2026-10-12", "09:00")),
            WindowState::Waiting
        );
        assert_eq!(
            window_state_at(WindowId::Eu, None, day, at("2026-10-12", "09:31")),
            WindowState::Due
        );
        assert_eq!(
            window_state_at(WindowId::Eu, None, day, at("2026-10-12", "12:30")),
            WindowState::Missed
        );
        assert_eq!(
            window_state_at(WindowId::Us, None, day, at("2026-10-12", "15:58")),
            WindowState::Due
        );
        assert_eq!(
            window_state_at(
                WindowId::Review,
                None,
                d("2026-10-11"),
                at("2026-10-12", "09:00")
            ),
            WindowState::Missed
        );
        assert_eq!(
            window_state_at(
                WindowId::Review,
                None,
                d("2026-10-13"),
                at("2026-10-12", "09:00")
            ),
            WindowState::Waiting
        );
    }

    #[test]
    fn reads_ready_and_partial_from_the_files_present() {
        let scale = json!({"region": "eu", "date": "2026-10-12", "asof": "", "sym": "FDAX", "factors": [], "result": {"long": 0, "short": 0, "undecided": 100, "needle": 0, "zone": "", "side": null}, "light": {"color": "green", "label": ""}});
        let d1 = with("2026-10-12", &[("scale_2026-10-12_eu.json", scale.clone())]);
        assert_eq!(
            window_state_at(WindowId::Eu, Some(&d1), d1.date, at("2026-10-12", "09:40")),
            WindowState::Partial
        );
        let d2 = with(
            "2026-10-12",
            &[
                ("scale_2026-10-12_eu.json", scale),
                (
                    "macro_2026-10-12_eu_verdict.json",
                    json!({"verdict": "LONG"}),
                ),
            ],
        );
        assert_eq!(
            window_state_at(WindowId::Eu, Some(&d2), d2.date, at("2026-10-12", "09:40")),
            WindowState::Ready
        );
        // A verdict file that holds `false` is not a verdict (JS truthiness).
        let d3 = with(
            "2026-10-12",
            &[("macro_2026-10-12_eu_verdict.json", json!(false))],
        );
        assert_eq!(
            window_state_at(WindowId::Eu, Some(&d3), d3.date, at("2026-10-12", "09:40")),
            WindowState::Due
        );
    }

    #[test]
    fn opens_the_window_that_matters_now() {
        let day = empty("2026-10-12");
        assert_eq!(
            default_window_at(Some(&day), day.date, at("2026-10-12", "09:40")),
            WindowId::Eu
        );
        assert_eq!(
            default_window_at(Some(&day), day.date, at("2026-10-12", "16:05")),
            WindowId::Us
        );
        assert_eq!(
            default_window_at(Some(&day), day.date, at("2026-10-12", "20:10")),
            WindowId::Review
        );
        let rev = with(
            "2026-10-09",
            &[("review_2026-10-09.json", json!({"date": "", "regions": {}}))],
        );
        assert_eq!(
            default_window_at(Some(&rev), rev.date, at("2026-10-12", "09:00")),
            WindowId::Review
        );
        assert_eq!(
            default_window_at(None, d("2026-10-09"), at("2026-10-12", "09:00")),
            WindowId::Eu
        );
    }

    // ---- TZ item 21: the clocks follow the exchanges

    #[test]
    fn ordinary_days_give_the_old_table() {
        for date in ["2026-01-15", "2026-07-01", "2026-10-12", "2026-11-25"] {
            for id in WindowId::ALL {
                assert_eq!(
                    window_times(id, d(date)),
                    standard_times(id),
                    "{date} {id:?}"
                );
            }
        }
        assert_eq!(standard_times(WindowId::Eu).at, t("09:30"));
        assert_eq!(standard_times(WindowId::Us).until, t("18:30"));
    }

    #[test]
    fn the_us_window_moves_to_15_00_while_the_clocks_disagree() {
        // 2026: US summer time from Sun 8 March, EU from Sun 29 March;
        // EU winter time from Sun 25 October, US from Sun 1 November.
        for date in [
            "2026-03-09",
            "2026-03-20",
            "2026-03-27",
            "2026-10-26",
            "2026-10-30",
        ] {
            let w = window_times(WindowId::Us, d(date));
            assert_eq!(
                (w.at, w.open, w.until),
                (t("15:00"), Some(t("15:30")), t("17:30")),
                "{date}"
            );
            // Europe does not move: Kyiv and Berlin change together.
            assert_eq!(
                window_times(WindowId::Eu, d(date)),
                standard_times(WindowId::Eu),
                "{date}"
            );
        }
        for date in ["2026-03-06", "2026-03-30", "2026-10-23", "2026-11-02"] {
            assert_eq!(
                window_times(WindowId::Us, d(date)),
                standard_times(WindowId::Us),
                "{date}"
            );
        }
        // 2027 has the same gaps on its own dates.
        assert_eq!(window_times(WindowId::Us, d("2027-03-15")).at, t("15:00"));
        assert_eq!(window_times(WindowId::Us, d("2027-04-01")).at, t("16:00"));
    }

    #[test]
    fn window_state_uses_the_shifted_clock() {
        let day = empty("2026-03-09");
        // Kyiv is UTC+2 on 9 March: 12:55 UTC = 14:55 Kyiv = 5 minutes before 15:00.
        assert_eq!(
            window_state(WindowId::Us, utc("2026-03-09T12:54:00Z"), &day),
            WindowState::Waiting
        );
        assert_eq!(
            window_state(WindowId::Us, utc("2026-03-09T12:55:00Z"), &day),
            WindowState::Due
        );
        assert_eq!(
            window_state(WindowId::Us, utc("2026-03-09T15:30:00Z"), &day),
            WindowState::Due
        );
        assert_eq!(
            window_state(WindowId::Us, utc("2026-03-09T15:31:00Z"), &day),
            WindowState::Missed
        );
        // The same Kyiv minute on an ordinary day is still waiting.
        let plain = empty("2026-03-02");
        assert_eq!(
            window_state(WindowId::Us, utc("2026-03-02T12:55:00Z"), &plain),
            WindowState::Waiting
        );
    }

    #[test]
    fn kyiv_clock_across_its_own_dst_changes() {
        // 29 March 2026, 01:00 UTC = 04:00 EEST (clocks jumped 03:00 → 04:00).
        assert_eq!(
            kyiv_now(utc("2026-03-29T01:00:00Z")),
            at("2026-03-29", "04:00")
        );
        assert_eq!(
            kyiv_now(utc("2026-03-29T00:59:00Z")),
            at("2026-03-29", "02:59")
        );
        // 25 October 2026, 00:30 UTC = 03:30 EEST, 01:30 UTC = 03:30 EET (the hour repeats).
        assert_eq!(
            kyiv_now(utc("2026-10-25T00:30:00Z")),
            at("2026-10-25", "03:30")
        );
        assert_eq!(
            kyiv_now(utc("2026-10-25T01:30:00Z")),
            at("2026-10-25", "03:30")
        );
        // Late evening UTC is already the next Kyiv day.
        assert_eq!(
            kyiv_now(utc("2026-11-24T22:10:00Z")),
            at("2026-11-25", "00:10")
        );
        assert_eq!(kyiv_stamp(utc("2026-10-12T06:31:59Z")), "2026-10-12 09:31");
    }

    #[test]
    fn the_day_boundary_is_kyiv_midnight() {
        let day = empty("2026-11-25");
        // 21:59 UTC on the 24th is 23:59 Kyiv on the 24th: the 25th is still ahead.
        assert_eq!(
            window_state(WindowId::Eu, utc("2026-11-24T21:59:00Z"), &day),
            WindowState::Waiting
        );
        // 22:00 UTC on the 25th is 00:00 Kyiv on the 26th: the 25th is over.
        assert_eq!(
            window_state(WindowId::Review, utc("2026-11-25T22:00:00Z"), &day),
            WindowState::Missed
        );
        assert_eq!(
            window_state(WindowId::Review, utc("2026-11-25T19:30:00Z"), &day),
            WindowState::Due
        );
    }
}
