//! What the desk sends to the agent, as pure string builders: `runCommand`,
//! `discussPrompt`, `macroChatTitle` of `src/shared/macro.ts`. The Russian
//! text is the skill's contract and stays word for word.

use crate::js;
use crate::vault::{Region, Side};
use crate::windows::WindowId;
use chrono::NaiveDate;

/// `runCommand`: the slash command the skill answers to, per window, with
/// the day on screen as its second argument. Without it the skill takes
/// «today», and a review run post factum for Monday would be written as
/// Tuesday's.
pub fn run_command(id: WindowId, date: NaiveDate) -> String {
    format!("/macro-review {} {}", id.as_str(), date.format("%Y-%m-%d"))
}

/// `discussPrompt`: an argument from the owner, phrased so the skill's
/// «Обсуждение после отчёта» path picks it up — side and share are explicit,
/// so the agent does not have to ask, and it still answers with its own
/// position before recording it. The share is rounded and kept in 1–50 %.
pub fn discuss_prompt(
    region: Region,
    side: Side,
    share: f64,
    text: &str,
    date: NaiveDate,
) -> String {
    let where_ = match region {
        Region::Eu => "Европа",
        Region::Us => "США",
    };
    // Math.max(1, Math.min(50, Math.round(share))): NaN stays NaN, as in JS.
    let s = js::round(share);
    let s = if s.is_nan() { s } else { s.clamp(1.0, 50.0) };
    let d = date.format("%Y-%m-%d");
    let side_word = match side {
        Side::Long => "лонг",
        Side::Short => "шорт",
    };
    [
        format!("Довод по шкале ({where_}, регион {region}, день {d}): {}", js::trim(text)),
        format!("Сторона: {side_word} ({}). Доля: {} %.", side.as_str(), js::number_to_string(s)),
        format!(
            "Ответь своей позицией (согласен / частично / не согласен) с причиной из данных, запиши довод через scale.py discuss --date {d} и скажи, как изменились стрелка и план."
        ),
    ]
    .join("\n")
}

/// `macroChatTitle`: title of the chat that carries one day of reviews.
pub fn macro_chat_title(date: NaiveDate) -> String {
    format!("Обзор дня {}", date.format("%d.%m"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn states_side_and_share_so_the_skill_does_not_have_to_ask() {
        let p = discuss_prompt(
            Region::Eu,
            Side::Long,
            80.0,
            "  DAX у недельного минимума  ",
            d("2026-10-12"),
        );
        assert!(p.contains("регион eu"));
        assert!(p.contains("день 2026-10-12"));
        assert!(p.contains("DAX у недельного минимума"));
        assert!(p.contains("(LONG)"));
        assert!(p.contains("Доля: 50 %"));
        assert!(p.contains("scale.py discuss --date 2026-10-12"));
        assert_eq!(
            p,
            "Довод по шкале (Европа, регион eu, день 2026-10-12): DAX у недельного минимума\n\
             Сторона: лонг (LONG). Доля: 50 %.\n\
             Ответь своей позицией (согласен / частично / не согласен) с причиной из данных, запиши довод через scale.py discuss --date 2026-10-12 и скажи, как изменились стрелка и план."
        );
    }

    #[test]
    fn clamps_and_rounds_the_share() {
        let share = |x: f64| {
            let p = discuss_prompt(Region::Us, Side::Short, x, "x", d("2026-10-12"));
            p.lines().nth(1).unwrap().to_owned()
        };
        assert_eq!(share(0.2), "Сторона: шорт (SHORT). Доля: 1 %.");
        assert_eq!(share(12.5), "Сторона: шорт (SHORT). Доля: 13 %.");
        assert_eq!(share(f64::NAN), "Сторона: шорт (SHORT). Доля: NaN %.");
        assert!(
            discuss_prompt(Region::Us, Side::Short, 5.0, "x", d("2026-10-12"))
                .starts_with("Довод по шкале (США, регион us")
        );
    }

    #[test]
    fn runs_the_skill_for_the_day_on_screen_never_for_today() {
        assert_eq!(
            run_command(WindowId::Review, d("2026-10-12")),
            "/macro-review review 2026-10-12"
        );
        assert_eq!(
            run_command(WindowId::Eu, d("2026-10-13")),
            "/macro-review eu 2026-10-13"
        );
    }

    #[test]
    fn names_the_day_chat_by_date() {
        assert_eq!(macro_chat_title(d("2026-10-12")), "Обзор дня 12.10");
    }
}
