//! Stand-ins for desk-core's stage-6 pieces (`settings`, `schedule`), which
//! the core agent is writing in parallel (CONTRACTS «… (этап 6)»).
//!
//! TODO(core): replace with `desk_core::settings::{Settings, import_electron}`
//! and `desk_core::schedule::{planned_runs, due_window, PlannedRun}`, then
//! delete this module. Shapes follow the contract so the swap is a rename.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use desk_core::windows::{kyiv_now, standard_times, window_times, WindowId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Kyiv clock of the three scheduled runs, and the switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleSettings {
    pub eu: String,
    pub us: String,
    pub review: String,
    pub enabled: bool,
}

impl Default for ScheduleSettings {
    fn default() -> Self {
        Self {
            eu: "09:30".into(),
            us: "16:00".into(),
            review: "20:00".into(),
            enabled: false,
        }
    }
}

/// TODO(core): `desk_core::settings::Settings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The vault folder; empty = not chosen yet.
    pub vault: String,
    /// Claude Code binary; empty = `DESK_CLAUDE_BIN`, then PATH.
    pub claude_bin: String,
    /// A tradingview-mcp checkout (external runtime); empty = bundled.
    pub tvmcp_path: String,
    pub cdp_port: u16,
    pub theme: String,
    pub scale: f32,
    /// `bundled` | `user`.
    pub macro_mcp: String,
    pub schedule: ScheduleSettings,
    pub setup_skipped: Vec<String>,
    /// TradingView Desktop exe when it is not where it is looked for.
    pub tv_desktop_exe: String,
    /// Unix ms of the last passing `session_check`; 0 = never.
    pub tv_session_verified_at: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            vault: String::new(),
            claude_bin: String::new(),
            tvmcp_path: String::new(),
            cdp_port: crate::macro_env::DEFAULT_CDP_PORT,
            theme: "system".into(),
            scale: 1.0,
            macro_mcp: "bundled".into(),
            schedule: ScheduleSettings::default(),
            setup_skipped: Vec::new(),
            tv_desktop_exe: String::new(),
            tv_session_verified_at: 0,
        }
    }
}

/// TODO(core): `desk_core::settings::import_electron`. What the old app's
/// `config.json` says, where it says it; secrets are never carried over.
pub fn import_electron(old: &Value) -> Settings {
    let mut s = Settings::default();
    let str_of = |k: &str| {
        old.get(k)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    s.vault = str_of("vaultPath");
    s.tvmcp_path = str_of("tvmcpPath");
    s.tv_desktop_exe = str_of("tvDesktopExe");
    if let Some(p) = old
        .get("cdpPort")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
    {
        s.cdp_port = p;
    }
    if let Some(t) = old.get("theme").and_then(Value::as_str) {
        s.theme = t.to_string();
    }
    if let Some(z) = old.get("uiScale").and_then(Value::as_f64) {
        s.scale = z as f32;
    }
    // An old config without the field is the owner's: his ~/.claude has the server.
    s.macro_mcp = match old.get("macroMcp").and_then(Value::as_str) {
        Some("bundled") => "bundled".into(),
        _ => "user".into(),
    };
    s
}

/// TODO(core): `desk_core::schedule::PlannedRun`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedRun {
    pub window: WindowId,
    /// The PC's local clock (assumed = Kyiv here; core owns the conversion).
    pub at_local: NaiveTime,
}

fn configured(id: WindowId, s: &Settings) -> NaiveTime {
    let raw = match id {
        WindowId::Eu => &s.schedule.eu,
        WindowId::Us => &s.schedule.us,
        WindowId::Review => &s.schedule.review,
    };
    NaiveTime::parse_from_str(raw.trim(), "%H:%M").unwrap_or(standard_times(id).at)
}

/// The run of `id` on `date`: the configured clock shifted as the exchange
/// shifts the window that day (TZ item 21).
fn run_at(id: WindowId, date: NaiveDate, s: &Settings) -> NaiveTime {
    let shift = window_times(id, date).at - standard_times(id).at;
    configured(id, s) + shift
}

/// TODO(core): `desk_core::schedule::planned_runs`.
pub fn planned_runs(date: NaiveDate, s: &Settings) -> Vec<PlannedRun> {
    WindowId::ALL
        .iter()
        .map(|&w| PlannedRun {
            window: w,
            at_local: run_at(w, date, s),
        })
        .collect()
}

/// TODO(core): `desk_core::schedule::due_window` — the window whose run
/// time today is within ±10 minutes of `now` (Kyiv clock).
pub fn due_window(now: DateTime<Utc>, s: &Settings) -> Option<WindowId> {
    let k = kyiv_now(now);
    let t = NaiveTime::from_num_seconds_from_midnight_opt(k.minutes * 60, 0)?;
    WindowId::ALL.iter().copied().find(|&w| {
        let at = run_at(w, k.date, s);
        (t - at).abs() <= Duration::minutes(10)
    })
}
