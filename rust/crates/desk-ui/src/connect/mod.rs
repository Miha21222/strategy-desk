//! Stage 6: the lights in the sidebar, «Настройки» with «Диагностика», the
//! first-run wizard, and the one place their actions become desk-app
//! commands.

pub mod diag;
pub mod lights;
pub mod model;
pub mod settings;
pub mod wizard;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;

use desk_app::Command;

pub use self::model::{
    conn_of, Check, CheckId, ConnView, Fix, Light, MacroMcp, Schedule, SettingsView, State,
    StatusLights, StepId, TaskId, TaskState, TaskView, Theme,
};

/// What the connection screens ask desk-app for.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnAction {
    /// Probe the lights now (the «Обновить» button, a red light).
    CheckStatus,
    /// Run every check of «Диагностика» again.
    RunPreflight,
    LaunchTradingView,
    RunTask(TaskId),
    /// A line for a task that reads stdin (`claude_login`: the browser code).
    TaskInput {
        id: TaskId,
        text: String,
    },
    SetVault(PathBuf),
    SetTheme(Theme),
    SetScale(f32),
    SetMacroMcp(MacroMcp),
    SetSchedule(Schedule),
    /// The sessionid goes to the keyring through desk-app; the UI keeps no copy.
    SetTvSession(String),
    SetCdpPort(u16),
    ClearRuntime,
    /// A wizard page skipped (`true`) or taken back (`false`).
    SkipStep(StepId, bool),
    /// The wizard is done: do not open it by itself again.
    FinishWizard,
}

/// The bridge to today's desk-app: what has a command goes out, the rest is
/// dropped with a trace line until the stage-6 backend lands. The sessionid
/// itself is never traced.
pub fn to_commands(a: ConnAction) -> Vec<Command> {
    let c = match a {
        ConnAction::CheckStatus => Command::CheckStatus,
        ConnAction::LaunchTradingView => Command::LaunchTradingView,
        ConnAction::RunTask(id) => Command::RunTask(desk_app::app::TaskId(id.as_str().to_owned())),
        ConnAction::SetVault(p) => Command::SetVault(p),
        ConnAction::SetTvSession(_) => {
            crate::perf::trace::event("conn_action_unwired", ", \"action\": \"SetTvSession\"");
            return Vec::new();
        }
        other => {
            let what = format!("{other:?}");
            crate::perf::trace::event("conn_action_unwired", &format!(", \"action\": {what:?}"));
            return Vec::new();
        }
    };
    vec![c]
}

/// The two tabs of the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Settings,
    Diagnostics,
}

/// The wizard's own memory: the page, and the tasks it already pressed.
#[derive(Debug, Default)]
pub struct WizardUi {
    pub step: usize,
    pub auto_done: HashSet<TaskId>,
    /// What a page pressed by itself, for «Запущено само: …».
    pub auto_shown: Vec<(StepId, TaskId)>,
    /// Skipped in this session (desk-app keeps `setup_skipped` for good).
    pub skipped: Vec<StepId>,
}

/// Everything the connection screens remember between frames.
#[derive(Debug, Default)]
pub struct ConnUi {
    pub tab: SettingsTab,
    pub wizard: WizardUi,
    /// The sessionid field: cleared the moment it is saved.
    pub session_field: String,
    /// The vault path field (typed or picked).
    pub vault_field: Option<String>,
    /// Schedule times as typed («09:30»), until saved.
    pub times: Option<[String; 3]>,
    /// «Очистить рантайм» pressed once (time).
    pub confirm_clear: Option<f64>,
    /// The browser code for `claude_login`.
    pub task_input: String,
    /// A copy-fix was copied: (check, time), for «Скопировано».
    pub copied: Option<(CheckId, f64)>,
    /// A native folder picker is open on its own thread.
    pub picker: Option<mpsc::Receiver<Option<PathBuf>>>,
    /// The wizard opened by itself once already this session.
    pub wizard_shown: bool,
}

impl ConnUi {
    /// Open the native folder picker off the UI thread (Windows); elsewhere
    /// the path is typed into the field.
    pub fn pick_folder(&mut self) {
        self.picker = pick_folder();
    }

    /// A folder came back from the picker.
    pub fn poll_picker(&mut self, out: &mut Vec<ConnAction>) {
        let Some(rx) = &self.picker else { return };
        match rx.try_recv() {
            Ok(Some(p)) => {
                self.vault_field = Some(p.display().to_string());
                out.push(ConnAction::SetVault(p));
                self.picker = None;
            }
            Ok(None) | Err(mpsc::TryRecvError::Disconnected) => self.picker = None,
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
}

#[cfg(windows)]
fn pick_folder() -> Option<mpsc::Receiver<Option<PathBuf>>> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("desk-picker".into())
        .spawn(move || {
            let _ = tx.send(rfd::FileDialog::new().pick_folder());
        });
    spawned.ok().map(|_| rx)
}

#[cfg(not(windows))]
fn pick_folder() -> Option<mpsc::Receiver<Option<PathBuf>>> {
    None
}

/// Whether this build has a native folder picker.
pub const HAS_PICKER: bool = cfg!(windows);

/// Kyiv wall clock «09:31» of a moment.
pub fn kyiv_hm(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&desk_core::windows::KYIV)
        .format("%H:%M")
        .to_string()
}

/// Kyiv «27.11 09:31» of a moment.
pub fn kyiv_dm_hm(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&desk_core::windows::KYIV)
        .format("%d.%m %H:%M")
        .to_string()
}

/// «09:30» → time.
pub fn parse_time(s: &str) -> Option<chrono::NaiveTime> {
    chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M").ok()
}

/// time → «09:30».
pub fn fmt_time(t: chrono::NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_wired_actions_become_commands() {
        assert_eq!(
            to_commands(ConnAction::CheckStatus),
            vec![Command::CheckStatus]
        );
        assert_eq!(
            to_commands(ConnAction::RunTask(TaskId::ProvisionRuntime)),
            vec![Command::RunTask(desk_app::app::TaskId(
                "provision_runtime".into()
            ))]
        );
        assert!(to_commands(ConnAction::SetTvSession("secret".into())).is_empty());
    }

    #[test]
    fn times_read_and_write_as_hh_mm() {
        let t = parse_time(" 9:05 ").unwrap();
        assert_eq!(fmt_time(t), "09:05");
        assert!(parse_time("25:00").is_none());
        assert!(parse_time("утром").is_none());
    }
}
