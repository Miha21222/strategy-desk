//! «Сначала человек» (note 23): what a window shows before the owner said
//! his own read. A port of `unread()` and of the branches at the top of
//! `RegionView` in `MacroDesk.tsx`; the truth table is section 4 of
//! docs/rust/TEST_PLAN.md (16 rows), tested below row by row.

use chrono::NaiveDate;
use desk_core::{Day, Region, WindowId};

/// A window has a review when its scale or its verdict is there
/// (`Boolean(r.scale || r.verdict)`).
pub fn has_review(day: &Day, r: Region) -> bool {
    let s = day.region(r);
    s.scale.is_some() || s.verdict.as_ref().is_some_and(|v| v.is_truthy())
}

pub fn has_human(day: &Day, r: Region) -> bool {
    day.region(r).human.is_some()
}

fn region_of(id: WindowId) -> Option<Region> {
    match id {
        WindowId::Eu => Some(Region::Eu),
        WindowId::Us => Some(Region::Us),
        WindowId::Review => None,
    }
}

/// The facts `unread` looks at, for one day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub is_today: bool,
    /// `[eu, us]`
    pub review: [bool; 2],
    pub human: [bool; 2],
}

impl Facts {
    pub fn of(day: Option<&Day>, date: NaiveDate, today: NaiveDate) -> Option<Facts> {
        let day = day?;
        Some(Facts {
            is_today: date == today,
            review: [has_review(day, Region::Eu), has_review(day, Region::Us)],
            human: [has_human(day, Region::Eu), has_human(day, Region::Us)],
        })
    }
}

/// `unread(id)`: on the day itself, a window with a review and no read from
/// the owner withholds its direction — the tile's needle, the agent's text,
/// the plan — until he answers or peeks. The evening tile hides too while
/// either window does: «N из 2 верно» would tell on the scale.
pub fn unread(id: WindowId, facts: Option<Facts>, peek: Option<WindowId>) -> bool {
    let Some(f) = facts.filter(|f| f.is_today) else {
        return false;
    };
    match region_of(id) {
        None => unread(WindowId::Eu, facts, peek) || unread(WindowId::Us, facts, peek),
        Some(r) => {
            let i = r as usize;
            f.review[i] && !f.human[i] && peek != Some(id)
        }
    }
}

/// What the region view puts on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionLayout {
    /// No review: the question (when it is asked) above «Обзора не было».
    NoData { ask: bool },
    /// The review is there, the owner has not answered: only the question,
    /// with «Показать обзор без ответа».
    AskOnly,
    /// The whole review; `top` says what sits above it.
    Review { top: ReviewTop },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewTop {
    Nothing,
    /// The question, after a peek (an answer now is recorded `before_scale: false`).
    Ask,
    /// The owner's read and how it compares with the scale.
    Said,
}

/// The branches at the top of `RegionView`.
pub fn region_layout(has_review: bool, human: bool, past: bool, peeked: bool) -> RegionLayout {
    let ask = !human && !past;
    if !has_review {
        return RegionLayout::NoData { ask };
    }
    if ask && !peeked {
        return RegionLayout::AskOnly;
    }
    RegionLayout::Review {
        top: if human {
            ReviewTop::Said
        } else if ask {
            ReviewTop::Ask
        } else {
            ReviewTop::Nothing
        },
    }
}

/// `beforeScale: !(peek && hasReview)` of `sendRead`.
pub fn before_scale(peeked: bool, has_review: bool) -> bool {
    !(peeked && has_review)
}

#[cfg(test)]
mod tests {
    use super::*;
    use RegionLayout::*;
    use ReviewTop::*;

    const EU: WindowId = WindowId::Eu;
    const US: WindowId = WindowId::Us;
    const RV: WindowId = WindowId::Review;

    fn facts(is_today: bool, review: [bool; 2], human: [bool; 2]) -> Option<Facts> {
        Some(Facts {
            is_today,
            review,
            human,
        })
    }

    // Rows of TEST_PLAN.md §4. `past` = the day is before today.

    #[test]
    fn row01_today_review_no_read_shows_only_the_question() {
        let f = facts(true, [true, false], [false, false]);
        assert!(unread(EU, f, None), "tile needle hidden");
        assert_eq!(region_layout(true, false, false, false), AskOnly);
    }

    #[test]
    fn row02_peek_shows_the_review_with_the_question_on_top() {
        let f = facts(true, [true, false], [false, false]);
        assert!(!unread(EU, f, Some(EU)));
        assert_eq!(region_layout(true, false, false, true), Review { top: Ask });
        assert!(
            !before_scale(true, true),
            "answer after peek: before_scale false"
        );
    }

    #[test]
    fn row03_today_answered_shows_read_and_review() {
        let f = facts(true, [true, false], [true, false]);
        assert!(!unread(EU, f, None));
        assert_eq!(
            region_layout(true, true, false, false),
            Review { top: Said }
        );
    }

    #[test]
    fn row04_today_no_review_no_read_asks_above_no_data() {
        let f = facts(true, [false, false], [false, false]);
        assert!(!unread(EU, f, None));
        assert_eq!(
            region_layout(false, false, false, false),
            NoData { ask: true }
        );
        assert!(before_scale(false, false));
        // Even after a peek there was nothing to see: still before the scale.
        assert!(before_scale(true, false));
    }

    #[test]
    fn row05_today_no_review_with_read_shows_no_data_only() {
        assert_eq!(
            region_layout(false, true, false, false),
            NoData { ask: false }
        );
    }

    #[test]
    fn row06_past_review_no_read_shows_review_without_question() {
        let f = facts(false, [true, true], [false, false]);
        assert!(!unread(EU, f, None));
        assert_eq!(
            region_layout(true, false, true, false),
            Review { top: Nothing }
        );
    }

    #[test]
    fn row07_past_review_with_read_shows_read() {
        assert_eq!(region_layout(true, true, true, false), Review { top: Said });
    }

    #[test]
    fn row08_past_no_review_no_question() {
        assert_eq!(
            region_layout(false, false, true, false),
            NoData { ask: false }
        );
        assert_eq!(
            region_layout(false, true, true, false),
            NoData { ask: false }
        );
    }

    #[test]
    fn row09_future_day_asks_like_the_old_code() {
        // A future day is not `past`: the question is asked; no review yet.
        let f = facts(false, [false, false], [false, false]);
        assert!(!unread(EU, f, None));
        assert_eq!(
            region_layout(false, false, false, false),
            NoData { ask: true }
        );
    }

    #[test]
    fn row10_review_tile_hidden_while_eu_hidden() {
        let f = facts(true, [true, true], [false, true]);
        assert!(unread(RV, f, None));
    }

    #[test]
    fn row11_review_tile_hidden_while_us_hidden() {
        let f = facts(true, [true, true], [true, false]);
        assert!(unread(RV, f, None));
    }

    #[test]
    fn row12_review_tile_shown_when_both_answered() {
        let f = facts(true, [true, true], [true, true]);
        assert!(!unread(RV, f, None));
        // A window without a review does not hide the evening either.
        let f = facts(true, [true, false], [true, false]);
        assert!(!unread(RV, f, None));
    }

    #[test]
    fn row13_peek_is_per_window_and_reset_with_the_day() {
        let f = facts(true, [true, true], [false, false]);
        // Peeking at EU does not open US.
        assert!(!unread(EU, f, Some(EU)));
        assert!(unread(US, f, Some(EU)));
        // The screen drops `peek` on a day change (ScreenState::go); back on
        // the day the question is there again.
        assert!(unread(EU, f, None));
    }

    #[test]
    fn row14_partial_verdict_only_counts_as_a_review() {
        // Facts::of takes a truthy verdict as a review (has_review); the rule
        // is the same as row 1.
        let f = facts(true, [true, false], [false, false]);
        assert!(unread(EU, f, None));
        assert_eq!(region_layout(true, false, false, false), AskOnly);
    }

    #[test]
    fn row15_demo_vault_gates_the_same() {
        // The demo flag only disables the chat buttons; gating is row 1.
        let f = facts(true, [true, true], [false, false]);
        assert!(unread(EU, f, None) && unread(US, f, None) && unread(RV, f, None));
    }

    #[test]
    fn row16_remember_card_goes_with_the_gauge() {
        // «Сегодня помни» is drawn only in the Review layout, never in AskOnly.
        assert!(!matches!(
            region_layout(true, false, false, false),
            Review { .. }
        ));
    }

    #[test]
    fn no_day_hides_nothing() {
        assert!(!unread(EU, None, None));
        assert!(!unread(RV, None, None));
    }
}
