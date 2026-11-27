//! Stage 6: the lights in the sidebar, «Настройки» with «Диагностика» and
//! the first-run wizard. They draw desk-app's snapshot ([`ConnView`]) and
//! push desk-app [`Command`]s; theme and scale also apply in the window at
//! once (the app looks at every `SaveSettings` going out).

pub mod diag;
pub mod lights;
pub mod model;
pub mod settings;
pub mod wizard;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc;

use desk_app::Command;

pub use self::model::{
    check_key, conn_of, Check, CheckId, CheckState, ConnView, Fix, Light, LightState, MacroMcp,
    PickField, ScheduleSettings, Settings, StatusLights, StepId, TaskId, TaskState, TasksView,
    Theme,
};

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
    /// Index into the visible pages (`desk_core::wizard::visible_steps`).
    pub step: usize,
    /// Started by a page by itself in this wizard session (never again).
    pub auto_done: BTreeSet<TaskId>,
    /// What a page pressed by itself, for «Запущено само: …».
    pub auto_shown: Vec<(StepId, TaskId)>,
    /// Skipped just now: shown as skipped until the snapshot's settings say so.
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
    /// «Очистить рантайм» pressed once (time).
    pub confirm_clear: Option<f64>,
    /// The browser code for `claude-login`.
    pub task_input: String,
    /// A copy-fix was copied: (check, time), for «Скопировано».
    pub copied: Option<(CheckId, f64)>,
    /// A native folder picker is open on its own thread, for this field.
    pub picker: Option<(PickField, mpsc::Receiver<Option<PathBuf>>)>,
    /// The wizard opened by itself once already this session.
    pub wizard_shown: bool,
}

impl ConnUi {
    /// Open the native folder picker off the UI thread (Windows); elsewhere
    /// the path is typed into the field.
    pub fn pick_folder(&mut self, field: PickField) {
        self.picker = pick_folder().map(|rx| (field, rx));
    }

    /// A folder came back from the picker: saved into its setting.
    pub fn poll_picker(&mut self, conn: &ConnView, out: &mut Vec<Command>) {
        let Some((field, rx)) = &self.picker else {
            return;
        };
        match rx.try_recv() {
            Ok(Some(p)) => {
                let path = p.display().to_string();
                let field = *field;
                if field == PickField::Vault {
                    self.vault_field = Some(path.clone());
                }
                out.push(Command::SaveSettings(conn.edited(|s| match field {
                    PickField::Vault => s.vault = Some(path),
                    PickField::TvmcpPath => s.tvmcp_path = path,
                })));
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

/// Kyiv wall clock «09:31» of a Unix-ms moment.
pub fn kyiv_hm_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(kyiv_hm)
        .unwrap_or_default()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kyiv_clock_of_unix_ms() {
        // 2026-11-27 07:31 UTC = 09:31 in Kyiv (EET).
        assert_eq!(kyiv_hm_ms(1_795_764_660_000), "09:31");
        assert_eq!(kyiv_hm_ms(i64::MAX), "");
    }
}
