//! Stage 6 inside the app's loop: settings, the sessionid, the tasks, the
//! runtime, and when the lights are refreshed. Owned by the actor in
//! `app.rs`, which publishes what changes; nothing here touches the snapshot.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Days, Utc};

use crate::app::UserError;
use crate::chat::launch::{resolve_claude, ClaudeLaunch};
use crate::claude::process::CLAUDE_BIN_ENV;
use crate::macro_env::Platform;
use crate::paths;
use crate::runtime::{self, RuntimePaths};
use crate::secrets::{self, SecretStore};
use crate::settings_store::SettingsStore;
use crate::tasks::{self, After, PlanCtx, TaskEvent, TaskId, TasksView};
use desk_core::preflight::Check;
use desk_core::schedule::planned_runs;
use desk_core::settings::Settings;
use desk_core::wizard::{self, StepId};

pub mod keys {
    pub const SETTINGS_READ: &str = "error.settings.read";
    pub const SETTINGS_WRITE: &str = "error.settings.write";
    pub const SECRET_BAD: &str = "error.session.bad_shape";
    pub const SECRET_STORE: &str = "error.session.store";
    pub const RUNTIME_BUSY: &str = "error.runtime.busy";
    pub const RUNTIME_CLEAR: &str = "error.runtime.clear";
}

pub struct Ops {
    pub data_dir: PathBuf,
    store: SettingsStore,
    pub settings: Settings,
    secrets: Arc<dyn SecretStore>,
    pub runtime: RuntimePaths,
    pub tasks: TasksView,
    running: HashMap<TaskId, (tasks::Running, After)>,
    /// The tasks view changed since it was last published.
    pub tasks_dirty: bool,
    exe: Option<PathBuf>,
    launch: ClaudeLaunch,
}

impl Ops {
    /// Load (or import, first start) the settings. The error, if any, is for the list.
    pub fn new(
        data_dir: PathBuf,
        import_from: Option<PathBuf>,
        default_vault: Option<PathBuf>,
        secrets: Arc<dyn SecretStore>,
        exe: Option<PathBuf>,
        launch: ClaudeLaunch,
    ) -> (Self, Option<UserError>) {
        let store = SettingsStore::new(&data_dir);
        let (settings, _origin, err) = store.load(import_from.as_deref(), default_vault.as_deref());
        let runtime = runtime::resolve(
            &paths::runtime_root(&data_dir),
            paths::exe_dir().as_deref(),
            &settings.tvmcp_path,
        );
        let ops = Self {
            data_dir,
            store,
            settings,
            secrets,
            runtime,
            tasks: TasksView::default(),
            running: HashMap::new(),
            tasks_dirty: false,
            exe,
            launch,
        };
        (
            ops,
            err.map(|e| UserError::new(keys::SETTINGS_READ).var("detail", e.to_string())),
        )
    }

    pub fn has_tv_session(&self) -> bool {
        secrets::has_tv_session(self.secrets.as_ref())
    }

    /// The vault the settings name (empty = none).
    pub fn settings_vault(&self) -> Option<PathBuf> {
        self.settings.vault.as_deref().map(PathBuf::from)
    }

    /// Claude Code: the test seam, the setting, `DESK_CLAUDE_BIN`, PATH.
    pub fn claude_bin(&self) -> Option<PathBuf> {
        let configured = self.launch.program.clone().or_else(|| {
            let c = self.settings.claude_bin.trim();
            (!c.is_empty()).then(|| PathBuf::from(c))
        });
        // A named binary that is not there is «missing», not a reason to
        // fall back to some other claude on PATH.
        if configured.as_ref().is_some_and(|p| !p.is_file()) {
            return None;
        }
        resolve_claude(
            configured.as_deref(),
            std::env::var_os(CLAUDE_BIN_ENV),
            std::env::var_os("PATH"),
        )
    }

    pub fn save_settings(&mut self, next: Settings) -> Result<(), UserError> {
        self.store
            .save(&next)
            .map_err(|e| UserError::new(keys::SETTINGS_WRITE).var("detail", e.to_string()))?;
        if next.tvmcp_path != self.settings.tvmcp_path {
            self.runtime = runtime::resolve(
                &paths::runtime_root(&self.data_dir),
                paths::exe_dir().as_deref(),
                &next.tvmcp_path,
            );
        }
        self.settings = next;
        Ok(())
    }

    /// The error never carries the value.
    pub fn set_tv_session(&mut self, value: &str) -> Result<(), UserError> {
        if !crate::macro_env::looks_like_session_id(value) {
            return Err(UserError::new(keys::SECRET_BAD));
        }
        secrets::set_tv_session(self.secrets.as_ref(), value)
            .map_err(|e| UserError::new(keys::SECRET_STORE).var("detail", e.0))?;
        // A new cookie is unverified until session_check says otherwise.
        let mut s = self.settings.clone();
        s.tv_session_verified_at = None;
        self.save_settings(s)
    }

    pub fn clear_tv_session(&mut self) -> Result<(), UserError> {
        self.secrets
            .delete(secrets::TV_SESSION)
            .map_err(|e| UserError::new(keys::SECRET_STORE).var("detail", e.0))?;
        let mut s = self.settings.clone();
        s.tv_session_verified_at = None;
        self.save_settings(s)
    }

    /// What the diagnosis probes need, taken now (the probes run off the loop).
    pub fn diagnose_inputs(
        &self,
        vault: Option<&Path>,
        now: DateTime<Utc>,
    ) -> crate::diagnose::Inputs {
        crate::diagnose::Inputs {
            settings: self.settings.clone(),
            vault: vault.map(Path::to_path_buf),
            runtime: self.runtime.clone(),
            claude: self.claude_bin(),
            has_tv_session: self.has_tv_session(),
            exe: self.exe.clone(),
            has_template: !crate::vault_template::builtin().is_empty(),
            now,
        }
    }

    /// «Пропустить» on a wizard page.
    pub fn skip_step(&mut self, id: StepId) -> Result<(), UserError> {
        let mut s = self.settings.clone();
        s.setup_skipped = wizard::apply_skip(&s.setup_skipped, id, true);
        self.save_if_changed(s)
    }

    /// «Готово»: the wizard is not opened by itself again.
    pub fn finish_wizard(&mut self, now: DateTime<Utc>) -> Result<(), UserError> {
        let s = wizard::finish(&self.settings, &self.settings.setup_skipped, now);
        self.save_settings(s)
    }

    /// A skipped page that is green now is no longer skipped. `Ok(true)` = saved.
    pub fn reconcile_skips(&mut self, checks: &[Check]) -> Result<bool, UserError> {
        let kept = wizard::reconcile_skips(&self.settings.setup_skipped, checks);
        if kept == self.settings.setup_skipped {
            return Ok(false);
        }
        let mut s = self.settings.clone();
        s.setup_skipped = kept;
        self.save_settings(s).map(|()| true)
    }

    fn save_if_changed(&mut self, next: Settings) -> Result<(), UserError> {
        if next == self.settings {
            return Ok(());
        }
        self.save_settings(next)
    }

    /// «Очистить рантайм»: refused while a task could be using it.
    pub fn clear_runtime(&mut self) -> Result<(), UserError> {
        if self.tasks.any_running() {
            return Err(UserError::new(keys::RUNTIME_BUSY));
        }
        runtime::clear(&paths::runtime_root(&self.data_dir))
            .map_err(|e| UserError::new(keys::RUNTIME_CLEAR).var("detail", e.to_string()))
    }

    /// Start a task by id. A refusal ends it at once with its key, so the UI
    /// shows it where it shows every other outcome.
    pub fn run_task(&mut self, id: TaskId, vault: Option<&Path>, forward: tasks::Forward) {
        if self.tasks.get(id).is_some_and(|t| t.running) {
            return;
        }
        let claude = self.claude_bin();
        let session = secrets::tv_session(self.secrets.as_ref());
        let template = crate::vault_template::builtin();
        let ctx = PlanCtx {
            settings: &self.settings,
            vault,
            runtime: &self.runtime,
            claude: claude.as_deref(),
            tv_session: session.as_deref(),
            exe: self.exe.as_deref(),
            data_dir: &self.data_dir,
            platform: Platform::host(),
            has_template: !template.is_empty(),
        };
        self.tasks_dirty = true;
        match tasks::plan(id, &ctx) {
            Err(key) => {
                self.tasks.start(id, String::new());
                self.tasks.end(id, false, key, "");
            }
            Ok(plan) => {
                let after = match &plan {
                    tasks::Plan::Spawn { after, .. } => *after,
                    tasks::Plan::VaultCreate { .. } => After::Nothing,
                };
                self.tasks.start(id, plan.shown());
                let running = tasks::start(id, plan, template, forward);
                self.running.insert(id, (running, after));
            }
        }
    }

    pub fn task_input(&self, id: TaskId, text: &str) {
        // Only the login reads stdin; for the rest the line goes nowhere.
        if id == TaskId::ClaudeLogin {
            if let Some((r, _)) = self.running.get(&id) {
                r.input(text);
            }
        }
    }

    pub fn stop_task(&self, id: TaskId) {
        if let Some((r, _)) = self.running.get(&id) {
            r.kill();
        }
    }

    /// Fold a task event. Returns true when the lights should be refreshed
    /// (a task finished: the runtime, the login or the schedule may differ).
    pub fn on_task(&mut self, ev: TaskEvent, now: DateTime<Utc>) -> Result<bool, UserError> {
        self.tasks_dirty = true;
        match ev {
            TaskEvent::Line(id, l) => {
                self.tasks.line(id, &l);
                Ok(false)
            }
            TaskEvent::End {
                id,
                ok,
                key,
                detail,
            } => {
                let after = self.running.remove(&id).map_or(After::Nothing, |(_, a)| a);
                let mut res = Ok(true);
                let (mut ok, mut key, mut detail) = (ok, key, detail);
                if ok {
                    if let Err(e) = self.after(after, now) {
                        ok = false;
                        key = "task.error.after";
                        detail = e.vars.first().map(|v| v.1.clone()).unwrap_or_default();
                        res = Err(e);
                    }
                }
                self.tasks.end(id, ok, key, &detail);
                res
            }
        }
    }

    fn after(&mut self, after: After, now: DateTime<Utc>) -> Result<(), UserError> {
        let mut s = self.settings.clone();
        match after {
            After::Nothing => return Ok(()),
            After::WriteMarker => {
                let uv = self
                    .runtime
                    .uv_bin
                    .as_ref()
                    .map_or_else(|| "uv".to_string(), |p| p.to_string_lossy().into_owned());
                return runtime::write_marker(&self.runtime, &uv, now.timestamp_millis()).map_err(
                    |e| UserError::new(keys::SETTINGS_WRITE).var("detail", e.to_string()),
                );
            }
            After::SessionVerified => s.tv_session_verified_at = Some(now),
            After::ScheduleOn => s.schedule.enabled = true,
            After::ScheduleOff => s.schedule.enabled = false,
        }
        self.save_settings(s)
    }
}

/// When the lights should next refresh by themselves: 5 minutes before the
/// next scheduled window (Kyiv clock; a weekend has none, so a week ahead).
pub fn next_prewindow(now: DateTime<Utc>, s: &Settings) -> Option<DateTime<Utc>> {
    let kyiv = chrono_tz::Europe::Kyiv;
    let today = now.with_timezone(&kyiv).date_naive();
    (0..8u64)
        .filter_map(|k| today.checked_add_days(Days::new(k)))
        .flat_map(|d| planned_runs(d, s).into_iter().map(move |r| (d, r.at_local)))
        .filter_map(|(d, t)| {
            use chrono::TimeZone;
            let at = kyiv
                .from_local_datetime(&d.and_time(t))
                .earliest()?
                .with_timezone(&Utc);
            Some(at - chrono::Duration::minutes(5))
        })
        .filter(|at| *at > now)
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use chrono::TimeZone;

    fn ops(data: &Path) -> Ops {
        Ops::new(
            data.to_path_buf(),
            None,
            None,
            Arc::new(MemoryStore::default()),
            None,
            ClaudeLaunch::default(),
        )
        .0
    }

    #[test]
    fn the_session_never_lands_in_settings() {
        let d = tempfile::tempdir().unwrap();
        let mut o = ops(d.path());
        assert!(!o.has_tv_session());
        assert_eq!(o.set_tv_session("nope").unwrap_err().key, keys::SECRET_BAD);
        o.set_tv_session("CANARYa1b2c3d4e5f6a1b2c3d4e5f6").unwrap();
        assert!(o.has_tv_session());
        let text = std::fs::read_to_string(paths::settings_file(d.path())).unwrap();
        assert!(!text.contains("CANARY"), "{text}");
        o.clear_tv_session().unwrap();
        assert!(!o.has_tv_session());
    }

    #[tokio::test]
    async fn a_refused_task_ends_with_its_key_and_after_hooks_save() {
        let d = tempfile::tempdir().unwrap();
        let mut o = ops(d.path());
        let fwd: tasks::Forward = Arc::new(|_| {});
        o.run_task(TaskId::SessionCheck, None, Arc::clone(&fwd));
        let t = o.tasks.get(TaskId::SessionCheck).unwrap();
        assert_eq!(
            (t.running, t.ok, t.error_key.as_str()),
            (false, Some(false), "task.refused.no_session")
        );
        // A finished register switches the schedule on in the settings.
        o.running.clear();
        let now = Utc.with_ymd_and_hms(2026, 11, 27, 10, 0, 0).unwrap();
        o.after(After::ScheduleOn, now).unwrap();
        assert!(o.settings.schedule.enabled);
        o.after(After::SessionVerified, now).unwrap();
        let back = SettingsStore::new(d.path()).load(None, None).0;
        assert!(back.schedule.enabled);
        assert_eq!(back.tv_session_verified_at, Some(now));
    }

    #[test]
    fn the_lights_refresh_five_minutes_before_a_window() {
        let s = Settings::default();
        // 27.11, 09:00 Kyiv (UTC+2) → 09:25 Kyiv.
        let now = Utc.with_ymd_and_hms(2026, 11, 27, 7, 0, 0).unwrap();
        assert_eq!(
            next_prewindow(now, &s),
            Some(Utc.with_ymd_and_hms(2026, 11, 27, 7, 25, 0).unwrap())
        );
        // Friday after 20:00: Monday's 09:25.
        let late = Utc.with_ymd_and_hms(2026, 11, 27, 19, 0, 0).unwrap();
        assert_eq!(
            next_prewindow(late, &s),
            Some(Utc.with_ymd_and_hms(2026, 11, 30, 7, 25, 0).unwrap())
        );
    }
}
