//! The first-run wizard: six pages (vault → claude → tvmcp → tradingview →
//! session → schedule) with progress, the page's checks with «Исправить»,
//! the output of the tasks, a skip that says what weakens, and the old
//! `AUTO_TASKS` rule: a page presses its own task once when it opens.

use std::collections::BTreeMap;

use desk_app::Command;
use desk_core::wizard::{self as core_wizard, TaskRun};
use eframe::egui::{self, CornerRadius, Ui};

use super::diag::{self, state_mark};
use super::settings::{schedule_editor, session_field, vault_field};
use super::{CheckId, CheckState, ConnUi, ConnView, StepId, TaskId};
use crate::i18n::{t, tf};
use crate::screen::kit::{self, bold, txt, CardKind};
use crate::theme::{radius, space, text, Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardEvent {
    Close,
}

/// The pages this diagnosis has something to say about (all six before it).
pub fn steps(conn: &ConnView) -> Vec<StepId> {
    core_wizard::visible_steps(&conn.checks)
}

/// Done = nothing on the page failed or is unchecked (a warning is enough).
pub fn step_ok(conn: &ConnView, step: StepId) -> bool {
    conn.checks_at > 0 && core_wizard::step_done(step, &conn.checks)
}

/// Skipped in the settings, or just now (until the snapshot says so).
pub fn skipped(st: &ConnUi, conn: &ConnView, step: StepId) -> bool {
    core_wizard::is_skipped(&conn.settings, step) || st.wizard.skipped.contains(&step)
}

/// The one task a page starts by itself now (`desk_core::wizard::auto_task`):
/// provision-runtime or session-check, once per wizard session.
pub fn auto_press(st: &ConnUi, conn: &ConnView, step: StepId) -> Option<TaskId> {
    if conn.checks_at == 0 {
        return None;
    }
    let runs: BTreeMap<TaskId, TaskRun> = conn
        .tasks
        .tasks
        .iter()
        .map(|(id, t)| {
            (
                *id,
                TaskRun {
                    running: t.running,
                    ok: t.ok,
                },
            )
        })
        .collect();
    core_wizard::auto_task(step, &conn.checks, &runs, &st.wizard.auto_done, None)
}

pub fn page(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) -> Option<WizardEvent> {
    let mut event = None;
    let pages = steps(conn);
    let n = pages.len();
    st.wizard.step = st.wizard.step.min(n.saturating_sub(1));
    let step = pages.get(st.wizard.step).copied().unwrap_or(StepId::Vault);
    // A page that turned green is no longer «just skipped».
    if step_ok(conn, step) {
        st.wizard.skipped.retain(|s| *s != step);
    }

    if let Some(task) = auto_press(st, conn, step) {
        st.wizard.auto_done.insert(task);
        st.wizard.auto_shown.push((step, task));
        out.push(Command::RunTask(task));
    }

    ui.horizontal(|ui| {
        ui.label(bold(t("wiz.title"), text::TITLE, p.fg));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::link(ui, p, t("wiz.close")).clicked() {
                event = Some(WizardEvent::Close);
            }
        });
    });
    progress(ui, p, st, conn, &pages);
    let name = t(&format!("wiz.name.{}", step.as_str())).to_owned();
    ui.label(txt(
        tf(
            "wiz.step",
            &[("n", &(st.wizard.step + 1)), ("total", &n), ("name", &name)],
        ),
        text::MD,
        p.fg_muted,
    ));

    kit::card(ui, p, Some(&name), CardKind::Plain, |ui| {
        kit::para(
            ui,
            txt(t(&format!("wiz.intro.{}", step.as_str())), text::BASE, p.fg),
        );
        match step {
            StepId::Vault => vault_field(ui, p, st, conn, out),
            StepId::Session => {
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = space::S2;
                    kit::numbered(
                        ui,
                        p,
                        &[1, 2, 3, 4].map(|i| t(&format!("wiz.session.how{i}")).to_owned()),
                        text::MD,
                    );
                });
                session_field(ui, p, st, conn, out);
            }
            StepId::Schedule => schedule_editor(ui, p, st, conn, out),
            _ => {}
        }
        let auto: Vec<String> = st
            .wizard
            .auto_shown
            .iter()
            .filter(|(s, _)| *s == step)
            .map(|(_, task)| diag::task_title(*task))
            .collect();
        if !auto.is_empty() {
            let list = auto.join(", ");
            kit::para(
                ui,
                txt(tf("wiz.auto", &[("list", &list)]), text::SM, p.accent),
            );
        }
        ui.separator();
        let own = core_wizard::checks_of_step(step, &conn.checks);
        if own.is_empty() {
            for id in step.checks() {
                unknown(ui, p, *id);
            }
        }
        for c in own {
            diag::check_row(ui, p, st, conn, c, out);
        }
    });

    let ok = step_ok(conn, step);
    let is_skipped = skipped(st, conn, step);
    if is_skipped {
        kit::tinted(ui, p.warn_soft, |ui| {
            let limited = t(&format!("setup.limited.{}", step.as_str())).to_owned();
            ui.add(egui::Label::new(txt(limited, text::MD, p.warn)).wrap());
        });
    } else {
        let (line, color) = if ok {
            (t("wiz.stepOk"), p.ok)
        } else {
            (t("wiz.stepTodo"), p.fg_muted)
        };
        ui.label(txt(line, text::MD, color));
    }

    ui.horizontal(|ui| {
        if st.wizard.step > 0 && kit::button(ui, p, t("wiz.back"), false, true).clicked() {
            st.wizard.step -= 1;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let last = st.wizard.step + 1 == n;
            let label = if last { t("wiz.finish") } else { t("wiz.next") };
            // A page that is not done moves on only through «Пропустить шаг»,
            // so every skip is remembered and said out loud later.
            if kit::button(ui, p, label, ok || is_skipped, ok || is_skipped).clicked() {
                if last {
                    out.push(Command::FinishWizard);
                    event = Some(WizardEvent::Close);
                } else {
                    st.wizard.step += 1;
                }
            }
            if !ok && !is_skipped && kit::button(ui, p, t("wiz.skip"), false, true).clicked() {
                st.wizard.skipped.push(step);
                out.push(Command::SkipStep(step));
                if !last {
                    st.wizard.step += 1;
                }
            }
        });
    });
    if !ok && !is_skipped {
        let weak = t(&format!("wiz.weak.{}", step.as_str())).to_owned();
        ui.label(txt(
            tf("wiz.skipNote", &[("weak", &weak)]),
            text::SM,
            p.fg_muted,
        ));
    }
    event
}

fn unknown(ui: &mut Ui, p: &Palette, id: CheckId) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = space::S5;
        let (g, c) = state_mark(p, CheckState::Skipped);
        ui.add_sized([diag::MARK_W, 22.0], egui::Label::new(bold(g, text::LG, c)));
        ui.label(bold(diag::check_title(id), text::BASE, p.fg_muted));
        ui.label(txt(t("conn.diag.checking"), text::SM, p.fg_muted));
    });
}

/// Six bars: green = done, amber = skipped, accent = here, grey = ahead.
/// A bar is a button: the pages can be visited in any order.
fn progress(ui: &mut Ui, p: &Palette, st: &mut ConnUi, conn: &ConnView, pages: &[StepId]) {
    let n = pages.len().max(1);
    let gap = space::S3;
    let w = ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).max(20.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, step) in pages.iter().enumerate() {
            let color = if i == st.wizard.step {
                p.accent
            } else if step_ok(conn, *step) {
                p.ok
            } else if skipped(st, conn, *step) {
                p.warn
            } else {
                p.track
            };
            let (rect, r) = ui.allocate_exact_size(egui::vec2(w, 6.0), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, CornerRadius::same(radius::XS), color);
            let name = t(&format!("wiz.name.{}", step.as_str())).to_owned();
            r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            if r.on_hover_text(name).clicked() {
                st.wizard.step = i;
            }
        }
    });
}
