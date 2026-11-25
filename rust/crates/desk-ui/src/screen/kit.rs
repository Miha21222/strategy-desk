//! Small building blocks of the macro screen, after `cards.css` and
//! `macro.css`: cards, captions, chips, segmented buttons, bullet lists.

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, Response, RichText, Sense, Shadow, Stroke, Ui,
};

use crate::fonts;
use crate::theme::{radius, space, text, Palette};

/// Text at a size and colour.
pub fn txt(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(s).size(size).color(color)
}

/// Semibold text (Inter SemiBold).
pub fn bold(s: impl Into<String>, size: f32, color: Color32) -> RichText {
    txt(s, size, color).family(fonts::bold())
}

/// A wrapped label.
pub fn para(ui: &mut Ui, r: RichText) -> Response {
    ui.add(egui::Label::new(r).wrap())
}

/// `.macro .sub`: a small muted caption above a block.
pub fn sub(ui: &mut Ui, p: &Palette, s: &str) {
    ui.label(bold(s.to_uppercase(), text::SM, p.fg_muted));
}

/// The look of a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardKind {
    #[default]
    Plain,
    /// A question to the owner (`.card.human.ask`): accent border.
    Ask,
    /// `.card.memo`: the memory cards, a quieter head.
    Memo,
}

/// `.card` with an optional `.card-head`; `extra` paints on the right of the head.
pub fn card<R>(
    ui: &mut Ui,
    p: &Palette,
    title: Option<&str>,
    kind: CardKind,
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    card_ex(ui, p, title, kind, |_| {}, body)
}

pub fn card_ex<R>(
    ui: &mut Ui,
    p: &Palette,
    title: Option<&str>,
    kind: CardKind,
    extra: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let r = radius::MD;
    let border = match kind {
        CardKind::Ask => p.accent,
        _ => p.border,
    };
    Frame::new()
        .fill(p.bg_raised)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(r))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 60 } else { 16 }),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            if let Some(title) = title {
                let head_fill = match kind {
                    CardKind::Ask => p.accent_soft,
                    _ => p.bg_elev,
                };
                let head = Frame::new()
                    .fill(head_fill)
                    .corner_radius(CornerRadius {
                        nw: r - 1,
                        ne: r - 1,
                        sw: 0,
                        se: 0,
                    })
                    .inner_margin(Margin::symmetric(space::S6 as i8, space::S3 as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(bold(title, text::BASE, p.fg));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), extra);
                        });
                    })
                    .response;
                ui.painter().hline(
                    head.rect.x_range(),
                    head.rect.bottom(),
                    Stroke::new(1.0, p.border),
                );
            }
            Frame::new()
                .inner_margin(Margin::symmetric(space::S6 as i8, space::S5 as i8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = space::S5;
                    body(ui)
                })
                .inner
        })
        .inner
}

/// A tinted box (`.mlight`, `.demo-banner`, `.todo .cancel`).
pub fn tinted<R>(ui: &mut Ui, fill: Color32, body: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S6 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui)
        })
        .inner
}

/// How a chip is tinted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Good,
    Bad,
    Warn,
    Neutral,
    Accent,
}

pub fn tone_colors(p: &Palette, tone: Tone) -> (Color32, Color32) {
    match tone {
        Tone::Good => (p.ok_soft, p.ok),
        Tone::Bad => (p.err_soft, p.err),
        Tone::Warn => (p.warn_soft, p.warn),
        Tone::Neutral => (p.bg_elev, p.fg_muted),
        Tone::Accent => (p.accent_solid, p.accent_fg),
    }
}

/// `.chip` / `.fhit` / `.wt-badge`: a pill with text.
pub fn chip(ui: &mut Ui, p: &Palette, s: &str, tone: Tone, size: f32) -> Response {
    let (fill, fg) = tone_colors(p, tone);
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(space::S4 as i8, 1))
        .show(ui, |ui| ui.label(bold(s, size, fg)))
        .response
}

/// `.seg`: a row of mutually exclusive buttons. Returns the index clicked.
pub fn seg(
    ui: &mut Ui,
    p: &Palette,
    items: &[(&str, Tone)],
    selected: Option<usize>,
    big: bool,
) -> Option<usize> {
    let mut clicked = None;
    let size = if big { text::LG } else { text::MD };
    let pad = if big {
        egui::vec2(space::S8, space::S4)
    } else {
        egui::vec2(space::S6, space::S2)
    };
    Frame::new()
        .stroke(Stroke::new(1.0, p.border_control))
        .corner_radius(CornerRadius::same(radius::SM))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.spacing_mut().button_padding = pad;
            ui.horizontal(|ui| {
                for (i, (label, tone)) in items.iter().enumerate() {
                    let on = selected == Some(i);
                    let (fill, fg) = if on {
                        match tone {
                            Tone::Good => (p.ok_soft, p.ok),
                            Tone::Bad => (p.err_soft, p.err),
                            _ => (p.accent_soft, p.accent),
                        }
                    } else {
                        (Color32::TRANSPARENT, p.fg)
                    };
                    let label = if on {
                        bold(*label, size, fg)
                    } else {
                        txt(*label, size, fg)
                    };
                    let b = egui::Button::new(label)
                        .fill(fill)
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::ZERO)
                        .selected(on);
                    if ui.add(b).clicked() {
                        clicked = Some(i);
                    }
                }
            });
        });
    clicked
}

/// A plain button in the desk style; `primary` is the filled accent one.
pub fn button(ui: &mut Ui, p: &Palette, s: &str, primary: bool, enabled: bool) -> Response {
    let b = if primary && enabled {
        egui::Button::new(bold(s, text::BASE, p.accent_fg)).fill(p.accent_solid)
    } else {
        egui::Button::new(txt(s, text::BASE, if enabled { p.fg } else { p.fg_muted }))
    };
    ui.add_enabled(enabled, b)
}

/// A text-only button (`button.link` / `.ghost`).
pub fn link(ui: &mut Ui, p: &Palette, s: &str) -> Response {
    ui.add(egui::Button::new(txt(s, text::MD, p.accent)).frame(false))
}

/// `<ul>`: one wrapped line per item with a dot.
pub fn bullets(ui: &mut Ui, p: &Palette, items: &[String], size: f32) {
    for x in items {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = space::S3;
            ui.label(txt("•", size, p.fg_muted));
            ui.add(egui::Label::new(txt(x, size, p.fg)).wrap());
        });
    }
}

/// `<ol>`.
pub fn numbered(ui: &mut Ui, p: &Palette, items: &[String], size: f32) {
    for (i, x) in items.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = space::S3;
            ui.label(bold(format!("{}.", i + 1), size, p.fg_muted));
            ui.add(egui::Label::new(txt(x, size, p.fg)).wrap());
        });
    }
}

/// A coloured left rule beside a block (`.opinion`, `.talk`).
pub fn ruled<R>(ui: &mut Ui, color: Color32, body: impl FnOnce(&mut Ui) -> R) -> R {
    let inner = Frame::new()
        .inner_margin(Margin {
            left: space::S5 as i8,
            right: 0,
            top: 2,
            bottom: 2,
        })
        .show(ui, body);
    let r = inner.response.rect;
    ui.painter()
        .vline(r.left() + 1.5, r.y_range(), Stroke::new(3.0, color));
    inner.inner
}

/// `ui.columns` with plain top-down columns: egui's own columns are
/// justified, and justified layouts stretch wrapped text to both edges.
pub fn columns(ui: &mut Ui, n: usize, mut cell: impl FnMut(usize, &mut Ui)) {
    ui.columns(n, |cols| {
        for (i, col) in cols.iter_mut().enumerate() {
            col.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| cell(i, ui));
        }
    });
}

/// Two cards side by side when there is room (`.macro-grid`: columns of at
/// least 360 px), else one under the other.
pub fn grid2(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let w = ui.available_width();
    if w >= 2.0 * 360.0 + space::S6 {
        let (mut left, mut right) = (Some(left), Some(right));
        columns(ui, 2, |i, ui| {
            ui.spacing_mut().item_spacing.y = space::S6;
            if i == 0 {
                if let Some(f) = left.take() {
                    f(ui);
                }
            } else if let Some(f) = right.take() {
                f(ui);
            }
        });
    } else {
        left(ui);
        right(ui);
    }
}

/// [`grid2`] with one closure that gets the cell index (0 = left, 1 = right),
/// for cells that share mutable state.
pub fn grid2_cells(ui: &mut Ui, mut cell: impl FnMut(&mut Ui, usize)) {
    let w = ui.available_width();
    if w >= 2.0 * 360.0 + space::S6 {
        columns(ui, 2, |i, ui| {
            ui.spacing_mut().item_spacing.y = space::S6;
            cell(ui, i);
        });
    } else {
        cell(ui, 0);
        cell(ui, 1);
    }
}

/// A rounded bar from 0 to `frac` of the width (`.stat-track`).
pub fn meter(ui: &mut Ui, p: &Palette, frac: f32, fill: Color32, mid_line: bool) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), Sense::hover());
    let painter = ui.painter_at(rect.expand(3.0));
    painter.rect_filled(rect, CornerRadius::same(radius::XS), p.track);
    let mut bar = rect;
    bar.set_width(rect.width() * frac.clamp(0.0, 1.0));
    painter.rect_filled(bar, CornerRadius::same(radius::XS), fill);
    if mid_line {
        let x = rect.center().x;
        painter.vline(
            x,
            rect.top() - 2.0..=rect.bottom() + 2.0,
            Stroke::new(1.0, p.fg_muted),
        );
    }
    resp
}
