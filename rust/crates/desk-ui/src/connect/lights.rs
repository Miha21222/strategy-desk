//! «Подключения» in the sidebar: four lights with a short line each and
//! «Запустить TradingView». A light that is not green opens «Диагностика».

use eframe::egui::{self, CornerRadius, Frame, Margin, Sense, Ui};

use super::diag::{dot, say};
use super::{ConnAction, ConnView, Light, State};
use crate::i18n::t;
use crate::screen::kit::{self, txt};
use crate::theme::{radius, space, text, Palette};

/// What a light click asks of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightsEvent {
    OpenDiagnostics,
}

fn light_color(p: &Palette, s: State) -> egui::Color32 {
    match s {
        State::Ok => p.ok,
        State::Warn => p.warn,
        State::Fail => p.err,
        State::Unknown => p.border_strong,
    }
}

/// The short line under a light's name.
fn line(l: &Light) -> String {
    if l.detail_key.is_empty() {
        t("status.unknown").to_owned()
    } else {
        say(&l.detail_key, &l.vars)
    }
}

pub fn show(
    ui: &mut Ui,
    p: &Palette,
    conn: &ConnView,
    out: &mut Vec<ConnAction>,
) -> Option<LightsEvent> {
    let mut event = None;
    ui.horizontal(|ui| {
        kit::sub(ui, p, t("sidebar.simple.title"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let r = ui
                .add(egui::Button::new(txt(t("sidebar.refresh"), text::SM, p.accent)).frame(false));
            if r.clicked() {
                out.push(ConnAction::CheckStatus);
            }
        });
    });
    let l = &conn.lights;
    for (key, light) in [
        ("conn.light.tradingview", &l.tradingview),
        ("conn.light.cdp", &l.cdp),
        ("conn.light.claude", &l.claude),
        ("conn.light.vault", &l.vault),
    ] {
        let bad = matches!(light.state, State::Fail | State::Warn);
        let inner = Frame::new()
            .corner_radius(CornerRadius::same(radius::SM))
            .inner_margin(Margin::symmetric(space::S3 as i8, space::S1 as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = space::S4;
                    ui.add_space(1.0);
                    ui.vertical(|ui| {
                        ui.add_space(4.0);
                        dot(ui, light_color(p, light.state));
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(txt(t(key), text::MD, p.fg));
                        let color = if bad {
                            light_color(p, light.state)
                        } else {
                            p.fg_muted
                        };
                        ui.add(egui::Label::new(txt(line(light), text::SM, color)).wrap());
                    });
                });
            });
        let rect = inner.response.rect;
        let r = ui.interact(rect, ui.scope_id().with(key), Sense::click());
        let label = format!("{}: {}", t(key), line(light));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if bad {
            let r = r
                .on_hover_text(t("conn.openDiag"))
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if r.hovered() {
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(radius::SM),
                    egui::Stroke::new(1.0, p.border),
                    egui::StrokeKind::Inside,
                );
            }
            if r.clicked() {
                event = Some(LightsEvent::OpenDiagnostics);
            }
        }
    }
    // «Запустить TradingView» while it is closed or its port is silent.
    let tv_off = matches!(l.tradingview.state, State::Fail | State::Warn)
        || matches!(l.cdp.state, State::Fail);
    if tv_off && kit::button(ui, p, t("sidebar.launchTv"), false, true).clicked() {
        out.push(ConnAction::LaunchTradingView);
    }
    event
}
