//! «Настройки» (TZ item 23): vault, look, the TradingView sessionid, the
//! review's TradingView server, the schedule, «Очистить рантайм», the
//! wizard — and «Диагностика» as the second tab.

use desk_app::Command;
use desk_core::windows::{window_times, WindowId};
use eframe::egui::{self, Ui};

use super::diag::{self, detail};
use super::{
    kyiv_dm_hm, CheckId, CheckState, ConnUi, ConnView, MacroMcp, PickField, SettingsTab, TaskId,
    Theme,
};
use crate::i18n::t;
use crate::screen::kit::{self, bold, txt, CardKind, Tone};
use crate::theme::{space, text, Palette};

/// The zoom steps of «Масштаб».
pub const SCALES: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];

/// What the page asks of the app besides desk-app commands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsEvent {
    OpenWizard,
}

pub fn page(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) -> Option<SettingsEvent> {
    let mut event = None;
    ui.label(bold(t("tabs.settings"), text::TITLE, p.fg));
    let tabs = [
        (t("conn.tab.settings"), Tone::Accent),
        (t("conn.tab.diag"), Tone::Accent),
    ];
    let sel = match st.tab {
        SettingsTab::Settings => 0,
        SettingsTab::Diagnostics => 1,
    };
    if let Some(i) = kit::seg(ui, p, &tabs, Some(sel), false) {
        st.tab = if i == 0 {
            SettingsTab::Settings
        } else {
            SettingsTab::Diagnostics
        };
        if st.tab == SettingsTab::Diagnostics && conn.checks_at == 0 {
            out.push(Command::RunPreflight);
        }
    }
    match st.tab {
        SettingsTab::Diagnostics => diag::diagnostics(ui, p, st, conn, out),
        SettingsTab::Settings => {
            vault_card(ui, p, st, conn, out);
            look_card(ui, p, conn, out);
            session_card(ui, p, st, conn, out);
            mcp_card(ui, p, conn, out);
            schedule_card(ui, p, st, conn, out);
            if runtime_card(ui, p, st, conn, out) {
                event = Some(SettingsEvent::OpenWizard);
            }
        }
    }
    event
}

/// The vault path: typed, or picked with the native dialog.
pub fn vault_field(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    let current = conn.settings.vault.clone().unwrap_or_default();
    let mut field = st.vault_field.take().unwrap_or_else(|| current.clone());
    let mut pick = false;
    let picking = st.picker.is_some();
    ui.horizontal(|ui| {
        let w = ui.available_width() - if super::HAS_PICKER { 260.0 } else { 120.0 };
        ui.add(
            egui::TextEdit::singleline(&mut field)
                .hint_text(t("conn.set.vaultPlaceholder"))
                .desired_width(w.max(160.0)),
        );
        let changed = !field.trim().is_empty() && field.trim() != current;
        if kit::button(ui, p, t("conn.set.apply"), changed, changed).clicked() {
            let v = field.trim().to_owned();
            out.push(Command::SaveSettings(conn.edited(|s| s.vault = Some(v))));
        }
        if super::HAS_PICKER && kit::button(ui, p, t("conn.set.pick"), false, !picking).clicked() {
            pick = true;
        }
    });
    st.vault_field = Some(field);
    if pick {
        st.pick_folder(PickField::Vault);
    }
    st.poll_picker(conn, out);
}

fn vault_card(ui: &mut Ui, p: &Palette, st: &mut ConnUi, conn: &ConnView, out: &mut Vec<Command>) {
    kit::card(ui, p, Some(t("conn.set.vault")), CardKind::Plain, |ui| {
        kit::para(ui, txt(t("conn.set.vaultHint"), text::MD, p.fg_muted));
        vault_field(ui, p, st, conn, out);
        if let Some(c) = conn.check(CheckId::Vault) {
            diag::check_row(ui, p, st, conn, c, out);
        }
    });
}

fn look_card(ui: &mut Ui, p: &Palette, conn: &ConnView, out: &mut Vec<Command>) {
    kit::card(ui, p, Some(t("conn.set.look")), CardKind::Plain, |ui| {
        kit::grid2_cells(ui, |ui, col| {
            if col == 0 {
                ui.label(bold(t("conn.set.theme"), text::MD, p.fg));
                let items = [
                    (t("desk.theme.system"), Tone::Accent),
                    (t("desk.theme.light"), Tone::Accent),
                    (t("desk.theme.dark"), Tone::Accent),
                ];
                let themes = [Theme::System, Theme::Light, Theme::Dark];
                let sel = themes.iter().position(|x| *x == conn.settings.theme);
                if let Some(i) = kit::seg(ui, p, &items, sel, false) {
                    let th = themes[i];
                    out.push(Command::SaveSettings(conn.edited(|s| s.theme = th)));
                }
            } else {
                ui.label(bold(t("conn.set.scale"), text::MD, p.fg));
                let labels: Vec<String> = SCALES
                    .iter()
                    .map(|s| format!("{} %", (s * 100.0).round()))
                    .collect();
                let items: Vec<(&str, Tone)> =
                    labels.iter().map(|l| (l.as_str(), Tone::Accent)).collect();
                let sel = SCALES
                    .iter()
                    .position(|s| (s - conn.settings.scale).abs() < 0.01);
                if let Some(i) = kit::seg(ui, p, &items, sel, false) {
                    let z = SCALES[i];
                    out.push(Command::SaveSettings(conn.edited(|s| s.scale = z)));
                }
            }
        });
    });
}

/// The sessionid: a field that is emptied once saved, «Проверить», and only
/// whether one is stored comes back.
pub fn session_field(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    let s = &conn.settings;
    let (status, color) = match (conn.has_tv_session, s.tv_session_verified_at) {
        (true, Some(at)) => (
            crate::i18n::tf("conn.set.sessionOk", &[("time", &kyiv_dm_hm(at))]),
            p.ok,
        ),
        (true, None) => (t("conn.set.sessionUnverified").to_owned(), p.warn),
        (false, _) => (t("conn.set.sessionNone").to_owned(), p.err),
    };
    ui.label(bold(status, text::MD, color));
    ui.horizontal(|ui| {
        let w = ui.available_width() - 250.0;
        ui.add(
            egui::TextEdit::singleline(&mut st.session_field)
                .password(true)
                .hint_text(t("conn.set.sessionPlaceholder"))
                .desired_width(w.max(160.0)),
        );
        let can_save = !st.session_field.trim().is_empty();
        if kit::button(ui, p, t("conn.set.save"), can_save, can_save).clicked() {
            out.push(Command::SetTvSession(
                std::mem::take(&mut st.session_field).trim().to_owned(),
            ));
            out.push(Command::RunTask(TaskId::SessionCheck));
        }
        let checking = conn.running(TaskId::SessionCheck);
        if kit::button(
            ui,
            p,
            t("conn.set.check"),
            false,
            conn.has_tv_session && !checking,
        )
        .clicked()
        {
            out.push(Command::RunTask(TaskId::SessionCheck));
        }
        if conn.has_tv_session && kit::link(ui, p, t("conn.set.forget")).clicked() {
            out.push(Command::ClearTvSession);
        }
    });
    kit::para(ui, txt(t("conn.set.sessionStored"), text::SM, p.fg_muted));
    if let Some(tv) = conn.task(TaskId::SessionCheck) {
        diag::task_box(ui, p, st, TaskId::SessionCheck, tv, out);
    }
}

fn session_card(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    kit::card(ui, p, Some(t("conn.set.session")), CardKind::Plain, |ui| {
        kit::para(ui, txt(t("conn.set.sessionHint"), text::MD, p.fg_muted));
        session_field(ui, p, st, conn, out);
    });
}

fn mcp_card(ui: &mut Ui, p: &Palette, conn: &ConnView, out: &mut Vec<Command>) {
    kit::card(ui, p, Some(t("conn.set.mcp")), CardKind::Plain, |ui| {
        let items = [
            (t("conn.set.mcpBundled"), Tone::Accent),
            (t("conn.set.mcpUser"), Tone::Accent),
        ];
        let sel = match conn.settings.macro_mcp {
            MacroMcp::Bundled => 0,
            MacroMcp::User => 1,
        };
        if let Some(i) = kit::seg(ui, p, &items, Some(sel), false) {
            let v = if i == 0 {
                MacroMcp::Bundled
            } else {
                MacroMcp::User
            };
            if v != conn.settings.macro_mcp {
                out.push(Command::SaveSettings(conn.edited(|s| s.macro_mcp = v)));
            }
        }
        kit::para(ui, txt(t("conn.set.mcpHint"), text::MD, p.fg_muted));
    });
}

/// The three runs (their times come from the exchanges' clocks, so they
/// are shown, not edited) and on/off. A change re-registers the jobs when
/// the schedule is on; off removes them.
pub fn schedule_editor(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    let cur = conn.settings.schedule;
    let busy = conn.running(TaskId::SchedulerRegister) || conn.running(TaskId::SchedulerUnregister);
    egui::Grid::new("schedule-runs")
        .spacing(egui::vec2(space::S8, space::S3))
        .show(ui, |ui| {
            for (id, key) in [
                (WindowId::Eu, "conn.set.eu"),
                (WindowId::Us, "conn.set.us"),
                (WindowId::Review, "conn.set.review"),
            ] {
                let on = match id {
                    WindowId::Eu => cur.eu,
                    WindowId::Us => cur.us,
                    WindowId::Review => cur.review,
                };
                let mut v = on;
                let at = window_times(id, conn.today).at.format("%H:%M").to_string();
                let label = format!("{} · {at}", t(key));
                if ui
                    .add_enabled(
                        !busy,
                        egui::Checkbox::new(&mut v, txt(label, text::BASE, p.fg)),
                    )
                    .changed()
                {
                    out.push(Command::SaveSettings(conn.edited(|s| match id {
                        WindowId::Eu => s.schedule.eu = v,
                        WindowId::Us => s.schedule.us = v,
                        WindowId::Review => s.schedule.review = v,
                    })));
                    if cur.enabled {
                        out.push(Command::RunTask(TaskId::SchedulerRegister));
                    }
                }
                ui.end_row();
            }
        });
    ui.label(txt(t("conn.set.scheduleTimes"), text::SM, p.fg_muted));
    let items = [
        (t("conn.set.scheduleOff"), Tone::Bad),
        (t("conn.set.scheduleOn"), Tone::Good),
    ];
    if let Some(i) = kit::seg(ui, p, &items, Some(usize::from(cur.enabled)), false) {
        let on = i == 1;
        if on != cur.enabled && !busy {
            out.push(Command::SaveSettings(
                conn.edited(|s| s.schedule.enabled = on),
            ));
            out.push(Command::RunTask(if on {
                TaskId::SchedulerRegister
            } else {
                TaskId::SchedulerUnregister
            }));
        }
    }
    for id in [TaskId::SchedulerRegister, TaskId::SchedulerUnregister] {
        if let Some(tv) = conn.task(id) {
            diag::task_box(ui, p, st, id, tv, out);
        }
    }
}

fn schedule_card(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) {
    kit::card(ui, p, Some(t("conn.set.schedule")), CardKind::Plain, |ui| {
        kit::para(ui, txt(t("conn.set.scheduleHint"), text::MD, p.fg_muted));
        schedule_editor(ui, p, st, conn, out);
        if let Some(c) = conn.check(CheckId::Timezone) {
            if c.state != CheckState::Ok {
                kit::para(ui, txt(detail(c), text::SM, p.warn));
            }
        }
    });
}

/// «Очистить рантайм» (two taps) and the wizard. Returns true for the wizard.
fn runtime_card(
    ui: &mut Ui,
    p: &Palette,
    st: &mut ConnUi,
    conn: &ConnView,
    out: &mut Vec<Command>,
) -> bool {
    let mut wizard = false;
    kit::card(ui, p, Some(t("conn.set.more")), CardKind::Plain, |ui| {
        ui.horizontal(|ui| {
            if kit::button(ui, p, t("conn.set.wizard"), false, true).clicked() {
                wizard = true;
            }
            ui.label(txt(t("conn.set.wizardHint"), text::SM, p.fg_muted));
        });
        let now = ui.input(|i| i.time);
        if st.confirm_clear.is_some_and(|t0| now - t0 > 6.0) {
            st.confirm_clear = None;
        }
        let confirming = st.confirm_clear.is_some();
        ui.horizontal(|ui| {
            let label = if confirming {
                t("conn.set.clearConfirm")
            } else {
                t("conn.set.clearRuntime")
            };
            let busy = conn.tasks.tasks.values().any(|t| t.running);
            if kit::button(ui, p, label, confirming, !busy).clicked() {
                if confirming {
                    st.confirm_clear = None;
                    out.push(Command::ClearRuntime);
                } else {
                    st.confirm_clear = Some(now);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_secs(6));
                }
            }
            ui.add(egui::Label::new(txt(t("conn.set.clearNote"), text::SM, p.fg_muted)).wrap());
        });
    });
    wizard
}
