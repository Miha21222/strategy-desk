//! The memory cards of note 23 §2–3 (`memoryCards.tsx`): «Сегодня помни»,
//! «Что изменилось», «Неделя в пять строк», «О тебе», «Идеи на проверке».
//! Read-only, and SILENT when there is nothing to say: no empty frame.

use desk_core::normalize::visible_ideas;
use desk_core::vault::{
    Drift, IdeaStatus, Ideas, NumOrStr, Remember, RememberKind, ShareProposal, Week,
};
use eframe::egui::{self, Ui};

use super::kit::{self, bold, txt, CardKind};
use crate::i18n::{t, tf};
use crate::theme::{space, text, Palette};

/// «1 раз», «3 раза», «5 раз».
pub fn times(n: u64) -> String {
    let (m10, m100) = (n % 10, n % 100);
    let key = if m10 == 1 && m100 != 11 {
        "macro.timesOne"
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        "macro.timesFew"
    } else {
        "macro.timesMany"
    };
    tf(key, &[("n", &n)])
}

fn kind_key(k: RememberKind) -> &'static str {
    match k {
        RememberKind::Lesson => "macro.rememberKind.lesson",
        RememberKind::Rule => "macro.rememberKind.rule",
        RememberKind::Similar => "macro.rememberKind.similar",
    }
}

/// «Сегодня помни»: up to three lines picked for this window.
pub fn remember(ui: &mut Ui, p: &Palette, r: Option<&Remember>) {
    let Some(r) = r.filter(|r| !r.items.is_empty()) else {
        return;
    };
    kit::card(
        ui,
        p,
        Some(t("macro.rememberTitle")),
        CardKind::Memo,
        |ui| {
            for it in &r.items {
                ui.horizontal_top(|ui| {
                    let kc = match it.kind {
                        RememberKind::Lesson => p.accent,
                        RememberKind::Rule => p.warn,
                        RememberKind::Similar => p.fg_muted,
                    };
                    ui.add_sized(
                        [110.0, 20.0],
                        egui::Label::new(bold(t(kind_key(it.kind)).to_uppercase(), text::SM, kc)),
                    );
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = space::S1;
                        let mut line = it.text.clone();
                        if !it.ref_.is_empty() {
                            line.push_str(" · ");
                            line.push_str(&it.ref_);
                        }
                        kit::para(ui, txt(line, text::LG, p.fg));
                        if !it.why.is_empty() {
                            kit::para(ui, txt(&it.why, text::SM, p.fg_muted));
                        }
                    });
                });
            }
        },
    );
}

/// «Что изменилось»: one line per signal; a warning keeps the warning tint.
pub fn drift(ui: &mut Ui, p: &Palette, d: Option<&Drift>) {
    let Some(d) = d.filter(|d| !d.signals.is_empty()) else {
        return;
    };
    kit::card(ui, p, Some(t("macro.driftTitle")), CardKind::Memo, |ui| {
        for s in &d.signals {
            let (dot, fg) = if s.severity == "warn" {
                (p.warn, p.warn)
            } else {
                (p.border_strong, p.fg)
            };
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 18.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, dot);
                kit::para(ui, txt(&s.text, text::BASE, fg));
            });
        }
    });
}

fn num_or_str(v: &NumOrStr) -> String {
    match v {
        NumOrStr::Num(n) => desk_core::js::number_to_string(*n),
        NumOrStr::Str(s) => s.clone(),
    }
}

/// The share proposal as one sentence; no button — the owner answers in chat.
fn share_sentence(sp: &ShareProposal) -> String {
    let mv = match (&sp.from, &sp.to) {
        (Some(f), Some(to)) => tf(
            "macro.weekShareMove",
            &[
                ("factor", &sp.factor),
                ("from", &num_or_str(f)),
                ("to", &num_or_str(to)),
            ],
        ),
        _ => tf("macro.weekShareOnly", &[("factor", &sp.factor)]),
    };
    if sp.why.is_empty() {
        mv
    } else {
        format!("{mv} {}", sp.why)
    }
}

fn trader_block(ui: &mut Ui, p: &Palette, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    kit::sub(ui, p, t("macro.traderTitle"));
    kit::bullets(ui, p, lines, text::MD);
}

/// «дд.мм – дд.мм» from two ISO dates.
fn range(a: &str, b: &str) -> String {
    let dm = |s: &str| match (s.get(8..10), s.get(5..7)) {
        (Some(d), Some(m)) => Some(format!("{d}.{m}")),
        _ => None,
    };
    match (dm(a), dm(b)) {
        (Some(x), Some(y)) => format!("{x} – {y}"),
        _ => String::new(),
    }
}

/// «Неделя в пять строк» (Friday evening) with «О тебе» inside.
pub fn week(ui: &mut Ui, p: &Palette, w: Option<&Week>, trader: &[String]) -> bool {
    let Some(w) = w.filter(|w| !w.lines.is_empty()) else {
        return false;
    };
    let span = range(&w.week_start, &w.week_end);
    let title = if span.is_empty() {
        t("macro.weekTitle").to_owned()
    } else {
        format!("{} · {span}", t("macro.weekTitle"))
    };
    kit::card(ui, p, Some(&title), CardKind::Memo, |ui| {
        kit::numbered(ui, p, &w.lines, text::BASE);
        trader_block(ui, p, trader);
        if let Some(sp) = &w.share_proposal {
            kit::para(ui, txt(share_sentence(sp), text::MD, p.fg_muted));
        }
    });
    true
}

/// «О тебе» on a weekday.
pub fn trader(ui: &mut Ui, p: &Palette, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    kit::card(ui, p, None, CardKind::Memo, |ui| trader_block(ui, p, lines));
}

/// «Идеи на проверке»: open and proposed ideas, newest first, at most five.
pub fn ideas(ui: &mut Ui, p: &Palette, ideas: Option<&Ideas>) {
    let items = visible_ideas(ideas);
    if items.is_empty() {
        return;
    }
    kit::card(ui, p, Some(t("macro.ideasTitle")), CardKind::Memo, |ui| {
        for idea in items {
            ui.horizontal_top(|ui| {
                let w = ui.available_width() - 72.0;
                ui.allocate_ui(egui::vec2(w.max(80.0), 0.0), |ui| {
                    kit::para(ui, txt(&idea.text, text::BASE, p.fg));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = space::S3;
                    let tip = tf(
                        "macro.ideaScoreTip",
                        &[("c", &idea.confirmed), ("r", &idea.refuted)],
                    );
                    ui.label(bold(format!("✗{}", idea.refuted), text::MD, p.err))
                        .on_hover_text(&tip);
                    ui.label(bold(format!("✓{}", idea.confirmed), text::MD, p.ok))
                        .on_hover_text(&tip);
                });
            });
            if idea.status == IdeaStatus::Proposed {
                kit::para(
                    ui,
                    txt(
                        tf("macro.ideaProposed", &[("times", &times(idea.confirmed))]),
                        text::SM,
                        p.accent,
                    ),
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn russian_plurals() {
        assert_eq!(times(1), "1 раз");
        assert_eq!(times(3), "3 раза");
        assert_eq!(times(5), "5 раз");
        assert_eq!(times(11), "11 раз");
        assert_eq!(times(21), "21 раз");
        assert_eq!(times(22), "22 раза");
        assert_eq!(times(112), "112 раз");
    }

    #[test]
    fn week_range() {
        assert_eq!(range("2026-09-14", "2026-09-18"), "14.09 – 18.09");
        assert_eq!(range("", "2026-09-18"), "");
    }
}
