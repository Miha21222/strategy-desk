//! What the connection screens read: desk-app's snapshot fields of stage 6
//! (CONTRACTS «Подключения, мастер, настройки, расписание»), gathered in one
//! cheap struct of `Arc`s so a test can hand-make it.

use std::sync::Arc;

use chrono::NaiveDate;
use desk_app::Snapshot;

pub use desk_app::status::{Light, LightState, StatusLights};
pub use desk_app::tasks::{TaskId, TaskState, TasksView};
pub use desk_core::preflight::{Check, CheckId, CheckState, Fix};
pub use desk_core::settings::{MacroMcp, ScheduleSettings, Settings, Theme};
pub use desk_core::wizard::{PickField, StepId};

/// Everything the connection screens read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConnView {
    pub settings: Arc<Settings>,
    pub lights: Arc<StatusLights>,
    pub checks: Arc<Vec<Check>>,
    /// When `checks` was gathered (Unix ms); 0 = not yet («проверяю…»).
    pub checks_at: i64,
    pub tasks: Arc<TasksView>,
    /// The wizard was never finished: it opens by itself (again).
    pub first_run: bool,
    pub has_tv_session: bool,
    /// Today on the Kyiv clock: the schedule shows its runs' times for it.
    pub today: NaiveDate,
}

impl ConnView {
    pub fn check(&self, id: CheckId) -> Option<&Check> {
        self.checks.iter().find(|c| c.id == id)
    }

    pub fn task(&self, id: TaskId) -> Option<&TaskState> {
        self.tasks.get(id)
    }

    pub fn running(&self, id: TaskId) -> bool {
        self.task(id).is_some_and(|t| t.running)
    }

    /// Fail/warn checks: «Требует внимания: n».
    pub fn problems(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| matches!(c.state, CheckState::Fail | CheckState::Warn))
            .count()
    }

    /// The settings with one edit, for `SaveSettings`.
    pub fn edited(&self, edit: impl FnOnce(&mut Settings)) -> Box<Settings> {
        let mut s = (*self.settings).clone();
        edit(&mut s);
        Box::new(s)
    }
}

/// The connection state of a snapshot.
pub fn conn_of(snap: &Snapshot, today: NaiveDate) -> ConnView {
    ConnView {
        settings: Arc::clone(&snap.settings),
        lights: Arc::clone(&snap.status),
        checks: Arc::clone(&snap.checks),
        checks_at: snap.checks_at,
        tasks: Arc::clone(&snap.tasks),
        first_run: snap.first_run,
        has_tv_session: snap.has_tv_session,
        today,
    }
}

/// `CheckId` as the i18n keys spell it (`conn.check.<id>`).
pub fn check_key(id: CheckId) -> &'static str {
    match id {
        CheckId::Vault => "vault",
        CheckId::Claude => "claude",
        CheckId::ClaudeAuth => "claude_auth",
        CheckId::Uv => "uv",
        CheckId::Tvmcp => "tvmcp",
        CheckId::Tradingview => "tradingview",
        CheckId::Cdp => "cdp",
        CheckId::TvSession => "tv_session",
        CheckId::Scheduler => "scheduler",
        CheckId::Timezone => "timezone",
    }
}
