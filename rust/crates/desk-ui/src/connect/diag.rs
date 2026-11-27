//! «Диагностика»: the checks, each problem with ONE «Исправить» that runs its
//! [`Fix`], and the output of the tasks those fixes start. The wizard draws
//! the same rows for its page's checks.

use eframe::egui::{self, Color32, CornerRadius, FontId, Frame, Margin, ScrollArea, Stroke, Ui};

use super::{
    Check, CheckId, ConnAction, ConnUi, ConnView, Fix, State, TaskId, TaskState, TaskView,
};
use crate::i18n::{t, tf};
use crate::screen::kit::{self, bold, txt};
use crate::theme::{radius, space, text, Palette};

/// A key + vars through `tf`.
pub fn say(key: &str, vars: &[(String, String)]) -> String {
    let vars: Vec<(&str, &dyn std::fmt::Display)> = vars
        .iter()
        .map(|(k, v)| (k.as_str(), v as &dyn std::fmt::Display))
        .collect();
    tf(key, &vars)
}

pub fn check_title(id: CheckId) -> String {
    t(&format!("conn.check.{}", id.as_str())).to_owned()
}

pub fn task_title(id: TaskId) -> String {
    t(&format!("conn.taskName.{}", id.as_str())).to_owned()
}

/// The glyph and colour of a state.
pub fn state_mark(p: &Palette, s: State) -> (&'static str, Color32) {
    match s {
        State::Ok => ("✓", p.ok),
        State::Warn => ("⚠", p.warn),
        State::Fail => ("✗", p.err),
        State::Unknown => ("–", p.fg_muted),
    }
}

/// The whole «Диагностика» tab.
pub fn diagnostics(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<ConnAction>,
) {
    kit::para(ui, txt(t("conn.diag.intro"), text::BASE, p.fg_muted));
    ui.horizontal(|ui| {
        let label = if conn.checking {
            t("conn.diag.checking")
        } else {
            t("conn.diag.recheck")
        };
        if kit::button(ui, p, label, false, !conn.checking).clicked() {
            out.push(ConnAction::RunPreflight);
        }
        let summary = match (conn.checked_at, conn.problems()) {
            (None, _) => t("conn.diag.never").to_owned(),
            (Some(_), 0) => t("conn.diag.allOk").to_owned(),
            (Some(_), n) => tf("conn.diag.problems", &[("n", &n)]),
        };
        let color = if conn.problems() > 0 {
            p.warn
        } else {
            p.fg_muted
        };
        ui.label(bold(summary, text::MD, color));
        if let Some(at) = conn.checked_at {
            let time = super::kyiv_hm(at);
            ui.label(txt(
                tf("conn.diag.checkedAt", &[("time", &time)]),
                text::SM,
                p.fg_muted,
            ));
        }
    });
    kit::card(ui, p, None, kit::CardKind::Plain, |ui| {
        ui.spacing_mut().item_spacing.y = space::S6;
        if conn.checks.is_empty() {
            for id in CheckId::ALL {
                unknown_row(ui, p, id);
            }
        }
        for c in &conn.checks {
            check_row(ui, p, st, conn, c, out);
        }
    });
}

/// The column of the ✓ ⚠ ✗ marks, so the names line up.
pub const MARK_W: f32 = 16.0;

fn unknown_row(ui: &mut Ui, p: &Palette, id: CheckId) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = space::S5;
        let (g, c) = state_mark(p, State::Unknown);
        ui.add_sized([MARK_W, 22.0], egui::Label::new(bold(g, text::LG, c)));
        ui.label(bold(check_title(id), text::BASE, p.fg_muted));
    });
}

/// One check: mark, name, what was found, and «Исправить» when there is
/// something to fix. A fix that starts a task shows the task under it.
pub fn check_row(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    c: &Check,
    out: &mut Vec<ConnAction>,
) {
    let (glyph, color) = state_mark(p, c.state);
    let problem = matches!(c.state, State::Fail | State::Warn);
    let task = match &c.fix {
        Some(Fix::Task(id)) => conn.task(*id),
        _ => None,
    };
    let busy = task.is_some_and(|t| t.state == TaskState::Running);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = space::S5;
        ui.add_sized(
            [MARK_W, 22.0],
            egui::Label::new(bold(glyph, text::LG, color)),
        );
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = space::S2;
            ui.horizontal(|ui| {
                ui.label(bold(check_title(c.id), text::BASE, p.fg));
                if let (true, Some(fix)) = (problem, &c.fix) {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut r = kit::button(ui, p, t("conn.fix"), true, !busy);
                        if let Some(k) = &c.fix_label_key {
                            r = r.on_hover_text(t(k));
                        }
                        if r.clicked() {
                            run_fix(ui, st, c.id, fix, out);
                        }
                    });
                }
            });
            kit::para(ui, txt(say(&c.detail_key, &c.vars), text::MD, p.fg_muted));
            if let (true, Some(note)) = (problem, &c.note_key) {
                kit::para(ui, txt(t(note), text::SM, p.fg_muted));
            }
            if let (
                true,
                Some(Fix::Copy {
                    text: cmd,
                    explain_key,
                }),
            ) = (problem, &c.fix)
            {
                if c.note_key.is_none() {
                    kit::para(ui, txt(t(explain_key), text::SM, p.fg_muted));
                }
                code_line(ui, p, cmd);
                let now = ui.input(|i| i.time);
                if st
                    .copied
                    .as_ref()
                    .is_some_and(|(id, t0)| *id == c.id && now - t0 < 3.0)
                {
                    ui.label(txt(t("conn.fix.copied"), text::SM, p.ok));
                }
            }
            if let Some(tv) = task {
                task_box(ui, p, st, tv, out);
            }
        });
    });
}

fn run_fix(ui: &Ui, st: &mut ConnUi, id: CheckId, fix: &Fix, out: &mut Vec<ConnAction>) {
    match fix {
        Fix::Task(task) => out.push(ConnAction::RunTask(*task)),
        Fix::Copy { text, .. } => {
            ui.ctx().copy_text(text.clone());
            st.copied = Some((id, ui.input(|i| i.time)));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(3));
        }
        Fix::LaunchTradingView => out.push(ConnAction::LaunchTradingView),
        Fix::PickFolder => {
            st.tab = super::SettingsTab::Settings;
            st.pick_folder();
        }
        Fix::SetCdpPort(port) => out.push(ConnAction::SetCdpPort(*port)),
    }
}

fn code_line(ui: &mut Ui, p: &Palette, s: &str) {
    Frame::new()
        .fill(p.code_bg)
        .corner_radius(CornerRadius::same(radius::XS))
        .inner_margin(Margin::symmetric(space::S4 as i8, space::S2 as i8))
        .show(ui, |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(s)
                        .font(FontId::monospace(text::SM))
                        .color(p.fg),
                )
                .wrap(),
            );
        });
}

/// A task's name, state and output; a field for the browser code when the
/// task waits for it.
pub fn task_box(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    tv: &TaskView,
    out: &mut Vec<ConnAction>,
) {
    Frame::new()
        .fill(p.bg_elev)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S5 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = space::S3;
            ui.horizontal(|ui| {
                ui.label(bold(task_title(tv.id), text::MD, p.fg));
                let (s, c) = match &tv.state {
                    TaskState::Running => (t("conn.task.running").to_owned(), p.accent),
                    TaskState::Done => (t("conn.task.done").to_owned(), p.ok),
                    TaskState::Failed { key, vars } => (say(key, vars), p.err),
                };
                ui.add(egui::Label::new(txt(s, text::SM, c)).wrap());
            });
            if !tv.lines.is_empty() {
                ScrollArea::vertical()
                    .id_salt(("task-log", tv.id.as_str()))
                    .max_height(140.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        // The last lines are what matters; a long log stays cheap.
                        let from = tv.lines.len().saturating_sub(200);
                        let mut j = egui::text::LayoutJob::default();
                        for l in &tv.lines[from..] {
                            j.append(
                                l,
                                0.0,
                                egui::TextFormat::simple(
                                    FontId::monospace(text::SM - 1.0),
                                    p.fg_muted,
                                ),
                            );
                            j.append("\n", 0.0, egui::TextFormat::default());
                        }
                        ui.add(egui::Label::new(j).wrap());
                    });
            }
            if tv.wants_input && tv.state == TaskState::Running {
                ui.horizontal(|ui| {
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut st.task_input)
                            .hint_text(t("conn.task.code"))
                            .desired_width(260.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let can = !st.task_input.trim().is_empty();
                    if (kit::button(ui, p, t("conn.task.send"), can, can).clicked() || enter) && can
                    {
                        out.push(ConnAction::TaskInput {
                            id: tv.id,
                            text: std::mem::take(&mut st.task_input).trim().to_owned(),
                        });
                    }
                });
            }
        });
}

/// Colour helper for the lights.
pub fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}
