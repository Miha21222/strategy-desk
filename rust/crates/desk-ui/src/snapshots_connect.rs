//! kittest tests of stage 6 (lights, «Настройки», «Диагностика», the wizard)
//! against a hand-made `ConnView`: desk-core and desk-app are building the
//! real one next door.

use chrono::{TimeZone, Utc};
use desk_app::Command;
use desk_core::WindowId;
use eframe::egui::{self, Theme};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;

use crate::app::DeskApp;
use crate::chat::Page;
use crate::connect::{
    Check, CheckId, ConnView, Fix, Light, SettingsTab, State, StatusLights, TaskId, TaskState,
    TaskView,
};
use crate::snapshots::{has, screen_sized, TODAY};
#[cfg(target_os = "linux")]
use crate::snapshots::{snapshot_options, theme_name};

fn light(state: State, key: &str, vars: &[(&str, &str)]) -> Light {
    Light {
        state,
        detail_key: key.into(),
        vars: vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
    }
}

fn check(id: CheckId, state: State, key: &str, vars: &[(&str, &str)], fix: Option<Fix>) -> Check {
    Check {
        id,
        state,
        detail_key: key.into(),
        vars: vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
        fix,
        fix_label_key: None,
        note_key: None,
    }
}

/// A PC halfway through setup: vault and Claude Code there, no login, the
/// runtime being built, TradingView closed, no sessionid, no schedule.
pub(crate) fn sample_conn() -> ConnView {
    let at = Utc.with_ymd_and_hms(2026, 11, 27, 7, 31, 0).unwrap();
    let mut c = ConnView {
        lights: StatusLights {
            tradingview: light(State::Fail, "status.tv.stopped", &[]),
            cdp: light(State::Fail, "status.cdp.down", &[("port", "9222")]),
            claude: light(State::Warn, "status.claude.logged_out", &[]),
            vault: light(State::Ok, "status.vault.ok", &[]),
            checked_at: Some(at),
        },
        checks: vec![
            check(
                CheckId::Vault,
                State::Ok,
                "preflight.vault.ok",
                &[("path", "C:\\Users\\Michael\\Documents\\Strategy Desk")],
                None,
            ),
            check(
                CheckId::Claude,
                State::Ok,
                "preflight.claude.found",
                &[("version", "2.1.340")],
                None,
            ),
            check(
                CheckId::ClaudeAuth,
                State::Fail,
                "preflight.claude.none",
                &[],
                Some(Fix::Task(TaskId::ClaudeLogin)),
            ),
            check(
                CheckId::Uv,
                State::Ok,
                "preflight.uv.ok",
                &[("version", "0.9.2")],
                None,
            ),
            check(
                CheckId::Tvmcp,
                State::Fail,
                "preflight.runtime.missing",
                &[],
                Some(Fix::Task(TaskId::ProvisionRuntime)),
            ),
            check(
                CheckId::Tradingview,
                State::Fail,
                "preflight.tv.notRunning",
                &[],
                Some(Fix::LaunchTradingView),
            ),
            check(
                CheckId::Cdp,
                State::Ok,
                "preflight.port.free",
                &[("port", "9222")],
                None,
            ),
            check(
                CheckId::TvSession,
                State::Fail,
                "preflight.tvSession.none",
                &[],
                None,
            ),
            check(
                CheckId::Scheduler,
                State::Warn,
                "preflight.scheduler.none",
                &[],
                Some(Fix::Task(TaskId::SchedulerRegister)),
            ),
            check(
                CheckId::Timezone,
                State::Ok,
                "preflight.timezone.ok",
                &[],
                None,
            ),
        ],
        checked_at: Some(at),
        ..ConnView::default()
    };
    c.settings.scale = 1.0;
    c.settings.vault = Some("C:\\Users\\Michael\\Documents\\Strategy Desk".into());
    c.tasks = vec![TaskView {
        id: TaskId::ProvisionRuntime,
        state: TaskState::Running,
        lines: vec![
            "uv sync --frozen --project C:\\Program Files\\Strategy Desk\\resources\\tvmcp".into(),
            "Using CPython 3.12.8".into(),
            "Creating virtual environment at: C:\\Users\\Michael\\AppData\\Roaming\\strategy-desk-rs\\py\\venv".into(),
            "Resolved 94 packages in 2ms".into(),
            "Prepared 31 packages in 41.2s".into(),
        ],
        wants_input: false,
    }];
    c
}

/// A fresh PC: the vault folder is not there yet.
fn fresh_conn() -> ConnView {
    let mut c = sample_conn();
    let path = "C:\\Users\\Michael\\Documents\\Strategy Desk";
    c.lights.vault = light(State::Fail, "status.vault.missing", &[]);
    c.checks[0] = check(
        CheckId::Vault,
        State::Warn,
        "preflight.vault.missing",
        &[("path", path)],
        Some(Fix::Task(TaskId::VaultCreate)),
    );
    c.tasks.clear();
    c
}

fn harness(theme: Theme, conn: ConnView, page: Page, height: f32) -> Harness<'static, DeskApp> {
    let mut h = screen_sized(theme, TODAY, "2026-10-26", Some(WindowId::Eu), height);
    h.state_mut().set_conn(conn);
    h.state_mut().chat_mut().page = page;
    h
}

fn tasks_sent(h: &Harness<'_, DeskApp>) -> Vec<String> {
    h.state()
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::RunTask(id) => Some(id.0.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn lights_say_what_is_wrong_and_a_red_one_opens_diagnostics() {
    let mut h = harness(Theme::Light, sample_conn(), Page::Day, 1000.0);
    h.run();
    for s in [
        "TradingView: закрыт",
        "График для агента: порт 9222 молчит",
        "Claude: нет входа",
        "Папка проекта: на месте",
        "Запустить TradingView",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    assert!(!has(&h, "Появится в следующих версиях"));
    h.get_by_label("Запустить TradingView").click();
    h.run();
    assert!(h.state().sent.contains(&Command::LaunchTradingView));
    h.get_by_label("Claude: нет входа").click();
    h.run();
    assert_eq!(h.state_mut().chat_mut().page, Page::Settings);
    assert_eq!(h.state_mut().conn_ui_mut().tab, SettingsTab::Diagnostics);
    assert!(has(&h, "Требует внимания: 5"));
}

#[test]
fn every_problem_has_one_fix_and_it_runs_its_task() {
    let mut h = harness(Theme::Light, sample_conn(), Page::Settings, 1400.0);
    h.state_mut().conn_ui_mut().tab = SettingsTab::Diagnostics;
    h.run();
    // Fail/warn with a fix: login, runtime, TradingView, schedule.
    assert_eq!(h.query_all_by_label("Исправить").count(), 4);
    for s in [
        "Входа нет · кнопка откроет вход в браузере",
        "Среда Python",
        "Prepared 31 packages in 41.2s",
        "выполняю…",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    // The runtime is being built: its «Исправить» waits.
    let fixes: Vec<_> = h.query_all_by_label("Исправить").collect();
    let disabled = fixes
        .iter()
        .filter(|n| n.accesskit_node().is_disabled())
        .count();
    drop(fixes);
    assert_eq!(disabled, 1);
    h.query_all_by_label("Исправить").next().unwrap().click();
    h.run();
    assert_eq!(tasks_sent(&h), vec!["claude-login".to_owned()]);
}

#[test]
fn the_wizard_presses_its_task_once_and_skipping_says_what_weakens() {
    let mut conn = sample_conn();
    conn.tasks.clear();
    let mut h = harness(Theme::Light, conn, Page::Wizard, 1200.0);
    // Page 2 «Claude»: the login waits for the button.
    h.state_mut().conn_ui_mut().wizard.step = 1;
    h.run();
    assert!(tasks_sent(&h).is_empty());
    assert!(has(&h, "Шаг 2 из 6 · Claude"));
    // Page 3 «Рынок»: the runtime is pressed by itself, once.
    h.get_by_label("Пропустить шаг").click();
    h.run();
    h.run();
    assert!(has(&h, "Шаг 3 из 6 · Рынок"));
    assert_eq!(tasks_sent(&h), vec!["provision-runtime".to_owned()]);
    assert!(has(&h, "Запущено само: Среда Python"));
    h.run();
    assert_eq!(tasks_sent(&h).len(), 1, "never a retry loop");
    // Back on page 2 the skip says what is missing now.
    h.get_by_label("Назад").click();
    h.run();
    assert!(has(
        &h,
        "Шаг «Claude» пропущен · агент не ответит, пока нет Claude Code и входа"
    ));
}

#[test]
fn the_sessionid_is_sent_once_and_never_kept() {
    let mut h = harness(Theme::Light, sample_conn(), Page::Wizard, 1200.0);
    h.state_mut().conn_ui_mut().wizard.step = 4;
    h.run();
    assert!(has(&h, "Шаг 5 из 6 · Сессия TradingView"));
    assert!(has(&h, "Не сохранён"));
    let field = h.get_by_role(egui::accesskit::Role::PasswordInput);
    field.focus();
    field.type_text("abc123secret");
    h.run();
    h.get_by_label("Сохранить").click();
    h.run();
    assert!(h.state_mut().conn_ui_mut().session_field.is_empty());
    assert_eq!(tasks_sent(&h), vec!["session-check".to_owned()]);
    assert!(!has(&h, "abc123secret"));
}

#[test]
fn a_first_run_opens_the_wizard_once() {
    let mut conn = sample_conn();
    conn.settings.first_run = true;
    let mut h = harness(Theme::Light, conn, Page::Day, 1000.0);
    h.run();
    assert_eq!(h.state_mut().chat_mut().page, Page::Wizard);
    assert!(has(&h, "Шаг 1 из 6 · Папка проекта"));
    h.get_by_label("Закрыть").click();
    h.run();
    assert_eq!(h.state_mut().chat_mut().page, Page::Day);
}

#[test]
fn settings_change_theme_and_scale_here_and_now() {
    let mut h = harness(Theme::Light, sample_conn(), Page::Settings, 1400.0);
    h.run();
    for s in [
        "Папка проекта",
        "Сессия TradingView",
        "Сервер TradingView для обзора",
        "Расписание",
        "Очистить рантайм",
        "Мастер настройки",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    h.get_by_label("110 %").click();
    h.run();
    assert!((h.ctx.zoom_factor() - 1.1).abs() < 1e-6);
    h.get_by_label("Тёмная").click();
    h.run();
    assert_eq!(h.ctx.theme(), Theme::Dark);
}

/// Wizard pages 1 and 5, settings, diagnostics with failing checks (the
/// sidebar lights are in every picture).
#[cfg(target_os = "linux")]
#[test]
fn connect_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        for (name, page, step, tab) in [
            ("wizard_vault", Page::Wizard, 0, SettingsTab::Settings),
            ("wizard_session", Page::Wizard, 4, SettingsTab::Settings),
            ("settings", Page::Settings, 0, SettingsTab::Settings),
            ("diag", Page::Settings, 0, SettingsTab::Diagnostics),
        ] {
            let conn = if name == "wizard_vault" {
                fresh_conn()
            } else {
                sample_conn()
            };
            let mut h = harness(theme, conn, page, 1000.0);
            h.state_mut().conn_ui_mut().wizard.step = step;
            h.state_mut().conn_ui_mut().tab = tab;
            h.run();
            let name = format!("conn_{}_{name}", theme_name(theme));
            results.add(h.try_snapshot_options(name, &snapshot_options()));
        }
    }
}
