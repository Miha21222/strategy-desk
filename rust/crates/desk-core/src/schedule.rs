//! The schedule: when each window starts on a day, which Task Scheduler jobs
//! to register, and whether a job that just fired is the one whose window it is.
//!
//! The Windows Task Scheduler fires at fixed local minutes, Monday to Friday,
//! while the US window moves between 16:00 and 15:00 Kyiv in the weeks the US
//! and the EU change clocks on different dates (TZ item 21,
//! [`crate::windows::window_times`]). [`planned_runs`] is when each window
//! really starts on a given day; the jobs are registered on the union of
//! those minutes over a year ([`triggers`]: EU 09:30, US 15:00 and 16:00,
//! review 20:00), and the headless `run` asks [`due_window`] whether this
//! firing is today's real one — otherwise it exits 0 without work.
//!
//! The old app had three jobs at 09:30 / 16:00 / 20:00 and a fourth US one
//! «at 15:00 on the weeks of different clock changes» in its vault script.

use crate::settings::Settings;
use crate::windows::{window_times, WindowId, KYIV};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use serde::Serialize;

/// Every job of this desk is named «Strategy Desk <window> <HH:MM>».
pub const TASK_PREFIX: &str = "Strategy Desk ";

/// The Electron desk's jobs (`SCHEDULER_TASKS`). Registering this desk's jobs
/// removes them: with both sets the skill would run twice per window.
pub const LEGACY_TASKS: [&str; 3] = ["Trading Macro EU", "Trading Macro US", "Trading Review"];

/// A firing counts for its window from the planned minute to 30 minutes after
/// it: a PC that woke up late still runs the review, while the other US
/// trigger, one hour away, never counts.
pub const TOLERANCE: Duration = Duration::minutes(30);

/// How far ahead [`triggers`] looks for the minutes a window can start at.
const HORIZON_DAYS: i64 = 366;

/// A window and a Kyiv wall-clock minute: when it starts on a day
/// ([`planned_runs`]), or a Task Scheduler job, Monday to Friday ([`triggers`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct PlannedRun {
    pub window: WindowId,
    pub at_local: NaiveTime,
}

fn window_on(settings: &Settings, id: WindowId) -> bool {
    match id {
        WindowId::Eu => settings.schedule.eu,
        WindowId::Us => settings.schedule.us,
        WindowId::Review => settings.schedule.review,
    }
}

/// Monday to Friday. Exchange holidays are not known here: the run goes, and
/// the skill sees a closed market.
pub fn is_trading_day(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// When each window switched on in the settings starts on `date`, Kyiv time,
/// in window order; nothing on a weekend. Independent of `schedule.enabled`
/// (that says whether the jobs should exist; the wizard shows the times either way).
pub fn planned_runs(date: NaiveDate, settings: &Settings) -> Vec<PlannedRun> {
    if !is_trading_day(date) {
        return Vec::new();
    }
    WindowId::ALL
        .into_iter()
        .filter(|id| window_on(settings, *id))
        .map(|id| PlannedRun {
            window: id,
            at_local: window_times(id, date).at,
        })
        .collect()
}

/// The jobs to register from `date` on: the union of [`planned_runs`] over
/// the next year, in window order, earliest first.
pub fn triggers(date: NaiveDate, settings: &Settings) -> Vec<PlannedRun> {
    let mut runs: Vec<PlannedRun> = Vec::new();
    for k in 0..HORIZON_DAYS {
        let Some(d) = date.checked_add_signed(Duration::days(k)) else {
            break;
        };
        for run in planned_runs(d, settings) {
            if !runs.contains(&run) {
                runs.push(run);
            }
        }
    }
    runs.sort();
    runs
}

/// The job's name in the Task Scheduler: «Strategy Desk US 15:00».
pub fn task_name(run: &PlannedRun) -> String {
    let w = match run.window {
        WindowId::Eu => "EU",
        WindowId::Us => "US",
        WindowId::Review => "Review",
    };
    format!("{TASK_PREFIX}{w} {}", run.at_local.format("%H:%M"))
}

/// The window whose planned start is now: today (Kyiv) has a [`planned_runs`]
/// entry with start ≤ `now` < start + [`TOLERANCE`]. `None` = this firing is
/// not a window's (the other US trigger, a weekend, a PC that woke too late, a
/// window switched off): the run exits 0 without work. Not gated by
/// `schedule.enabled`: a job that fires was registered, and switching the
/// schedule off is desk-app's unregistering.
pub fn due_window(now: DateTime<Utc>, settings: &Settings) -> Option<WindowId> {
    let date = now.with_timezone(&KYIV).date_naive();
    planned_runs(date, settings).into_iter().find_map(|r| {
        let start = KYIV
            .from_local_datetime(&date.and_time(r.at_local))
            .earliest()?;
        let d = now.signed_duration_since(start.with_timezone(&Utc));
        (d >= Duration::zero() && d < TOLERANCE).then_some(r.window)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ScheduleSettings;
    use std::collections::BTreeSet;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    fn on() -> Settings {
        Settings {
            schedule: ScheduleSettings {
                enabled: true,
                ..ScheduleSettings::default()
            },
            ..Settings::default()
        }
    }

    /// The instant the Task Scheduler fires a job on a Kyiv-clock PC.
    fn fire(date: NaiveDate, at: NaiveTime) -> DateTime<Utc> {
        KYIV.from_local_datetime(&date.and_time(at))
            .earliest()
            .unwrap()
            .with_timezone(&Utc)
    }

    fn days(from: &str, to: &str) -> impl Iterator<Item = NaiveDate> {
        let (a, b) = (d(from), d(to));
        a.iter_days().take_while(move |x| *x <= b)
    }

    #[test]
    fn registers_both_us_minutes_and_one_for_the_others() {
        for start in ["2026-01-01", "2026-11-27", "2027-01-01"] {
            let names: Vec<String> = triggers(d(start), &on()).iter().map(task_name).collect();
            assert_eq!(
                names,
                [
                    "Strategy Desk EU 09:30",
                    "Strategy Desk US 15:00",
                    "Strategy Desk US 16:00",
                    "Strategy Desk Review 20:00"
                ],
                "{start}"
            );
        }
        let no_us = Settings {
            schedule: ScheduleSettings {
                us: false,
                ..on().schedule
            },
            ..on()
        };
        assert_eq!(triggers(d("2026-11-27"), &no_us).len(), 2);
        // The plan does not depend on the switch that says whether to register it.
        assert_eq!(triggers(d("2026-11-27"), &Settings::default()).len(), 4);
    }

    #[test]
    fn planned_runs_are_the_days_real_starts() {
        let s = on();
        let at = |day: &str| {
            planned_runs(d(day), &s)
                .iter()
                .map(|r| r.at_local.format("%H:%M").to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(at("2026-03-06"), ["09:30", "16:00", "20:00"]);
        assert_eq!(at("2026-03-09"), ["09:30", "15:00", "20:00"]);
        assert!(at("2026-03-28").is_empty(), "Saturday");
        let no_review = Settings {
            schedule: ScheduleSettings {
                review: false,
                ..s.schedule
            },
            ..s
        };
        assert_eq!(planned_runs(d("2026-03-06"), &no_review).len(), 2);
    }

    /// Every trading day of 2026 and 2027, every registered firing: exactly one
    /// firing per window does the work, and it is the one at that day's start.
    #[test]
    fn exactly_one_firing_per_window_per_trading_day_2026_2027() {
        let s = on();
        for year in ["2026", "2027"] {
            let jobs = triggers(d(&format!("{year}-01-01")), &s);
            for day in days(&format!("{year}-01-01"), &format!("{year}-12-31")) {
                let mut did: Vec<PlannedRun> = Vec::new();
                for j in &jobs {
                    if let Some(w) = due_window(fire(day, j.at_local), &s) {
                        assert_eq!(w, j.window, "{day} {} woke another window", j.at_local);
                        did.push(*j);
                    }
                }
                assert_eq!(did, planned_runs(day, &s), "{day}");
            }
        }
    }

    #[test]
    fn the_us_runs_at_15_00_exactly_in_the_mismatch_weeks() {
        let s = on();
        let mut early: BTreeSet<NaiveDate> = BTreeSet::new();
        for day in days("2026-01-01", "2027-12-31").filter(|x| is_trading_day(*x)) {
            match (
                due_window(fire(day, t("15:00")), &s),
                due_window(fire(day, t("16:00")), &s),
            ) {
                (Some(WindowId::Us), None) => {
                    early.insert(day);
                }
                (None, Some(WindowId::Us)) => {}
                other => panic!("{day}: {other:?}"),
            }
        }
        let want: BTreeSet<NaiveDate> = [
            // 2026: US summer time from 8 March, EU from 29 March; EU winter time from 25 October, US from 1 November.
            days("2026-03-09", "2026-03-27").collect::<Vec<_>>(),
            days("2026-10-26", "2026-10-30").collect(),
            // 2027: US from 14 March, EU from 28 March; EU back on 31 October, US on 7 November.
            days("2027-03-15", "2027-03-26").collect(),
            days("2027-11-01", "2027-11-05").collect(),
        ]
        .concat()
        .into_iter()
        .filter(|x| is_trading_day(*x))
        .collect();
        assert_eq!(early, want);
    }

    #[test]
    fn europe_and_the_review_never_move_not_even_on_kyivs_own_change() {
        let s = on();
        // The Mondays after Kyiv's own changes (29 March and 25 October 2026, 28 March and 31 October 2027).
        for day in ["2026-03-30", "2026-10-26", "2027-03-29", "2027-11-01"] {
            assert_eq!(
                due_window(fire(d(day), t("09:30")), &s),
                Some(WindowId::Eu),
                "{day}"
            );
            assert_eq!(
                due_window(fire(d(day), t("20:00")), &s),
                Some(WindowId::Review),
                "{day}"
            );
        }
        // The same Kyiv minute is a different UTC instant across the change.
        assert_eq!(
            fire(d("2026-03-27"), t("09:30")).to_rfc3339(),
            "2026-03-27T07:30:00+00:00"
        );
        assert_eq!(
            fire(d("2026-03-30"), t("09:30")).to_rfc3339(),
            "2026-03-30T06:30:00+00:00"
        );
    }

    #[test]
    fn the_tolerance_edges() {
        let s = on();
        let day = d("2026-11-27"); // a Friday, ordinary week
        let at = |m: i64| fire(day, t("16:00")) + Duration::minutes(m);
        assert_eq!(due_window(at(0), &s), Some(WindowId::Us));
        assert_eq!(due_window(at(-1), &s), None);
        assert_eq!(due_window(at(29), &s), Some(WindowId::Us));
        assert_eq!(due_window(at(30), &s), None);
        // The 15:00 firing on an ordinary day is an hour early: no work.
        assert_eq!(due_window(fire(day, t("15:00")), &s), None);
    }

    #[test]
    fn nothing_runs_for_a_window_switched_off_or_on_a_weekend() {
        let day = d("2026-11-27");
        // `enabled` is about registering the jobs, not about a job that fired.
        assert_eq!(
            due_window(fire(day, t("09:30")), &Settings::default()),
            Some(WindowId::Eu)
        );
        let no_eu = Settings {
            schedule: ScheduleSettings {
                eu: false,
                ..on().schedule
            },
            ..on()
        };
        assert_eq!(due_window(fire(day, t("09:30")), &no_eu), None);
        assert_eq!(
            due_window(fire(day, t("20:00")), &no_eu),
            Some(WindowId::Review)
        );
        assert_eq!(due_window(fire(d("2026-11-28"), t("09:30")), &on()), None);
        assert_eq!(due_window(fire(d("2026-11-29"), t("20:00")), &on()), None);
    }
}
