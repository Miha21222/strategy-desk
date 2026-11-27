//! The first-run wizard: six pages (vault → claude → tvmcp → tradingview →
//! session → schedule) with progress, the page's checks with «Исправить»,
//! the output of the tasks, a skip that says what weakens, and the old
//! `AUTO_TASKS` rule: a page presses its own task once when it opens.

use eframe::egui::{self, CornerRadius, Ui};

use super::diag::{self, state_mark};
use super::settings::{schedule_editor, session_field, vault_field};
use super::{CheckId, ConnAction, ConnUi, ConnView, Fix, State, StepId, TaskId};
use crate::i18n::{t, tf};
use crate::screen::kit::{self, bold, txt, CardKind};
use crate::theme::{radius, space, text, Palette};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardEvent {
    Close,
}

/// A page is done when every one of its checks is green.
pub fn step_ok(conn: &ConnView, step: StepId) -> bool {
    step.checks()
        .iter()
        .all(|id| conn.check(*id).is_some_and(|c| c.state == State::Ok))
}

pub fn skipped(st: &ConnUi, conn: &ConnView, step: StepId) -> bool {
    st.wizard.skipped.contains(&step) || conn.settings.setup_skipped.contains(&step)
}

/// The tasks a page presses by itself now: listed in its `auto_tasks`, asked
/// for by one of its failing checks, not pressed before in this wizard
/// session and not already running.
pub fn auto_presses(st: &ConnUi, conn: &ConnView, step: StepId) -> Vec<TaskId> {
    step.auto_tasks()
        .iter()
        .copied()
        .filter(|task| !st.wizard.auto_done.contains(task) && !conn.running(*task))
        .filter(|task| {
            step.checks().iter().any(|id| {
                conn.check(*id).is_some_and(|c| {
                    matches!(c.state, State::Fail | State::Warn) && c.fix == Some(Fix::Task(*task))
                })
            })
        })
        .collect()
}

pub fn page(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<ConnAction>,
) -> Option<WizardEvent> {
    let mut event = None;
    let n = StepId::ALL.len();
    st.wizard.step = st.wizard.step.min(n - 1);
    let step = StepId::ALL[st.wizard.step];

    for task in auto_presses(st, conn, step) {
        st.wizard.auto_done.insert(task);
        st.wizard.auto_shown.push((step, task));
        out.push(ConnAction::RunTask(task));
    }

    ui.horizontal(|ui| {
        ui.label(bold(t("wiz.title"), text::TITLE, p.fg));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::link(ui, p, t("wiz.close")).clicked() {
                event = Some(WizardEvent::Close);
            }
        });
    });
    progress(ui, p, st, conn);
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
        for id in step.checks() {
            match conn.check(*id) {
                Some(c) => diag::check_row(ui, p, st, conn, c, out),
                None => unknown(ui, p, *id),
            }
        }
    });

    let ok = step_ok(conn, step);
    let is_skipped = skipped(st, conn, step);
    if is_skipped {
        kit::tinted(ui, p.warn_soft, |ui| {
            ui.horizontal(|ui| {
                let weak = t(&format!("wiz.weak.{}", step.as_str())).to_owned();
                ui.add(
                    egui::Label::new(txt(tf("wiz.skipped", &[("weak", &weak)]), text::MD, p.warn))
                        .wrap(),
                );
            });
            if kit::link(ui, p, t("wiz.unskip")).clicked() {
                st.wizard.skipped.retain(|s| *s != step);
                out.push(ConnAction::SkipStep(step, false));
            }
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
                    out.push(ConnAction::FinishWizard);
                    event = Some(WizardEvent::Close);
                } else {
                    st.wizard.step += 1;
                }
            }
            if !ok && !is_skipped && kit::button(ui, p, t("wiz.skip"), false, true).clicked() {
                st.wizard.skipped.push(step);
                out.push(ConnAction::SkipStep(step, true));
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
        let (g, c) = state_mark(p, State::Unknown);
        ui.add_sized([diag::MARK_W, 22.0], egui::Label::new(bold(g, text::LG, c)));
        ui.label(bold(diag::check_title(id), text::BASE, p.fg_muted));
        ui.label(txt(t("conn.diag.never"), text::SM, p.fg_muted));
    });
}

/// Six bars: green = done, amber = skipped, accent = here, grey = ahead.
/// A bar is a button: the pages can be visited in any order.
fn progress(ui: &mut Ui, p: &Palette, st: &mut ConnUi, conn: &ConnView) {
    let n = StepId::ALL.len();
    let gap = space::S3;
    let w = ((ui.available_width() - gap * (n as f32 - 1.0)) / n as f32).max(20.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for (i, step) in StepId::ALL.iter().enumerate() {
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
