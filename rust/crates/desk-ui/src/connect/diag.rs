//! «Диагностика»: the checks, each problem with ONE «Исправить» that runs its
//! [`Fix`], and the output of the tasks those fixes start. The wizard draws
//! the same rows for its page's checks.

use std::collections::BTreeMap;

use desk_app::Command;
use eframe::egui::{self, Color32, CornerRadius, FontId, Frame, Margin, ScrollArea, Stroke, Ui};

use super::{
    check_key, kyiv_hm_ms, Check, CheckId, CheckState, ConnUi, ConnView, Fix, TaskId, TaskState,
};
use crate::i18n::{t, tf};
use crate::screen::kit::{self, bold, txt};
use crate::theme::{radius, space, text, Palette};

/// The column of the ✓ ⚠ ✗ marks, so the names line up.
pub const MARK_W: f32 = 16.0;

/// A key + vars through `tf`.
pub fn say<'a>(key: &str, vars: impl IntoIterator<Item = (&'a String, &'a String)>) -> String {
    let vars: Vec<(&str, &dyn std::fmt::Display)> = vars
        .into_iter()
        .map(|(k, v)| (k.as_str(), v as &dyn std::fmt::Display))
        .collect();
    tf(key, &vars)
}

/// A check's detail line.
pub fn detail(c: &Check) -> String {
    say(&c.detail_key, &c.vars)
}

/// A pairs list as `say` takes it.
pub fn pairs(v: &[(String, String)]) -> impl Iterator<Item = (&String, &String)> {
    v.iter().map(|(k, x)| (k, x))
}

pub fn check_title(id: CheckId) -> String {
    t(&format!("conn.check.{}", check_key(id))).to_owned()
}

pub fn task_title(id: TaskId) -> String {
    t(&format!("conn.taskName.{}", id.as_str())).to_owned()
}

/// The glyph and colour of a state.
pub fn state_mark(p: &Palette, s: CheckState) -> (&'static str, Color32) {
    match s {
        CheckState::Ok => ("✓", p.ok),
        CheckState::Warn => ("⚠", p.warn),
        CheckState::Fail => ("✗", p.err),
        CheckState::Skipped => ("–", p.fg_muted),
    }
}

/// The whole «Диагностика» tab.
pub fn diagnostics(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    kit::para(ui, txt(t("conn.diag.intro"), text::BASE, p.fg_muted));
    ui.horizontal(|ui| {
        if kit::button(ui, p, t("conn.diag.recheck"), false, true).clicked() {
            out.push(Command::RunPreflight);
        }
        let (summary, color) = match (conn.checks_at, conn.problems()) {
            (0, _) => (t("conn.diag.checking").to_owned(), p.fg_muted),
            (_, 0) => (t("conn.diag.allOk").to_owned(), p.ok),
            (_, n) => (tf("conn.diag.problems", &[("n", &n)]), p.warn),
        };
        ui.label(bold(summary, text::MD, color));
        if conn.checks_at > 0 {
            let time = kyiv_hm_ms(conn.checks_at);
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
        for c in conn.checks.iter() {
            check_row(ui, p, st, conn, c, out);
        }
    });
}

/// A check not gathered yet: its name and «проверяю…».
pub fn unknown_row(ui: &mut Ui, p: &Palette, id: CheckId) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = space::S5;
        let (g, c) = state_mark(p, CheckState::Skipped);
        ui.add_sized([MARK_W, 22.0], egui::Label::new(bold(g, text::LG, c)));
        ui.label(bold(check_title(id), text::BASE, p.fg_muted));
        ui.label(txt(t("conn.diag.checking"), text::SM, p.fg_muted));
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
    out: &mut Vec<Command>,
) {
    let (glyph, color) = state_mark(p, c.state);
    let problem = matches!(c.state, CheckState::Fail | CheckState::Warn);
    let task = match &c.fix {
        Some(Fix::Task(id)) => conn.task(*id).map(|t| (*id, t)),
        _ => None,
    };
    let busy = task.is_some_and(|(_, t)| t.running);
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
                            run_fix(ui, st, conn, c.id, fix, out);
                        }
                    });
                }
            });
            kit::para(ui, txt(detail(c), text::MD, p.fg_muted));
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
            if let Some((id, tv)) = task {
                task_box(ui, p, st, id, tv, out);
            }
        });
    });
}

fn run_fix(
    ui: &Ui,
    st: &mut ConnUi,
    conn: &ConnView,
    id: CheckId,
    fix: &Fix,
    out: &mut Vec<Command>,
) {
    match fix {
        Fix::Task(task) => out.push(Command::RunTask(*task)),
        Fix::Copy { text, .. } => {
            ui.ctx().copy_text(text.clone());
            st.copied = Some((id, ui.input(|i| i.time)));
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(3));
        }
        Fix::LaunchTradingView => out.push(Command::LaunchTradingView),
        Fix::PickFolder => {
            if let Some(field) = desk_core::wizard::pick_field(id) {
                st.pick_folder(field);
            }
        }
        Fix::SetCdpPort(port) => {
            let port = *port;
            out.push(Command::SaveSettings(conn.edited(|s| s.cdp_port = port)));
            out.push(Command::RunPreflight);
        }
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

/// A task's name, state, the command it runs and its output; a field for
/// the browser code while `claude-login` waits for it; «Остановить».
pub fn task_box(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    id: TaskId,
    tv: &TaskState,
    out: &mut Vec<Command>,
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
                ui.label(bold(task_title(id), text::MD, p.fg));
                let (s, c) = task_status(p, tv);
                ui.add(egui::Label::new(txt(s, text::SM, c)).wrap());
                if tv.running {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if kit::link(ui, p, t("conn.task.stop")).clicked() {
                            out.push(Command::StopTask(id));
                        }
                    });
                }
            });
            if !tv.command.is_empty() {
                let cmd = format!("> {}", tv.command);
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(cmd)
                            .font(FontId::monospace(text::SM - 1.0))
                            .color(p.fg),
                    )
                    .wrap(),
                );
            }
            if !tv.lines.is_empty() {
                ScrollArea::vertical()
                    .id_salt(("task-log", id.as_str()))
                    .max_height(140.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut j = egui::text::LayoutJob::default();
                        let fmt =
                            egui::TextFormat::simple(FontId::monospace(text::SM - 1.0), p.fg_muted);
                        for l in &tv.lines {
                            j.append(l, 0.0, fmt.clone());
                            j.append("\n", 0.0, fmt.clone());
                        }
                        ui.add(egui::Label::new(j).wrap());
                    });
            }
            if id.wants_stdin() && tv.running {
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
                        out.push(Command::TaskInput {
                            id,
                            text: std::mem::take(&mut st.task_input).trim().to_owned(),
                        });
                    }
                });
            }
        });
}

/// «выполняю…», «готово», or the error with its detail.
pub fn task_status(p: &Palette, tv: &TaskState) -> (String, Color32) {
    if tv.running {
        return (t("conn.task.running").to_owned(), p.accent);
    }
    match tv.ok {
        Some(true) => (t("conn.task.done").to_owned(), p.ok),
        Some(false) => {
            let key = if tv.error_key.is_empty() {
                "task.error.failed"
            } else {
                tv.error_key.as_str()
            };
            let vars = BTreeMap::from([("detail".to_owned(), tv.error_detail.clone())]);
            (say(key, &vars), p.err)
        }
        None => (String::new(), p.fg_muted),
    }
}

/// A coloured dot (the lights).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}
