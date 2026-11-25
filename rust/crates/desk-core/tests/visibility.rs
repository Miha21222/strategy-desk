// GATE: compiled out until `desk_core::windows::visibility` exists. Delete the
// next line when it lands. (`any()` is always false; a named feature would
// need a Cargo.toml entry, and `golden` is on in CI, so it cannot be that.)
#![cfg(any())]
//! «Сначала человек» (note 23) as a pure decision, without the UI: given the
//! day, `now` and the window the owner chose to peek at, what is withheld.
//! The 16 rows of docs/rust/TEST_PLAN.md §4, plus the Kyiv-midnight edge.
//!
//! The rule is the old renderer's, read out of `src/renderer/macro/MacroDesk.tsx`:
//!
//! ```text
//! today      = kyivNow(now).date ; past = day.date < today
//! hasReview  = scale || verdict                                   (eu / us)
//! unread(id) = day.date == today && (id == review ? unread(eu) || unread(us)
//!                                     : hasReview && !human && peek != id)
//! askRead    = !human && !past                                    (RegionView)
//! screen     = !hasReview          → question (if askRead) + «нет данных»
//!              askRead && !peeked  → question only, with «Показать обзор без ответа»
//!              otherwise           → the review (with the read card if askRead || human)
//! before_scale of a read given now = !(peeked && hasReview)
//! review tab : askDay = !past || answers ; body shown when mech || agent
//! ```
//!
//! Proposed API (desk_core::windows):
//!
//! ```ignore
//! #[derive(Debug, Clone, Copy, PartialEq, Eq)]
//! pub struct Visibility {
//!     /// unread(): the tile hides its needle («сначала твой взгляд» / «после твоего взгляда»)
//!     /// and the agent strip hides its last message.
//!     pub withheld: bool,
//!     /// «Сначала твой взгляд» is on this window's screen (eu / us only).
//!     pub ask_read: bool,
//!     /// «Показать обзор без ответа» is offered.
//!     pub offer_peek: bool,
//!     /// The window's content is drawn: gauge, plan, factors, «Сегодня помни», «Что изменилось»
//!     /// (eu / us); the evening review body (review).
//!     pub show_review: bool,
//!     /// `before_scale` of a read the owner gives from this screen now.
//!     pub before_scale: bool,
//!     /// «Твой день» is asked (review only).
//!     pub ask_day: bool,
//! }
//! pub fn visibility(id: WindowId, day: &Day, now: DateTime<Utc>, peek: Option<WindowId>) -> Visibility;
//! ```
//!
//! `peek` is UI state (one window at a time, reset when the date changes); the
//! reset itself is a desk-app / desk-ui test, not this one.

mod common;

use chrono::{DateTime, Utc};
use common::*;
use desk_core::day::Day;
use desk_core::windows::{visibility, Visibility, WindowId};
use serde_json::{json, Value};

/// 2026-10-26 09:40 Kyiv (UTC+2 that day): the demo vault's «today».
const NOW: &str = "2026-10-26T07:40:00Z";

fn now(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("rfc3339")
        .with_timezone(&Utc)
}

/// The golden «today» (a copy of 14.09: both windows reviewed, the evening
/// review written, no read, no answers), reshaped per case.
fn base() -> Value {
    read_json(&golden().join("expected/day_2026-10-26.json"))
}

fn read(side: &str, before_scale: bool) -> Value {
    json!({"date": "2026-10-26", "region": "eu", "side": side, "at": "2026-10-26 09:31", "before_scale": before_scale})
}

struct Case {
    date: &'static str,
    /// eu / us: "none" | "scale" | "verdict" | "both"
    eu: &'static str,
    us: &'static str,
    eu_read: bool,
    us_read: bool,
    answers: bool,
}

const TODAY_FULL: Case = Case {
    date: "2026-10-26",
    eu: "both",
    us: "both",
    eu_read: false,
    us_read: false,
    answers: false,
};

fn day(c: &Case) -> Day {
    let mut d = base();
    d["date"] = json!(c.date);
    for (r, has, human) in [("eu", c.eu, c.eu_read), ("us", c.us, c.us_read)] {
        assert!(
            d[r]["scale"].is_object() && d[r]["verdict"].is_object(),
            "the base day has both files for {r}"
        );
        if !matches!(has, "scale" | "both") {
            d[r]["scale"] = Value::Null;
        }
        if !matches!(has, "verdict" | "both") {
            d[r]["verdict"] = Value::Null;
        }
        let mut h = read("SHORT", true);
        h["region"] = json!(r);
        h["date"] = json!(c.date);
        d[r]["human"] = if human { h } else { Value::Null };
    }
    d["answers"] = if c.answers {
        json!({"date": c.date, "at": format!("{} 20:05", c.date), "regions": {"eu": {"trade": "none"}}, "note": ""})
    } else {
        Value::Null
    };
    serde_json::from_value(d).expect("Day deserializes")
}

fn v(
    withheld: bool,
    ask_read: bool,
    offer_peek: bool,
    show_review: bool,
    before_scale: bool,
    ask_day: bool,
) -> Visibility {
    Visibility {
        withheld,
        ask_read,
        offer_peek,
        show_review,
        before_scale,
        ask_day,
    }
}

fn check(row: &str, id: WindowId, c: &Case, at: &str, peek: Option<WindowId>, want: Visibility) {
    let got = visibility(id, &day(c), now(at), peek);
    assert_eq!(
        got, want,
        "row {row}: {id:?}, day {}, now {at}, peek {peek:?}",
        c.date
    );
}

// ---------------------------------------------------------------- one window (eu / us)

#[test]
fn row_01_today_review_no_read_is_question_only() {
    check(
        "1",
        WindowId::Eu,
        &TODAY_FULL,
        NOW,
        None,
        v(true, true, true, false, true, false),
    );
    check(
        "1",
        WindowId::Us,
        &TODAY_FULL,
        NOW,
        None,
        v(true, true, true, false, true, false),
    );
}

#[test]
fn row_02_peek_shows_the_review_and_marks_a_later_read() {
    check(
        "2",
        WindowId::Eu,
        &TODAY_FULL,
        NOW,
        Some(WindowId::Eu),
        v(false, true, false, true, false, false),
    );
}

#[test]
fn row_03_read_given_shows_everything() {
    let c = Case {
        eu_read: true,
        ..TODAY_FULL
    };
    check(
        "3",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, false, false, true, true, false),
    );
}

#[test]
fn row_04_today_no_review_asks_and_says_no_data() {
    let c = Case {
        eu: "none",
        ..TODAY_FULL
    };
    check(
        "4",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, true, false, false, true, false),
    );
}

#[test]
fn row_05_today_no_review_read_given() {
    let c = Case {
        eu: "none",
        eu_read: true,
        ..TODAY_FULL
    };
    check(
        "5",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, false, false, false, true, false),
    );
}

#[test]
fn row_06_past_day_shows_the_review_without_asking() {
    let c = Case {
        date: "2026-10-23",
        ..TODAY_FULL
    };
    check(
        "6",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, false, false, true, true, false),
    );
}

#[test]
fn row_07_past_day_with_read() {
    let c = Case {
        date: "2026-10-23",
        eu_read: true,
        ..TODAY_FULL
    };
    check(
        "7",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, false, false, true, true, false),
    );
}

#[test]
fn row_08_past_day_without_review() {
    let c = Case {
        date: "2026-10-23",
        eu: "none",
        ..TODAY_FULL
    };
    check(
        "8",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, false, false, false, true, false),
    );
}

/// TS quirk Q12: a day after today still asks (`!past`), but withholds nothing (not today).
#[test]
fn row_09_future_day_asks_but_withholds_nothing() {
    let c = Case {
        date: "2026-10-27",
        eu: "none",
        us: "none",
        ..TODAY_FULL
    };
    check(
        "9",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(false, true, false, false, true, false),
    );
}

#[test]
fn row_13_peek_at_another_window_does_not_count() {
    check(
        "13",
        WindowId::Eu,
        &TODAY_FULL,
        NOW,
        Some(WindowId::Us),
        v(true, true, true, false, true, false),
    );
}

#[test]
fn row_14_a_verdict_alone_is_a_review() {
    let c = Case {
        eu: "verdict",
        ..TODAY_FULL
    };
    check(
        "14",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(true, true, true, false, true, false),
    );
    let c = Case {
        eu: "scale",
        ..TODAY_FULL
    };
    check(
        "14",
        WindowId::Eu,
        &c,
        NOW,
        None,
        v(true, true, true, false, true, false),
    );
}

/// Row 15 (demo vault) is the same decision; that the send buttons are off is desk-app's.
/// Row 16: «Сегодня помни» / «Что изменилось» are part of `show_review`, so withheld with the gauge.
#[test]
fn row_16_memory_cards_follow_the_gauge() {
    let d = day(&TODAY_FULL);
    assert!(
        d.eu.remember.is_some(),
        "the base day has «Сегодня помни» for Europe"
    );
    assert!(!visibility(WindowId::Eu, &d, now(NOW), None).show_review);
}

// ---------------------------------------------------------------- the evening review

#[test]
fn row_10_evening_tile_withheld_while_europe_unread() {
    let c = Case {
        us_read: true,
        ..TODAY_FULL
    };
    // TS quirk (TEST_PLAN Q23): only the tile and the agent strip are withheld;
    // the review body itself is drawn when the owner opens the evening tab.
    check(
        "10",
        WindowId::Review,
        &c,
        NOW,
        None,
        v(true, false, false, true, true, true),
    );
}

#[test]
fn row_11_evening_tile_withheld_while_us_unread() {
    let c = Case {
        eu_read: true,
        ..TODAY_FULL
    };
    check(
        "11",
        WindowId::Review,
        &c,
        NOW,
        None,
        v(true, false, false, true, true, true),
    );
}

#[test]
fn row_12_evening_tile_shown_once_both_are_answered_or_peeked() {
    let c = Case {
        eu_read: true,
        us_read: true,
        ..TODAY_FULL
    };
    check(
        "12",
        WindowId::Review,
        &c,
        NOW,
        None,
        v(false, false, false, true, true, true),
    );
    // 14b-us-peek: Europe answered, the US peeked → «2 из 2 верно» on the tile.
    let c = Case {
        eu_read: true,
        ..TODAY_FULL
    };
    check(
        "12",
        WindowId::Review,
        &c,
        NOW,
        Some(WindowId::Us),
        v(false, false, false, true, true, true),
    );
}

#[test]
fn evening_form_is_asked_today_and_on_a_past_day_only_when_answered() {
    let past = Case {
        date: "2026-10-23",
        eu_read: true,
        us_read: true,
        ..TODAY_FULL
    };
    check(
        "6/review",
        WindowId::Review,
        &past,
        NOW,
        None,
        v(false, false, false, true, true, false),
    );
    let answered = Case {
        answers: true,
        ..past
    };
    check(
        "7/review",
        WindowId::Review,
        &answered,
        NOW,
        None,
        v(false, false, false, true, true, true),
    );
}

// ---------------------------------------------------------------- the clock is Kyiv's

/// «Today» is the Kyiv date of `now`, not the UTC one: at 21:59Z on 25.10 it is
/// still the 25th in Kyiv (the 26th is tomorrow: ask, withhold nothing); at
/// 22:00Z it is the 26th (withhold).
#[test]
fn today_turns_at_kyiv_midnight() {
    check(
        "9",
        WindowId::Eu,
        &TODAY_FULL,
        "2026-10-25T21:59:00Z",
        None,
        v(false, true, true, false, true, false),
    );
    check(
        "1",
        WindowId::Eu,
        &TODAY_FULL,
        "2026-10-25T22:00:00Z",
        None,
        v(true, true, true, false, true, false),
    );
    // Summer time: Kyiv is UTC+3, midnight is 21:00Z.
    let c = Case {
        date: "2026-07-01",
        ..TODAY_FULL
    };
    check(
        "9",
        WindowId::Eu,
        &c,
        "2026-06-30T20:59:00Z",
        None,
        v(false, true, true, false, true, false),
    );
    check(
        "1",
        WindowId::Eu,
        &c,
        "2026-06-30T21:00:00Z",
        None,
        v(true, true, true, false, true, false),
    );
}
