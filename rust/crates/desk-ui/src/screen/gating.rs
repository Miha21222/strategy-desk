//! «Сначала человек» (note 23) on screen. The rule itself — what a window
//! may show before the owner said his own read — is desk-core's
//! `windows::visibility` (the 16 rows of TEST_PLAN §4 are tested there); this
//! only turns its answer into the layout of a window's view.

use desk_core::windows::Visibility;

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

/// The branches at the top of `RegionView`, from desk-core's decision.
pub fn region_layout(v: Visibility, human: bool) -> RegionLayout {
    if v.offer_peek {
        return RegionLayout::AskOnly;
    }
    if !v.show_review {
        return RegionLayout::NoData { ask: v.ask_read };
    }
    RegionLayout::Review {
        top: if human {
            ReviewTop::Said
        } else if v.ask_read {
            ReviewTop::Ask
        } else {
            ReviewTop::Nothing
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(ask_read: bool, offer_peek: bool, show_review: bool) -> Visibility {
        Visibility {
            ask_read,
            offer_peek,
            show_review,
            ..Visibility::default()
        }
    }

    #[test]
    fn question_only_before_the_answer() {
        // Row 1: today, a review, no read, no peek.
        assert_eq!(
            region_layout(v(true, true, false), false),
            RegionLayout::AskOnly
        );
    }

    #[test]
    fn peek_keeps_the_question_on_top() {
        // Row 2.
        assert_eq!(
            region_layout(v(true, false, true), false),
            RegionLayout::Review {
                top: ReviewTop::Ask
            }
        );
    }

    #[test]
    fn answered_shows_the_read() {
        // Rows 3 and 7.
        assert_eq!(
            region_layout(v(false, false, true), true),
            RegionLayout::Review {
                top: ReviewTop::Said
            }
        );
    }

    #[test]
    fn no_review_asks_only_on_the_day() {
        // Rows 4, 5, 8, 9.
        assert_eq!(
            region_layout(v(true, false, false), false),
            RegionLayout::NoData { ask: true }
        );
        assert_eq!(
            region_layout(v(false, false, false), true),
            RegionLayout::NoData { ask: false }
        );
    }

    #[test]
    fn past_day_shows_the_review_alone() {
        // Row 6; row 16: «Сегодня помни» lives only in the Review layout.
        assert_eq!(
            region_layout(v(false, false, true), false),
            RegionLayout::Review {
                top: ReviewTop::Nothing
            }
        );
    }
}
