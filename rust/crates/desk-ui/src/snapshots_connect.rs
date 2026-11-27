//! kittest tests of stage 6 (lights, «Настройки», «Диагностика», the wizard)
//! against a hand-made `ConnView`: desk-core and desk-app are building the
//! real one next door.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use chrono::{NaiveDate, TimeZone, Utc};
use desk_app::Command;
use desk_core::WindowId;
use eframe::egui::{self, Theme};
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use egui_kittest::Harness;

use crate::app::DeskApp;
use crate::chat::Page;
use crate::connect::{
    Check, CheckId, CheckState, ConnView, Fix, Light, LightState, Settings, SettingsTab,
    StatusLights, StepId, TaskId, TaskState, TasksView,
};
use crate::snapshots::{has, screen_sized, TODAY};
#[cfg(target_os = "linux")]
use crate::snapshots::{snapshot_options, theme_name};

const VAULT: &str = "C:\\Users\\Michael\\Documents\\Strategy Desk";

fn light(state: LightState, key: &str, vars: &[(&str, &str)]) -> Light {
    Light {
        state,
        key: key.into(),
        vars: vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect(),
    }
}

fn check(
    id: CheckId,
    state: CheckState,
    key: &str,
    vars: &[(&str, &str)],
    fix: Option<Fix>,
) -> Check {
    Check {
        id,
        state,
        detail_key: key.into(),
        vars: vars
            .iter()
            .map(|(k, v)| ((*k).into(), (*v).into()))
            .collect::<BTreeMap<_, _>>(),
        fix,
        fix_label_key: None,
        note_key: None,
    }
}

fn ms(h: u32, m: u32) -> i64 {
    Utc.with_ymd_and_hms(2026, 11, 27, h, m, 0)
        .unwrap()
        .timestamp_millis()
}

/// A PC halfway through setup, as desk-app publishes it: vault and Claude
/// Code there, no login, the runtime being built, TradingView closed, no
/// sessionid, no schedule.
pub(crate) fn sample_conn() -> ConnView {
    use CheckState::{Fail, Ok, Warn};
    let mut login = check(
        CheckId::ClaudeAuth,
        Fail,
        "preflight.claude.none",
        &[],
        Some(Fix::Task(TaskId::ClaudeLogin)),
    );
    login.fix_label_key = Some("preflight.fix.claudeLogin".into());
    login.note_key = Some("preflight.claude.note".into());
    let mut runtime = check(
        CheckId::Tvmcp,
        Fail,
        "preflight.runtime.missing",
        &[],
        Some(Fix::Task(TaskId::ProvisionRuntime)),
    );
    runtime.fix_label_key = Some("preflight.fix.provision".into());
    runtime.note_key = Some("preflight.runtime.note".into());
    let mut session = check(
        CheckId::TvSession,
        Fail,
        "preflight.tvSession.none",
        &[],
        None,
    );
    session.note_key = Some("preflight.tvSession.howNote".into());
    let mut sched = check(
        CheckId::Scheduler,
        Warn,
        "preflight.scheduler.none",
        &[],
        Some(Fix::Task(TaskId::SchedulerRegister)),
    );
    sched.fix_label_key = Some("preflight.fix.schedulerRegister".into());
    let settings = Settings {
        vault: Some(VAULT.into()),
        ..Settings::default()
    };
    let mut tasks = TasksView::default();
    tasks.tasks.insert(
        TaskId::ProvisionRuntime,
        TaskState {
            running: true,
            ok: None,
            command: "uv sync --frozen --project C:\\Program Files\\Strategy Desk\\resources\\tvmcp".into(),
            lines: VecDeque::from(vec![
                "Using CPython 3.12.8".to_owned(),
                "Creating virtual environment at: C:\\Users\\Michael\\AppData\\Roaming\\strategy-desk-rs\\py\\venv".into(),
                "Resolved 94 packages in 2ms".into(),
                "Prepared 31 packages in 41.2s".into(),
            ]),
            error_key: String::new(),
            error_detail: String::new(),
        },
    );
    ConnView {
        settings: Arc::new(settings),
        lights: Arc::new(StatusLights {
            tradingview: light(LightState::Warn, "status.tv.stopped", &[]),
            cdp: light(LightState::Fail, "status.cdp.down", &[("port", "9222")]),
            claude: light(
                LightState::Warn,
                "status.claude.logged_out",
                &[("version", "2.1.340")],
            ),
            vault: light(LightState::Ok, "status.vault.ok", &[]),
            checked_at: ms(7, 31),
        }),
        checks: Arc::new(vec![
            check(
                CheckId::Vault,
                Ok,
                "preflight.vault.ok",
                &[("path", VAULT)],
                None,
            ),
            check(
                CheckId::Claude,
                Ok,
                "preflight.claude.found",
                &[("version", "2.1.340")],
                None,
            ),
            login,
            runtime,
            check(
                CheckId::Tradingview,
                Fail,
                "preflight.tv.notRunning",
                &[],
                Some(Fix::LaunchTradingView),
            ),
            check(
                CheckId::Cdp,
                Ok,
                "preflight.port.free",
                &[("port", "9222")],
                None,
            ),
            session,
            sched,
            check(
                CheckId::Timezone,
                Ok,
                "preflight.timezone.ok",
                &[("tz", "Europe/Kyiv")],
                None,
            ),
        ]),
        checks_at: ms(7, 31),
        tasks: Arc::new(tasks),
        first_run: false,
        has_tv_session: false,
        today: NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
    }
}

/// A fresh PC: the vault folder is not there yet.
fn fresh_conn() -> ConnView {
    let mut c = sample_conn();
    Arc::make_mut(&mut c.lights).vault = light(LightState::Fail, "status.vault.missing", &[]);
    let mut v = check(
        CheckId::Vault,
        CheckState::Warn,
        "preflight.vault.missing",
        &[("path", VAULT)],
        Some(Fix::Task(TaskId::VaultCreate)),
    );
    v.fix_label_key = Some("preflight.fix.vaultCreate".into());
    v.note_key = Some("preflight.vault.createNote".into());
    Arc::make_mut(&mut c.checks)[0] = v;
    c.tasks = Arc::default();
    c
}

fn harness(theme: Theme, conn: ConnView, page: Page, height: f32) -> Harness<'static, DeskApp> {
    let mut h = screen_sized(theme, TODAY, "2026-10-26", Some(WindowId::Eu), height);
    h.state_mut().set_conn(conn);
    h.state_mut().chat_mut().page = page;
    h
}

fn sent(h: &Harness<'_, DeskApp>) -> Vec<Command> {
    h.state().sent.clone()
}

fn tasks_sent(h: &Harness<'_, DeskApp>) -> Vec<TaskId> {
    h.state()
        .sent
        .iter()
        .filter_map(|c| match c {
            Command::RunTask(id) => Some(*id),
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
    assert!(sent(&h).contains(&Command::LaunchTradingView));
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
    assert_eq!(tasks_sent(&h), vec![TaskId::ClaudeLogin]);
}

#[test]
fn the_wizard_presses_its_task_once_and_skipping_says_what_weakens() {
    let mut conn = sample_conn();
    conn.tasks = Arc::default();
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
    assert!(sent(&h).contains(&Command::SkipStep(StepId::Claude)));
    assert!(has(&h, "Шаг 3 из 6 · Рынок"));
    assert_eq!(tasks_sent(&h), vec![TaskId::ProvisionRuntime]);
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
    assert_eq!(tasks_sent(&h), vec![TaskId::SessionCheck]);
    assert!(sent(&h).contains(&Command::SetTvSession("abc123secret".into())));
    assert!(!has(&h, "abc123secret"));
}

#[test]
fn a_first_run_opens_the_wizard_once() {
    let mut conn = sample_conn();
    conn.first_run = true;
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
    // Every change goes to desk-app as the whole settings.
    let saved: Vec<_> = sent(&h)
        .into_iter()
        .filter_map(|c| match c {
            Command::SaveSettings(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(saved.len(), 2);
    assert!((saved[0].scale - 1.1).abs() < 1e-6);
    assert_eq!(saved[1].theme, crate::connect::Theme::Dark);
    assert_eq!(saved[1].vault.as_deref(), Some(VAULT));
    // The schedule: on = saved + registered.
    h.get_by_label("Включено").click();
    h.run();
    assert!(tasks_sent(&h).contains(&TaskId::SchedulerRegister));
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
