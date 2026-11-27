//! The desk's settings (`%APPDATA%\strategy-desk-rs\settings.json`, written by
//! desk-app), and the one-time import of the old Electron `config.json`
//! (`src/shared/defaults.ts`: `DEFAULT_SETTINGS`, `macroMcpFor`,
//! `legacyConfigPatch`).
//!
//! Reading is tolerant the way the vault files are: an unknown field is
//! ignored, a missing or wrong-typed one takes its default, so a settings
//! file from an older or newer build never fails the start.

use crate::js;
use crate::vault::lenient;
use crate::wizard::StepId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The CDP port TradingView Desktop is started with (`DEFAULT_CDP_PORT`): 9222 usually belongs to some other app.
pub const DEFAULT_CDP_PORT: u16 = 9223;

/// Where the preflight's «git clone» fix points (`DEFAULT_TVMCP_REPO`). The app never runs it itself.
pub const DEFAULT_TVMCP_REPO: &str = "https://github.com/Miha21222/tradingview-mcp.git";

/// `theme`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// `macroMcp`: what a macro session gets as its `tradingview` MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroMcp {
    /// The desk's own server (fresh installs).
    #[default]
    Bundled,
    /// Nothing passed: the owner's `~/.claude` provides it.
    User,
}

/// Which scheduled runs exist. The minutes are not stored: they come from the
/// exchanges' clocks ([`crate::schedule::planned_runs`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScheduleSettings {
    #[serde(deserialize_with = "on_by_default")]
    pub eu: bool,
    #[serde(deserialize_with = "on_by_default")]
    pub us: bool,
    #[serde(deserialize_with = "on_by_default")]
    pub review: bool,
    /// The whole schedule is on (the Task Scheduler jobs should exist).
    #[serde(deserialize_with = "lenient::de")]
    pub enabled: bool,
}

impl Default for ScheduleSettings {
    /// All three windows, but off until the owner registers them in the wizard.
    fn default() -> Self {
        ScheduleSettings {
            eu: true,
            us: true,
            review: true,
            enabled: false,
        }
    }
}

/// The settings of the Rust desk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The vault folder. `None` = not chosen (desk-app offers `Documents\Strategy Desk`,
    /// as the old store did); a blank string reads as `None`.
    #[serde(deserialize_with = "vault_path")]
    pub vault: Option<String>,
    /// `claude` executable; empty = find it (PATH, the official installer's place).
    #[serde(deserialize_with = "lenient::de")]
    pub claude_bin: String,
    /// A tvmcp checkout of the owner's own; empty = the app's provisioned runtime.
    #[serde(deserialize_with = "lenient::de")]
    pub tvmcp_path: String,
    #[serde(deserialize_with = "port")]
    pub cdp_port: u16,
    #[serde(deserialize_with = "lenient::de")]
    pub theme: Theme,
    /// UI scale (`uiScale`), clamped to 0.5–3.
    #[serde(deserialize_with = "scale")]
    pub scale: f32,
    #[serde(deserialize_with = "lenient::de")]
    pub macro_mcp: MacroMcp,
    #[serde(deserialize_with = "lenient::de")]
    pub schedule: ScheduleSettings,
    /// Wizard steps the owner skipped (unknown ids dropped).
    #[serde(deserialize_with = "steps")]
    pub setup_skipped: Vec<StepId>,
    /// When the wizard was finished; `None` = open it on start (`setupCompletedAt`).
    #[serde(deserialize_with = "lenient::de")]
    pub setup_completed_at: Option<DateTime<Utc>>,
    /// TradingView Desktop's exe when it is not where desk-app looks (`tvDesktopExe`); empty = find it.
    #[serde(deserialize_with = "lenient::de")]
    pub tv_desktop_exe: String,
    /// When the stored sessionid last passed `session_check`; `None` = never, or a new cookie.
    #[serde(deserialize_with = "lenient::de")]
    pub tv_session_verified_at: Option<DateTime<Utc>>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            vault: None,
            claude_bin: String::new(),
            tvmcp_path: String::new(),
            cdp_port: DEFAULT_CDP_PORT,
            theme: Theme::System,
            scale: 1.0,
            macro_mcp: MacroMcp::Bundled,
            schedule: ScheduleSettings::default(),
            setup_skipped: Vec::new(),
            setup_completed_at: None,
            tv_desktop_exe: String::new(),
            tv_session_verified_at: None,
        }
    }
}

const SCALE_RANGE: (f32, f32) = (0.5, 3.0);

fn port_of(v: &Value) -> Option<u16> {
    v.as_u64()
        .filter(|p| (1..=65535).contains(p))
        .and_then(|p| u16::try_from(p).ok())
}

fn scale_of(v: &Value) -> Option<f32> {
    v.as_f64()
        .filter(|s| s.is_finite() && *s > 0.0)
        .map(|s| (s as f32).clamp(SCALE_RANGE.0, SCALE_RANGE.1))
}

fn steps_of(v: &Value) -> Vec<StepId> {
    let mut out: Vec<StepId> = v
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().and_then(StepId::parse))
        .collect();
    crate::wizard::order_steps(&mut out);
    out
}

/// A window toggle: a wrong type reads as the default (on), not as `false`.
fn on_by_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Value::deserialize(d)?.as_bool().unwrap_or(true))
}

/// A folder path: blank or not a string is «not chosen».
fn path_of(v: &Value) -> Option<String> {
    v.as_str()
        .map(js::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
}

fn vault_path<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(path_of(&Value::deserialize(d)?))
}

fn port<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u16, D::Error> {
    Ok(port_of(&Value::deserialize(d)?).unwrap_or(DEFAULT_CDP_PORT))
}

fn scale<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    Ok(scale_of(&Value::deserialize(d)?).unwrap_or(1.0))
}

fn steps<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<StepId>, D::Error> {
    Ok(steps_of(&Value::deserialize(d)?))
}

impl Settings {
    /// Read `settings.json`; anything unreadable is the defaults (never an error at start).
    pub fn from_json(bytes: &[u8]) -> Settings {
        js::parse_json(bytes)
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default()
    }
}

/// `macroMcpFor`: `None` (no old file) → bundled; a file from before the field
/// existed → `user` (the owner's `~/.claude` already provides the server, a
/// second one must not be added); otherwise what the file says, junk → `user`.
pub fn macro_mcp_for(existing: Option<&Value>) -> MacroMcp {
    match existing {
        None => MacroMcp::Bundled,
        Some(v) => match js::prop(Some(v), "macroMcp").and_then(Value::as_str) {
            Some("bundled") => MacroMcp::Bundled,
            _ => MacroMcp::User,
        },
    }
}

/// The settings carried over from the Electron desk's `config.json`
/// (`%APPDATA%\strategy-desk\config.json`): the vault, the tvmcp checkout, the
/// CDP port, the theme, the scale, `macroMcp` (by `legacyConfigPatch`'s rule)
/// and the wizard's progress. Not carried: the secrets (safeStorage is bound
/// to Chromium; the sessionid is entered once again), and the schedule, which
/// the old app kept in the Task Scheduler, not in its config: it starts off
/// here, so the two apps never both run the skill for the same window.
pub fn import_electron(old: &Value) -> Settings {
    let d = Settings::default();
    let s = |k: &str| {
        js::prop(Some(old), k)
            .and_then(Value::as_str)
            .map(|x| js::trim(x).to_owned())
    };
    let theme = match js::prop(Some(old), "theme").and_then(Value::as_str) {
        Some("light") => Theme::Light,
        Some("dark") => Theme::Dark,
        _ => Theme::System,
    };
    let completed = js::prop(Some(old), "setupCompletedAt")
        .and_then(Value::as_f64)
        .filter(|ms| *ms > 0.0)
        .and_then(|ms| DateTime::<Utc>::from_timestamp_millis(ms as i64));
    Settings {
        vault: js::prop(Some(old), "vaultPath").and_then(path_of),
        claude_bin: String::new(),
        tvmcp_path: s("tvmcpPath").unwrap_or_default(),
        cdp_port: js::prop(Some(old), "cdpPort")
            .and_then(port_of)
            .unwrap_or(d.cdp_port),
        theme,
        scale: js::prop(Some(old), "uiScale")
            .and_then(scale_of)
            .unwrap_or(d.scale),
        // A config that is an object (or an array, which the old reader took for one) is an
        // existing install; a string or a number is as good as no file at all.
        macro_mcp: macro_mcp_for(Some(old).filter(|v| v.is_object() || v.is_array())),
        schedule: ScheduleSettings::default(),
        setup_skipped: js::prop(Some(old), "setupSkipped")
            .map(steps_of)
            .unwrap_or_default(),
        setup_completed_at: completed,
        tv_desktop_exe: s("tvDesktopExe").unwrap_or_default(),
        // The cookie itself is not carried over, so neither is its check.
        tv_session_verified_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- ported from defaults.test.ts

    #[test]
    fn macro_mcp_is_bundled_fresh_user_for_an_old_file_and_otherwise_as_written() {
        assert_eq!(macro_mcp_for(None), MacroMcp::Bundled);
        assert_eq!(
            macro_mcp_for(Some(&json!({"vaultPath": "C:\\vault"}))),
            MacroMcp::User
        );
        assert_eq!(
            macro_mcp_for(Some(&json!({"macroMcp": "bundled"}))),
            MacroMcp::Bundled
        );
        assert_eq!(
            macro_mcp_for(Some(&json!({"macroMcp": "nonsense"}))),
            MacroMcp::User
        );
        assert_eq!(Settings::default().macro_mcp, MacroMcp::Bundled);
    }

    #[test]
    fn the_defaults_name_no_machine() {
        let d = Settings::default();
        assert_eq!(
            (
                d.vault.as_deref(),
                d.tvmcp_path.as_str(),
                d.claude_bin.as_str()
            ),
            (None, "", "")
        );
        assert!(!serde_json::to_string(&d).unwrap().contains("Admin"));
        assert_eq!(d.cdp_port, 9223);
        assert!(!d.schedule.enabled);
    }

    // ---- import_electron

    #[test]
    fn imports_the_owners_old_config() {
        let old = json!({
            "engine": "claude", "vaultPath": " C:\\Users\\Admin\\Trading Strategy Assistant ", "tvmcpPath": "C:\\Users\\Admin\\Projects\\tradingview-mcp",
            "cdpPort": 9224, "theme": "dark", "uiScale": 1.25, "setupSkipped": ["files", "schedule", "nonsense", "claude"],
            "setupCompletedAt": 1_760_000_000_000_i64, "hasTvSession": true, "apiKey": "secret?", "knowledgeBase": true
        });
        let s = import_electron(&old);
        assert_eq!(
            s.vault.as_deref(),
            Some("C:\\Users\\Admin\\Trading Strategy Assistant")
        );
        assert_eq!(s.tvmcp_path, "C:\\Users\\Admin\\Projects\\tradingview-mcp");
        assert_eq!(s.cdp_port, 9224);
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.scale, 1.25);
        // Written before `macroMcp` existed: the owner's ~/.claude provides the server.
        assert_eq!(s.macro_mcp, MacroMcp::User);
        // Legacy-only steps are dropped, the rest kept in wizard order.
        assert_eq!(s.setup_skipped, [StepId::Claude, StepId::Schedule]);
        assert_eq!(
            s.setup_completed_at.unwrap().timestamp_millis(),
            1_760_000_000_000
        );
        // Secrets never travel; the schedule starts off.
        let text = serde_json::to_string(&s).unwrap();
        assert!(!text.contains("secret"));
        assert!(!s.schedule.enabled);
    }

    #[test]
    fn import_tolerates_junk_and_a_fresh_config() {
        let s = import_electron(
            &json!({"cdpPort": 70000, "theme": 3, "uiScale": "big", "macroMcp": "bundled", "setupSkipped": "x", "setupCompletedAt": 0}),
        );
        assert_eq!(
            (s.cdp_port, s.theme, s.scale, s.macro_mcp),
            (9223, Theme::System, 1.0, MacroMcp::Bundled)
        );
        assert!(s.setup_skipped.is_empty() && s.setup_completed_at.is_none());
        assert_eq!(import_electron(&json!([1, 2])).macro_mcp, MacroMcp::User);
        assert_eq!(import_electron(&json!({"uiScale": 9})).scale, 3.0);
    }

    #[test]
    fn import_reads_a_blank_vault_as_not_chosen_and_non_objects_as_no_file() {
        assert_eq!(import_electron(&json!({"vaultPath": "   "})).vault, None);
        assert_eq!(import_electron(&json!({"vaultPath": 5})).vault, None);
        assert_eq!(import_electron(&json!({})).macro_mcp, MacroMcp::User);
        // An array passed the old reader's `typeof === 'object'`: an existing install.
        assert_eq!(import_electron(&json!([1, 2])).macro_mcp, MacroMcp::User);
        for not_a_config in [json!("hello"), json!(42), json!(null)] {
            assert_eq!(import_electron(&not_a_config), Settings::default());
        }
    }

    // ---- settings.json

    #[test]
    fn settings_round_trip_and_survive_old_or_wrong_fields() {
        let mut s = Settings {
            vault: Some("D:\\v".into()),
            cdp_port: 9225,
            theme: Theme::Light,
            ..Settings::default()
        };
        s.schedule.us = false;
        s.schedule.enabled = true;
        s.setup_skipped = vec![StepId::Session];
        let text = serde_json::to_vec(&s).unwrap();
        assert_eq!(Settings::from_json(&text), s);
        let odd = br#"{"vault": 5, "cdp_port": "x", "theme": "neon", "scale": -1, "schedule": {"eu": "yes", "enabled": true}, "setup_skipped": ["tvmcp", "files", 7], "future_field": {"a": 1}}"#;
        let r = Settings::from_json(odd);
        assert_eq!(r.vault, None);
        assert_eq!((r.cdp_port, r.theme, r.scale), (9223, Theme::System, 1.0));
        assert_eq!(
            r.schedule,
            ScheduleSettings {
                eu: true,
                us: true,
                review: true,
                enabled: true
            }
        );
        assert_eq!(r.setup_skipped, [StepId::Tvmcp]);
        assert_eq!(Settings::from_json(b"{broken"), Settings::default());
        assert_eq!(Settings::from_json(b""), Settings::default());
    }
}
