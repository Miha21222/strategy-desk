//! What the connection screens read, per CONTRACTS «Подключения, мастер,
//! настройки, расписание (этап 6)».
//!
//! desk-core (`settings`, `preflight`, `wizard`) and desk-app (`status`,
//! `tasks`, the settings store, secrets) are being built in parallel, so the
//! contract's shapes are spelled out here. When they land these become
//! `pub use` of the real types and [`conn_of`] reads the snapshot.

use std::path::PathBuf;

use chrono::{DateTime, NaiveTime, Utc};
use desk_app::Snapshot;

/// A light, a check: the four states the screens colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    Ok,
    Warn,
    Fail,
    /// Not checked (yet), or skipped because something before it failed.
    #[default]
    Unknown,
}

/// One sidebar light: its state and a short line (an i18n key + vars).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Light {
    pub state: State,
    pub detail_key: String,
    pub vars: Vec<(String, String)>,
}

/// `StatusLights { tradingview, cdp, claude, vault }`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusLights {
    pub tradingview: Light,
    pub cdp: Light,
    pub claude: Light,
    pub vault: Light,
    /// When the lights were last probed.
    pub checked_at: Option<DateTime<Utc>>,
}

/// `preflight::CheckId`, in the order of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CheckId {
    Vault,
    Claude,
    ClaudeAuth,
    Uv,
    Tvmcp,
    Tradingview,
    Cdp,
    TvSession,
    Scheduler,
    Timezone,
}

impl CheckId {
    pub const ALL: [CheckId; 10] = [
        CheckId::Vault,
        CheckId::Claude,
        CheckId::ClaudeAuth,
        CheckId::Uv,
        CheckId::Tvmcp,
        CheckId::Tradingview,
        CheckId::Cdp,
        CheckId::TvSession,
        CheckId::Scheduler,
        CheckId::Timezone,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
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
}

/// The tasks the app may run for the user; the UI names an id, never a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskId {
    ClaudeInstall,
    ClaudeLogin,
    ProvisionRuntime,
    VaultCreate,
    SessionCheck,
    SchedulerRegister,
    SchedulerUnregister,
}

impl TaskId {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskId::ClaudeInstall => "claude-install",
            TaskId::ClaudeLogin => "claude-login",
            TaskId::ProvisionRuntime => "provision-runtime",
            TaskId::VaultCreate => "vault-create",
            TaskId::SessionCheck => "session-check",
            TaskId::SchedulerRegister => "scheduler-register",
            TaskId::SchedulerUnregister => "scheduler-unregister",
        }
    }
}

/// What «Исправить» does. The app acts on its own only for these; anything
/// else that touches the system comes back as text to copy.
#[derive(Debug, Clone, PartialEq)]
pub enum Fix {
    Task(TaskId),
    Copy { text: String, explain_key: String },
    LaunchTradingView,
    PickFolder,
    SetCdpPort(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub id: CheckId,
    pub state: State,
    pub detail_key: String,
    pub vars: Vec<(String, String)>,
    pub fix: Option<Fix>,
    /// The fix's own words («Создать здесь»): the hover text of «Исправить».
    pub fix_label_key: Option<String>,
    /// What the fix will do, or what the owner does himself when there is none.
    pub note_key: Option<String>,
}

/// A task as it runs: its output, line by line.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskView {
    pub id: TaskId,
    pub state: TaskState,
    pub lines: Vec<String>,
    /// `claude_login` waits for the code from the browser.
    pub wants_input: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskState {
    Running,
    Done,
    Failed {
        key: String,
        vars: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// `settings.macroMcp`: the review chat's TradingView server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacroMcp {
    /// The desk's own tvmcp (fresh installs).
    #[default]
    Bundled,
    /// Whatever the owner's `~/.claude` provides.
    User,
}

/// `Settings.schedule`: local times of the three runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub eu: NaiveTime,
    pub us: NaiveTime,
    pub review: NaiveTime,
    pub enabled: bool,
}

impl Default for Schedule {
    fn default() -> Self {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default();
        Self {
            eu: t(9, 30),
            us: t(16, 0),
            review: t(20, 0),
            enabled: false,
        }
    }
}

/// A wizard page (`wizard` in desk-core): the order is the old `stepIdsFor()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepId {
    Vault,
    Claude,
    Tvmcp,
    Tradingview,
    Session,
    Schedule,
}

impl StepId {
    pub const ALL: [StepId; 6] = [
        StepId::Vault,
        StepId::Claude,
        StepId::Tvmcp,
        StepId::Tradingview,
        StepId::Session,
        StepId::Schedule,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StepId::Vault => "vault",
            StepId::Claude => "claude",
            StepId::Tvmcp => "tvmcp",
            StepId::Tradingview => "tradingview",
            StepId::Session => "session",
            StepId::Schedule => "schedule",
        }
    }

    /// The checks a page shows (and that make it «пройден»).
    pub fn checks(self) -> &'static [CheckId] {
        match self {
            StepId::Vault => &[CheckId::Vault],
            StepId::Claude => &[CheckId::Claude, CheckId::ClaudeAuth],
            StepId::Tvmcp => &[CheckId::Uv, CheckId::Tvmcp],
            StepId::Tradingview => &[CheckId::Tradingview, CheckId::Cdp],
            StepId::Session => &[CheckId::TvSession],
            StepId::Schedule => &[CheckId::Scheduler, CheckId::Timezone],
        }
    }

    /// Tasks a page presses by itself when it opens and its check asks for
    /// them (the old `AUTO_TASKS`): once per wizard session, never a retry
    /// loop. The login, the schedule and anything that removes something
    /// wait for the button.
    pub fn auto_tasks(self) -> &'static [TaskId] {
        match self {
            StepId::Claude => &[TaskId::ClaudeInstall],
            StepId::Tvmcp => &[TaskId::ProvisionRuntime],
            StepId::Session => &[TaskId::SessionCheck],
            _ => &[],
        }
    }
}

/// The settings the screens show. Secrets never come back: only whether
/// the sessionid is stored and when it last worked.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsView {
    pub vault: Option<PathBuf>,
    pub theme: Theme,
    /// UI zoom, 1.0 = 100 %.
    pub scale: f32,
    pub macro_mcp: MacroMcp,
    pub schedule: Schedule,
    pub setup_skipped: Vec<StepId>,
    pub has_tv_session: bool,
    pub tv_session_verified_at: Option<DateTime<Utc>>,
    /// No settings file yet: the wizard opens by itself.
    pub first_run: bool,
    /// Where the runtime lives (for «Очистить рантайм»).
    pub runtime_dir: Option<PathBuf>,
}

/// Everything the connection screens read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConnView {
    pub settings: SettingsView,
    pub lights: StatusLights,
    /// Empty until the first preflight.
    pub checks: Vec<Check>,
    pub checked_at: Option<DateTime<Utc>>,
    /// A preflight is running.
    pub checking: bool,
    pub tasks: Vec<TaskView>,
}

impl ConnView {
    pub fn check(&self, id: CheckId) -> Option<&Check> {
        self.checks.iter().find(|c| c.id == id)
    }

    pub fn task(&self, id: TaskId) -> Option<&TaskView> {
        self.tasks.iter().rev().find(|t| t.id == id)
    }

    pub fn running(&self, id: TaskId) -> bool {
        self.task(id).is_some_and(|t| t.state == TaskState::Running)
    }

    /// Fail/warn checks: «Требует внимания: n».
    pub fn problems(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| matches!(c.state, State::Fail | State::Warn))
            .count()
    }
}

/// The connection state of a snapshot. desk-app's `StatusLights`,
/// `TasksView` and settings have no fields yet: everything reads «не
/// проверяли» until they land.
pub fn conn_of(_snap: &Snapshot) -> ConnView {
    ConnView {
        settings: SettingsView {
            scale: 1.0,
            ..SettingsView::default()
        },
        ..ConnView::default()
    }
}
