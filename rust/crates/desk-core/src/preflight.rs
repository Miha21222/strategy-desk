//! The setup checks: facts gathered by desk-app → the ordered check list
//! (`buildChecks` of `src/shared/preflight.ts`, without the course knowledge
//! base, the official MCP, Codex and the legacy desk's checks).
//!
//! Two rules the old app kept and this one keeps:
//! - **The app never acts on its own initiative and never hides what it will
//!   run.** Installing someone's toolchain, cloning into their folders or
//!   running a command against their system is copyable text ([`Fix::Copy`]);
//!   what the app owns (its Python environment, the vault template, its own
//!   schedule, the Claude Code install and login) is a task ([`Fix::Task`]),
//!   started by the press, shown and streamed. The rest is exactly three
//!   presses: launch TradingView, pick a folder, switch the CDP port. Where the
//!   old app could only explain (close TradingView yourself, set the clock,
//!   where the cookie is), there is no fix: the explanation is the check's
//!   `note_key`.
//! - **Nothing is cached.** «Проверить заново» is a real re-probe (desk-app's).
//!
//! The list comes in ONE order, the wizard's pages (vault → claude → tvmcp →
//! tradingview → session → schedule), for the wizard and the diagnosis alike
//! (TEST_PLAN quirk S5: the old diagnosis listed tvmcp first).
//!
//! A check carries i18n keys and vars, never a sentence: Russian stays in the
//! UI. The keys are the old app's where the check is the old one
//! (`preflight.vault.*`, `preflight.tv.*`, …), so `src/renderer/i18n/ru.ts` is
//! where to copy them from.
//!
//! [`Facts`] also deserializes from the old app's `PreflightFacts` JSON
//! (camelCase, unknown fields ignored), so a probe dump of either app feeds it.

use crate::js;
use crate::schedule;
use crate::settings::{Settings, DEFAULT_TVMCP_REPO};
use crate::windows::kyiv_stamp;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The checks, in the order [`build_checks`] lists them (the wizard's pages).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckId {
    /// The vault folder.
    Vault,
    /// Claude Code is installed.
    Claude,
    /// … and logged in.
    ClaudeAuth,
    /// `uv` on PATH — only for a tvmcp checkout of the owner's own (external mode).
    Uv,
    /// tvmcp: the runtime (or the checkout), does it boot, does the vault's Python import it.
    Tvmcp,
    /// TradingView Desktop runs with the debug port.
    Tradingview,
    /// Who owns the CDP port.
    Cdp,
    /// The TradingView sessionid.
    TvSession,
    /// The Task Scheduler jobs.
    Scheduler,
    /// The PC's clock is Kyiv's.
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
}

/// `CheckStatus`. `Skipped` is the old `unknown`: not checked (there was
/// nothing to ask yet, or the probe could not answer) — neither good nor bad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState {
    Ok,
    Warn,
    Fail,
    Skipped,
}

/// What the app may do for the owner, by id: desk-app owns the commands
/// (the UI names an id, never a command).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskId {
    /// Claude Code by its official installer, when it is not there yet.
    ClaudeInstall,
    /// `claude auth login`: a browser page, then the code pasted back (stdin).
    ClaudeLogin,
    /// `uv sync --frozen` of the app's own Python environment.
    ProvisionRuntime,
    /// The vault template into the chosen folder, missing files only.
    VaultCreate,
    /// Exchange the stored sessionid for a token: the cheap proof it works.
    SessionCheck,
    /// The Task Scheduler jobs for [`crate::schedule::triggers`] (the old app's three are removed).
    SchedulerRegister,
    SchedulerUnregister,
}

impl TaskId {
    pub const ALL: [TaskId; 7] = [
        TaskId::ClaudeInstall,
        TaskId::ClaudeLogin,
        TaskId::ProvisionRuntime,
        TaskId::VaultCreate,
        TaskId::SessionCheck,
        TaskId::SchedulerRegister,
        TaskId::SchedulerUnregister,
    ];

    /// The id as the old app spelled it (`provision-runtime`).
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

    pub fn parse(s: &str) -> Option<TaskId> {
        TaskId::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Only the Claude login reads a line from the owner (the code from the browser).
    pub fn wants_stdin(self) -> bool {
        self == TaskId::ClaudeLogin
    }
}

/// The one way forward a check offers («Исправить»).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Fix {
    /// Something the app owns, started by the press.
    Task(TaskId),
    /// A command the owner runs himself, with what it will do.
    Copy {
        text: String,
        explain_key: String,
    },
    LaunchTradingView,
    /// A folder picker; the check says which setting it writes ([`crate::wizard::pick_field`]).
    PickFolder,
    SetCdpPort(u16),
}

/// One row of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub id: CheckId,
    pub state: CheckState,
    /// i18n key of the one-line explanation; `vars` fill its `{placeholders}`.
    pub detail_key: String,
    pub vars: BTreeMap<String, String>,
    pub fix: Option<Fix>,
    /// i18n key of the fix button («preflight.fix.vaultCreate» vs «…vaultComplete»: one task, two words).
    pub fix_label_key: Option<String>,
    /// i18n key of the line under the detail: what the fix will do, or — with no
    /// fix — what the owner does himself (the old `explain` fixes' notes).
    pub note_key: Option<String>,
}

// ---------------------------------------------------------------- the facts

/// The vault folder and the three files that make it a vault: the skill, the scale tool and its config.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VaultFact {
    pub path: String,
    pub exists: bool,
    /// Something is in there («create here» would write among someone's files).
    pub non_empty: bool,
    pub has_skill: bool,
    pub has_scale_tool: bool,
    pub has_scale_config: bool,
    /// What `template.json` says when the folder came from the shipped template; '' otherwise.
    pub template_version: String,
    /// The shipped template is there to copy from.
    pub template_available: bool,
}

/// How Claude Code is logged in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ClaudeAuth {
    Subscription,
    ApiKey,
    SetupToken,
    #[default]
    None,
}

impl ClaudeAuth {
    fn key(self) -> &'static str {
        match self {
            ClaudeAuth::Subscription => "subscription",
            ClaudeAuth::ApiKey => "api-key",
            ClaudeAuth::SetupToken => "setup-token",
            ClaudeAuth::None => "none",
        }
    }
}

/// Claude Code. Deserializes from `{found, version, auth}` or from the old
/// app's bare auth string (`"subscription"` …): the old app shipped its own
/// claude.exe, so there it was always found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClaudeFact {
    /// `claude --version` answered.
    pub found: bool,
    pub version: String,
    pub auth: ClaudeAuth,
}

impl<'de> Deserialize<'de> for ClaudeFact {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let auth = |x: Option<&Value>| {
            x.and_then(|a| ClaudeAuth::deserialize(a).ok())
                .unwrap_or_default()
        };
        Ok(match &v {
            Value::String(_) => ClaudeFact {
                found: true,
                version: String::new(),
                auth: auth(Some(&v)),
            },
            Value::Object(m) => ClaudeFact {
                found: m.get("found").and_then(Value::as_bool).unwrap_or(false),
                version: m
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                auth: auth(m.get("auth")),
            },
            _ => ClaudeFact::default(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct UvFact {
    pub found: bool,
    pub version: String,
}

/// Where tvmcp comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeMode {
    /// The app's own environment, provisioned from the shipped payload.
    #[default]
    Bundled,
    /// A complete checkout of the owner's own plus `uv` on PATH (`preferExternal`).
    External,
}

/// The app's own Python environment (bundled mode).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RuntimeFact {
    pub mode: RuntimeMode,
    pub tvmcp_dir: String,
    /// The shipped payload is intact (pyproject named tvmcp + uv.lock).
    pub payload_ok: bool,
    pub project_name: String,
    /// An environment exists and was built from exactly this uv.lock.
    pub provisioned: bool,
    /// An environment exists, built from another uv.lock.
    pub stale: bool,
    pub python_path: String,
    pub lock_hash: String,
}

/// `pyproject.toml` of the owner's checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Pyproject {
    #[default]
    Ok,
    Missing,
    WrongName,
}

/// The owner's checkout (external mode).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CloneFact {
    pub path: String,
    pub dir_exists: bool,
    pub pyproject: Pyproject,
    pub project_name: String,
}

/// Does tvmcp start, and with how many tools. `ran: false` = not attempted (nothing to start yet).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct BootFact {
    pub ran: bool,
    pub ok: bool,
    pub tools: u32,
    pub error: String,
}

/// The Python the vault tools and the scheduled runs use (`MACRO_PYTHON`):
/// `import_ok` is `python -c "import pandas, tvmcp"`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolsPythonFact {
    pub python: String,
    pub exists: bool,
    pub checked: bool,
    pub import_ok: bool,
    pub error: String,
}

/// A CDP target.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct CdpTarget {
    pub url: String,
    pub title: String,
}

/// Who answers on the CDP port, not merely whether someone does.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CdpProbe {
    pub up: bool,
    /// `Browser` of `/json/version`.
    pub browser: String,
    /// A tradingview.com/chart tab is open.
    pub chart_tab: bool,
    /// Anything on the port looks like TradingView at all.
    pub tradingview: bool,
    pub targets: Vec<CdpTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct TvFact {
    /// The TradingView process is there.
    pub running: bool,
    pub port: u16,
    pub cdp: CdpProbe,
}

/// The stored sessionid and when the cheap check last passed for it (the old
/// JSON's `verifiedAt` in milliseconds, 0 = never).
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TvSessionFact {
    pub set: bool,
    #[serde(deserialize_with = "millis")]
    pub verified_at: Option<DateTime<Utc>>,
}

fn millis<'de, D: Deserializer<'de>>(d: D) -> Result<Option<DateTime<Utc>>, D::Error> {
    let v = Value::deserialize(d)?;
    Ok(match &v {
        Value::Number(n) => n
            .as_f64()
            .filter(|ms| *ms > 0.0)
            .and_then(|ms| DateTime::from_timestamp_millis(ms as i64)),
        Value::String(_) => DateTime::<Utc>::deserialize(&v).ok(),
        _ => None,
    })
}

/// One Task Scheduler job as `Get-ScheduledTask` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct ScheduledTask {
    pub name: String,
    /// `Ready` / `Disabled` / `Running` …
    pub state: String,
    /// The action's command line ('' when not read); the old JSON's `arguments`.
    #[serde(alias = "arguments")]
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default)]
pub struct SchedulerFact {
    /// PowerShell answered at all (false off Windows or on a timeout).
    pub queried: bool,
    pub tasks: Vec<ScheduledTask>,
    /// What the jobs run: this `strategy-desk.exe` (the old JSON's `script`, the
    /// vault's `macro_schedule.ps1`). '' = there is nothing to register.
    #[serde(alias = "script")]
    pub exe: String,
}

/// Everything [`build_checks`] reads; desk-app gathers it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Facts {
    /// When the facts were gathered; `None` = [`REFERENCE_DATE`] for the planned jobs.
    pub now: Option<DateTime<Utc>>,
    /// The old JSON's `platform == "win32"`.
    #[serde(rename = "platform", deserialize_with = "win32")]
    pub windows: bool,
    pub uv: UvFact,
    pub runtime: RuntimeFact,
    pub clone: CloneFact,
    pub boot: BootFact,
    pub tools_python: ToolsPythonFact,
    pub claude: ClaudeFact,
    pub tv: TvFact,
    /// A port nobody answers on, so «switch the port» is a real offer.
    pub free_port: Option<u16>,
    pub vault: VaultFact,
    pub tv_session: TvSessionFact,
    pub scheduler: SchedulerFact,
    /// The PC's IANA zone (or the Windows name when that is all there is).
    pub timezone: String,
}

fn win32<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s == "win32",
        Value::Bool(b) => b,
        _ => false,
    })
}

/// The day the planned jobs are computed from when the facts carry no `now`.
pub const REFERENCE_DATE: (i32, u32, u32) = (2026, 1, 1);

/// Candidate ports offered when something else owns the configured one (`PORT_CANDIDATES`).
pub const PORT_CANDIDATES: [u16; 4] = [9223, 9224, 9225, 9226];

/// Zones under which the scheduled Kyiv minutes really are Kyiv's (`KYIV_ZONES`).
pub const KYIV_ZONES: [&str; 3] = ["Europe/Kyiv", "Europe/Kiev", "FLE Standard Time"];

pub fn is_kyiv_zone(tz: &str) -> bool {
    KYIV_ZONES.contains(&js::trim(tz))
}

const UV_WINDOWS: &str =
    "powershell -ExecutionPolicy ByPass -c \"irm https://astral.sh/uv/install.ps1 | iex\"";
const UV_UNIX: &str = "curl -LsSf https://astral.sh/uv/install.sh | sh";

// ---------------------------------------------------------------- helpers

struct C(Check);

fn check(id: CheckId, state: CheckState, key: &str) -> C {
    C(Check {
        id,
        state,
        detail_key: key.into(),
        vars: BTreeMap::new(),
        fix: None,
        fix_label_key: None,
        note_key: None,
    })
}

impl C {
    fn var(mut self, k: &str, v: impl ToString) -> C {
        self.0.vars.insert(k.into(), v.to_string());
        self
    }
    fn fix(mut self, fix: Fix, label: &str, note: Option<&str>) -> C {
        self.0.fix = Some(fix);
        self.0.fix_label_key = Some(label.into());
        self.0.note_key = note.map(Into::into);
        self
    }
    fn task(self, t: TaskId, label: &str, note: Option<&str>) -> C {
        self.fix(Fix::Task(t), label, note)
    }
    /// The old `explain` fix: no button, the note says what the owner does.
    fn explain(mut self, note: &str) -> C {
        self.0.note_key = Some(note.into());
        self
    }
    fn copy(self, text: String, label: &str, note: &str) -> C {
        self.fix(
            Fix::Copy {
                text,
                explain_key: note.into(),
            },
            label,
            Some(note),
        )
    }
    fn pick(self) -> C {
        self.fix(Fix::PickFolder, "preflight.fix.pickDir", None)
    }
    fn done(self) -> Check {
        self.0
    }
}

/// `firstLine`: the first non-blank line of an error, where the real cause is; at most 300 characters.
pub fn first_line(s: &str) -> String {
    let line = s
        .split('\n')
        .map(js::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("—");
    js::slice16(line, 300).to_owned()
}

/// `cdpOwner`: a short name for whoever answers on a CDP port — the first
/// titled target («wmux browser panel — claude.ai»), else its host, else the browser string.
pub fn cdp_owner(cdp: &CdpProbe) -> String {
    if !cdp.up {
        return String::new();
    }
    let target = cdp
        .targets
        .iter()
        .find(|t| !js::trim(&t.title).is_empty())
        .or(cdp.targets.first());
    let title = target
        .map(|t| js::slice16(js::trim(&t.title), 60).to_owned())
        .unwrap_or_default();
    let host = target
        .and_then(|t| url_host(js::trim(&t.url)))
        .unwrap_or_default();
    let joined = [title, host]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" — ");
    if !joined.is_empty() {
        joined
    } else if !cdp.browser.is_empty() {
        cdp.browser.clone()
    } else {
        "?".into()
    }
}

/// `/^[a-z-]+:\/\/([^/?#]+)/i`.
fn url_host(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || !scheme.bytes().all(|b| b.is_ascii_alphabetic() || b == b'-') {
        return None;
    }
    let host: String = rest
        .chars()
        .take_while(|c| !matches!(c, '/' | '?' | '#'))
        .collect();
    (!host.is_empty()).then_some(host)
}

fn severity(s: CheckState) -> u8 {
    match s {
        CheckState::Fail => 3,
        CheckState::Warn => 2,
        CheckState::Skipped => 1,
        CheckState::Ok => 0,
    }
}

/// `worstStatus`: fail, then warn, then not checked, then ok; empty = ok.
pub fn worst_state(checks: &[Check]) -> CheckState {
    checks
        .iter()
        .map(|c| c.state)
        .max_by_key(|s| severity(*s))
        .unwrap_or(CheckState::Ok)
}

/// `resultOf(...).ok`: no check failed (every check here is required; the old optional ones are gone).
pub fn all_ok(checks: &[Check]) -> bool {
    !checks.iter().any(|c| c.state == CheckState::Fail)
}

// ---------------------------------------------------------------- facts → checks

/// `buildChecks`, in the wizard's page order: the vault; Claude Code and its
/// login; where tvmcp comes from (`uv` only for an own checkout); TradingView
/// and its port; the sessionid; the schedule and the clock.
pub fn build_checks(settings: &Settings, f: &Facts) -> Vec<Check> {
    let mut out = Vec::with_capacity(10);
    out.push(vault_check(&f.vault));
    out.extend(claude_checks(&f.claude));
    if f.runtime.mode == RuntimeMode::External {
        out.push(uv_check(&f.uv, f.windows));
    }
    out.push(tvmcp_check(f));
    out.extend(tv_checks(&f.tv, f.free_port));
    out.push(tv_session_check(&f.tv_session));
    out.push(scheduler_check(settings, &f.scheduler, &f.vault, f.now));
    out.push(timezone_check(&f.timezone));
    out
}

fn uv_check(uv: &UvFact, windows: bool) -> Check {
    if uv.found {
        return check(CheckId::Uv, CheckState::Ok, "preflight.uv.ok")
            .var("version", &uv.version)
            .done();
    }
    let cmd = if windows { UV_WINDOWS } else { UV_UNIX };
    check(CheckId::Uv, CheckState::Fail, "preflight.uv.missing")
        .copy(cmd.into(), "preflight.fix.copyInstall", "preflight.uv.note")
        .done()
}

/// The old `tvmcpRuntime` / `tvmcpClone`, `tvmcpBoot` and `toolsPython` as one
/// row: the state and the detail of the worst of them (the first on a tie),
/// the fix of the first one, in that order, that has a fix — the source comes
/// first because a broken source is why the other two fail.
pub fn tvmcp_check(f: &Facts) -> Check {
    let source = match f.runtime.mode {
        RuntimeMode::Bundled => runtime_part(&f.runtime),
        RuntimeMode::External => clone_part(&f.clone),
    };
    let parts = [source, boot_part(f), tools_python_part(&f.tools_python)];
    let worst = parts.iter().map(|c| severity(c.state)).max().unwrap_or(0);
    if worst == 0 {
        let path = match f.runtime.mode {
            RuntimeMode::Bundled => &f.runtime.tvmcp_dir,
            RuntimeMode::External => &f.clone.path,
        };
        return check(CheckId::Tvmcp, CheckState::Ok, "preflight.tvmcp.ok")
            .var("tools", f.boot.tools)
            .var("python", &f.tools_python.python)
            .var("path", path)
            .done();
    }
    let fixed = parts.iter().find(|c| c.fix.is_some()).cloned();
    // `worst` is the severity of one of the parts, so the find always succeeds.
    let mut row = parts
        .into_iter()
        .find(|c| severity(c.state) == worst)
        .unwrap_or_else(|| {
            check(
                CheckId::Tvmcp,
                CheckState::Skipped,
                "preflight.boot.skipped",
            )
            .done()
        });
    if let Some(fx) = fixed {
        row.fix = fx.fix;
        row.fix_label_key = fx.fix_label_key;
        row.note_key = fx.note_key.or(row.note_key);
    }
    row
}

fn runtime_part(r: &RuntimeFact) -> Check {
    let id = CheckId::Tvmcp;
    if !r.payload_ok {
        // A damaged installation: no button can repair it, so it explains.
        return check(id, CheckState::Fail, "preflight.runtime.payloadBroken")
            .var("path", &r.tvmcp_dir)
            .var(
                "name",
                if r.project_name.is_empty() {
                    "—"
                } else {
                    &r.project_name
                },
            )
            .explain("preflight.runtime.payloadNote")
            .done();
    }
    if r.provisioned {
        return check(id, CheckState::Ok, "preflight.runtime.ok")
            .var("python", &r.python_path)
            .var("lock", &r.lock_hash)
            .done();
    }
    let (state, key) = if r.stale {
        (CheckState::Warn, "preflight.runtime.stale")
    } else {
        (CheckState::Fail, "preflight.runtime.missing")
    };
    check(id, state, key)
        .var("path", &r.tvmcp_dir)
        .task(
            TaskId::ProvisionRuntime,
            "preflight.fix.provision",
            Some("preflight.runtime.note"),
        )
        .done()
}

fn clone_part(c: &CloneFact) -> Check {
    let id = CheckId::Tvmcp;
    if js::trim(&c.path).is_empty() {
        return check(id, CheckState::Fail, "preflight.clone.unset")
            .pick()
            .done();
    }
    if !c.dir_exists {
        // Never cloned by the app: this is the command, the owner runs it.
        return check(id, CheckState::Fail, "preflight.clone.missing")
            .var("path", &c.path)
            .copy(
                format!("git clone {DEFAULT_TVMCP_REPO} \"{}\"", c.path),
                "preflight.fix.copyClone",
                "preflight.clone.note",
            )
            .done();
    }
    match c.pyproject {
        Pyproject::Missing => check(id, CheckState::Fail, "preflight.clone.noPyproject")
            .var("path", &c.path)
            .pick()
            .done(),
        Pyproject::WrongName => check(id, CheckState::Fail, "preflight.clone.wrongName")
            .var("path", &c.path)
            .var(
                "name",
                if c.project_name.is_empty() {
                    "—"
                } else {
                    &c.project_name
                },
            )
            .pick()
            .done(),
        Pyproject::Ok => check(id, CheckState::Ok, "preflight.clone.ok")
            .var("path", &c.path)
            .done(),
    }
}

fn boot_part(f: &Facts) -> Check {
    let id = CheckId::Tvmcp;
    let b = &f.boot;
    if !b.ran {
        return check(id, CheckState::Skipped, "preflight.boot.skipped").done();
    }
    if b.ok {
        return check(id, CheckState::Ok, "preflight.boot.ok")
            .var("tools", b.tools)
            .done();
    }
    let c = check(id, CheckState::Fail, "preflight.boot.failed").var("error", first_line(&b.error));
    match f.runtime.mode {
        // Re-provisioning is the app's own job; the owner's checkout is his to sync.
        RuntimeMode::Bundled => c.task(
            TaskId::ProvisionRuntime,
            "preflight.fix.provision",
            Some("preflight.boot.noteBundled"),
        ),
        RuntimeMode::External => c.copy(
            format!("uv --directory \"{}\" sync", f.clone.path),
            "preflight.fix.copySync",
            "preflight.boot.note",
        ),
    }
    .done()
}

fn tools_python_part(p: &ToolsPythonFact) -> Check {
    let id = CheckId::Tvmcp;
    if p.python.is_empty() {
        return check(id, CheckState::Fail, "preflight.toolsPython.none").done();
    }
    if !p.exists {
        return check(id, CheckState::Fail, "preflight.toolsPython.missing")
            .var("python", &p.python)
            .task(
                TaskId::ProvisionRuntime,
                "preflight.fix.provision",
                Some("preflight.runtime.note"),
            )
            .done();
    }
    if !p.checked {
        return check(id, CheckState::Skipped, "preflight.toolsPython.unchecked")
            .var("python", &p.python)
            .done();
    }
    if p.import_ok {
        return check(id, CheckState::Ok, "preflight.toolsPython.ok")
            .var("python", &p.python)
            .done();
    }
    check(id, CheckState::Fail, "preflight.toolsPython.importFailed")
        .var("python", &p.python)
        .var("error", first_line(&p.error))
        .task(
            TaskId::ProvisionRuntime,
            "preflight.fix.provision",
            Some("preflight.boot.noteBundled"),
        )
        .done()
}

/// Claude Code: installed (by its official installer, a task), then logged in
/// (`claude auth login`, a task). The login is not asked about before the install.
fn claude_checks(c: &ClaudeFact) -> [Check; 2] {
    if !c.found {
        return [
            check(
                CheckId::Claude,
                CheckState::Fail,
                "preflight.claude.missing",
            )
            .task(
                TaskId::ClaudeInstall,
                "preflight.fix.claudeInstall",
                Some("preflight.claude.installNote"),
            )
            .done(),
            check(
                CheckId::ClaudeAuth,
                CheckState::Skipped,
                "preflight.claude.authSkipped",
            )
            .done(),
        ];
    }
    let installed = check(CheckId::Claude, CheckState::Ok, "preflight.claude.found")
        .var("version", &c.version)
        .done();
    let auth = match c.auth {
        ClaudeAuth::None => check(
            CheckId::ClaudeAuth,
            CheckState::Fail,
            "preflight.claude.none",
        )
        .task(
            TaskId::ClaudeLogin,
            "preflight.fix.claudeLogin",
            Some("preflight.claude.note"),
        )
        .done(),
        a => check(
            CheckId::ClaudeAuth,
            CheckState::Ok,
            &format!("preflight.claude.{}", a.key()),
        )
        .done(),
    };
    [installed, auth]
}

/// TradingView Desktop and who owns its debug port. TradingView is a
/// single-instance app: a copy started without the flag cannot be fixed by
/// launching again, and the app never closes it — it explains.
fn tv_checks(tv: &TvFact, free_port: Option<u16>) -> [Check; 2] {
    let cdp = &tv.cdp;
    let port = tv.port;
    let tradingview = if cdp.up && cdp.tradingview {
        if cdp.chart_tab {
            check(CheckId::Tradingview, CheckState::Ok, "preflight.tv.ok")
                .var("port", port)
                .done()
        } else {
            check(
                CheckId::Tradingview,
                CheckState::Warn,
                "preflight.tv.noChart",
            )
            .explain("preflight.tv.noChartNote")
            .done()
        }
    } else if tv.running && !cdp.up {
        check(
            CheckId::Tradingview,
            CheckState::Fail,
            "preflight.tv.flagless",
        )
        .var("port", port)
        .explain("preflight.tv.flaglessNote")
        .done()
    } else if tv.running {
        check(
            CheckId::Tradingview,
            CheckState::Fail,
            "preflight.tv.otherOwner",
        )
        .var("port", port)
        .var("owner", cdp_owner(cdp))
        .explain("preflight.tv.flaglessNote")
        .done()
    } else {
        check(
            CheckId::Tradingview,
            CheckState::Fail,
            "preflight.tv.notRunning",
        )
        .fix(Fix::LaunchTradingView, "preflight.fix.launchTv", None)
        .done()
    };
    // A 200 from /json/version proves nothing: any CDP-speaking app can own the port.
    let cdp_check = if !cdp.up {
        check(CheckId::Cdp, CheckState::Ok, "preflight.port.free")
            .var("port", port)
            .done()
    } else if cdp.tradingview {
        check(CheckId::Cdp, CheckState::Ok, "preflight.port.tv")
            .var("port", port)
            .done()
    } else {
        let c = check(CheckId::Cdp, CheckState::Fail, "preflight.port.conflict")
            .var("port", port)
            .var("owner", cdp_owner(cdp))
            .var(
                "browser",
                if cdp.browser.is_empty() {
                    "—"
                } else {
                    &cdp.browser
                },
            );
        match free_port {
            Some(p) => c.fix(Fix::SetCdpPort(p), "preflight.fix.setPort", None),
            None => c.explain("preflight.port.noFreeNote"),
        }
        .done()
    };
    [tradingview, cdp_check]
}

/// `vaultCheck`: nothing there → create from the template; a vault missing
/// some of its files → complete it (missing files only, never state, journal
/// or config); a folder full of something else → pick another one.
pub fn vault_check(v: &VaultFact) -> Check {
    let id = CheckId::Vault;
    if js::trim(&v.path).is_empty() {
        return check(id, CheckState::Fail, "preflight.vault.unset")
            .pick()
            .done();
    }
    if v.has_skill && v.has_scale_tool && v.has_scale_config {
        let key = if v.template_version.is_empty() {
            "preflight.vault.ok"
        } else {
            "preflight.vault.okTemplate"
        };
        return check(id, CheckState::Ok, key)
            .var("path", &v.path)
            .var("version", &v.template_version)
            .done();
    }
    if !v.exists || !v.non_empty {
        let c = check(
            id,
            CheckState::Fail,
            if v.exists {
                "preflight.vault.empty"
            } else {
                "preflight.vault.missing"
            },
        )
        .var("path", &v.path);
        return if v.template_available {
            c.task(
                TaskId::VaultCreate,
                "preflight.fix.vaultCreate",
                Some("preflight.vault.createNote"),
            )
        } else {
            c.explain("preflight.vault.noTemplateNote")
        }
        .done();
    }
    if v.has_skill || v.has_scale_tool || v.has_scale_config {
        let c = check(id, CheckState::Fail, "preflight.vault.incomplete").var("path", &v.path);
        return if v.template_available {
            c.task(
                TaskId::VaultCreate,
                "preflight.fix.vaultComplete",
                Some("preflight.vault.completeNote"),
            )
        } else {
            c.pick()
        }
        .done();
    }
    check(id, CheckState::Fail, "preflight.vault.notVault")
        .var("path", &v.path)
        .pick()
        .done()
}

/// `tvSessionCheck`: the cookie is the owner's own, so «not set» explains where
/// to copy it from; «set» offers the one cheap proof (a token for it).
pub fn tv_session_check(s: &TvSessionFact) -> Check {
    let id = CheckId::TvSession;
    if !s.set {
        return check(id, CheckState::Fail, "preflight.tvSession.none")
            .explain("preflight.tvSession.howNote")
            .done();
    }
    match s.verified_at {
        Some(t) => check(id, CheckState::Ok, "preflight.tvSession.ok")
            .var("time", kyiv_stamp(t))
            .done(),
        None => check(id, CheckState::Warn, "preflight.tvSession.unverified")
            .task(
                TaskId::SessionCheck,
                "preflight.fix.sessionCheck",
                Some("preflight.tvSession.checkNote"),
            )
            .done(),
    }
}

/// A 0.2.1 registration whose path points into app.asar (issue #12).
fn in_asar(command: &str) -> bool {
    let c = command.to_lowercase();
    c.contains("\\app.asar\\") || c.contains("/app.asar/")
}

/// The Task Scheduler jobs: this desk's ([`crate::schedule::triggers`],
/// «Strategy Desk …»), or — until they are replaced — the old app's three
/// («Trading Macro EU / US», «Trading Review», which run the vault's script
/// and work on their own). The same states as the old check: all there (ok,
/// unless disabled or stale), some there, none; registering replaces either
/// set. With both sets present the skill would run twice per window: that is
/// a warning, and registering (which removes the old ones) is the fix.
pub fn scheduler_check(
    settings: &Settings,
    s: &SchedulerFact,
    v: &VaultFact,
    now: Option<DateTime<Utc>>,
) -> Check {
    let id = CheckId::Scheduler;
    if !s.queried {
        return check(id, CheckState::Skipped, "preflight.scheduler.unknown").done();
    }
    let date = now
        .map(|t| crate::windows::kyiv_now(t).date)
        .unwrap_or_else(|| {
            let (y, m, d) = REFERENCE_DATE;
            NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
        });
    let can_register = !js::trim(&s.exe).is_empty() && v.has_scale_tool;
    let register = |c: C| {
        if can_register {
            c.task(
                TaskId::SchedulerRegister,
                "preflight.fix.schedulerRegister",
                Some("preflight.scheduler.registerNote"),
            )
        } else {
            c
        }
    };
    let present = |name: &str| s.tasks.iter().find(|t| t.name == name);
    let ours: Vec<&ScheduledTask> = s
        .tasks
        .iter()
        .filter(|t| t.name.starts_with(schedule::TASK_PREFIX))
        .collect();
    let legacy: Vec<&ScheduledTask> = schedule::LEGACY_TASKS
        .iter()
        .filter_map(|n| present(n))
        .collect();
    let list = |v: &[&ScheduledTask]| {
        v.iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    if !ours.is_empty() && !legacy.is_empty() {
        return register(
            check(id, CheckState::Warn, "preflight.scheduler.legacy").var("list", list(&legacy)),
        )
        .done();
    }
    // The set the PC has: ours when there are any, else the old app's.
    let mine = !ours.is_empty();
    let wanted: Vec<String> = if mine {
        schedule::triggers(date, settings)
            .iter()
            .map(schedule::task_name)
            .collect()
    } else {
        schedule::LEGACY_TASKS
            .iter()
            .map(|n| (*n).to_owned())
            .collect()
    };
    let exe = js::trim(&s.exe).to_lowercase();
    // Ours run another copy of the exe (a moved install); the old ones point into app.asar (0.2.1, issue #12).
    let stale_of = |t: &ScheduledTask| {
        if mine {
            !exe.is_empty() && !t.command.is_empty() && !t.command.to_lowercase().contains(&exe)
        } else {
            in_asar(&t.command)
        }
    };
    let found: Vec<&ScheduledTask> = wanted.iter().filter_map(|w| present(w)).collect();
    // Ours for minutes no longer planned (a window switched off): one too many.
    let extra = ours.iter().any(|t| !wanted.contains(&t.name));
    if found.len() == wanted.len() && !extra {
        let disabled: Vec<&ScheduledTask> = found
            .iter()
            .copied()
            .filter(|t| t.state.to_lowercase().contains("disabled"))
            .collect();
        if !disabled.is_empty() {
            return register(
                check(id, CheckState::Warn, "preflight.scheduler.disabled")
                    .var("list", list(&disabled)),
            )
            .done();
        }
        let stale: Vec<&ScheduledTask> = found.iter().copied().filter(|t| stale_of(t)).collect();
        if !stale.is_empty() {
            return register(
                check(id, CheckState::Warn, "preflight.scheduler.stale").var("list", list(&stale)),
            )
            .done();
        }
        return check(id, CheckState::Ok, "preflight.scheduler.ok")
            .var("count", wanted.len())
            .done();
    }
    if !found.is_empty() || extra {
        let mut all = found;
        all.extend(ours.iter().copied().filter(|t| !wanted.contains(&t.name)));
        return register(
            check(id, CheckState::Warn, "preflight.scheduler.partial").var("found", list(&all)),
        )
        .done();
    }
    if can_register {
        register(check(id, CheckState::Warn, "preflight.scheduler.none")).done()
    } else {
        check(id, CheckState::Warn, "preflight.scheduler.noVault").done()
    }
}

/// `timezoneCheck`: the jobs fire at the PC's local minutes, which are the
/// planned Kyiv minutes only on a Kyiv clock.
pub fn timezone_check(tz: &str) -> Check {
    let id = CheckId::Timezone;
    if js::trim(tz).is_empty() {
        return check(id, CheckState::Skipped, "preflight.timezone.unknown").done();
    }
    if is_kyiv_zone(tz) {
        return check(id, CheckState::Ok, "preflight.timezone.ok")
            .var("tz", tz)
            .done();
    }
    check(id, CheckState::Warn, "preflight.timezone.other")
        .var("tz", tz)
        .explain("preflight.timezone.note")
        .done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ScheduleSettings;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-27T07:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn tv_cdp() -> CdpProbe {
        CdpProbe {
            up: true,
            browser: "Chrome/128.0.6613.36".into(),
            chart_tab: true,
            tradingview: true,
            targets: vec![CdpTarget {
                url: "https://www.tradingview.com/chart/abc/".into(),
                title: "US500 · 15 · PEPPERSTONE".into(),
            }],
        }
    }

    fn healthy_vault() -> VaultFact {
        VaultFact {
            path: "C:\\Users\\T\\Documents\\Strategy Desk".into(),
            exists: true,
            non_empty: true,
            has_skill: true,
            has_scale_tool: true,
            has_scale_config: true,
            template_version: "2026-10-12".into(),
            template_available: true,
        }
    }

    const EXE: &str = "C:\\Users\\T\\AppData\\Local\\Programs\\Strategy Desk\\strategy-desk.exe";

    fn on() -> Settings {
        Settings {
            schedule: ScheduleSettings {
                enabled: true,
                ..ScheduleSettings::default()
            },
            ..Settings::default()
        }
    }

    fn registered(settings: &Settings) -> SchedulerFact {
        let date = crate::windows::kyiv_now(now()).date;
        SchedulerFact {
            queried: true,
            tasks: schedule::triggers(date, settings)
                .iter()
                .map(|r| ScheduledTask {
                    name: schedule::task_name(r),
                    state: "Ready".into(),
                    command: format!("\"{EXE}\" run --window {}", r.window.as_str()),
                })
                .collect(),
            exe: EXE.into(),
        }
    }

    fn legacy_jobs() -> SchedulerFact {
        SchedulerFact {
            queried: true,
            tasks: schedule::LEGACY_TASKS
                .iter()
                .map(|n| ScheduledTask {
                    name: (*n).into(),
                    state: "Ready".into(),
                    command: "-File x.ps1".into(),
                })
                .collect(),
            exe: EXE.into(),
        }
    }

    /// A packaged install where everything is in place.
    fn healthy() -> Facts {
        Facts {
            now: Some(now()),
            windows: true,
            uv: UvFact {
                found: true,
                version: "0.9.7".into(),
            },
            runtime: RuntimeFact {
                mode: RuntimeMode::Bundled,
                tvmcp_dir: "C:\\app\\resources\\tvmcp".into(),
                payload_ok: true,
                project_name: "tvmcp".into(),
                provisioned: true,
                stale: false,
                python_path: "C:\\userData\\py\\venv\\Scripts\\python.exe".into(),
                lock_hash: "abc123".into(),
            },
            clone: CloneFact {
                path: "C:\\tvmcp".into(),
                dir_exists: true,
                pyproject: Pyproject::Ok,
                project_name: "tvmcp".into(),
            },
            boot: BootFact {
                ran: true,
                ok: true,
                tools: 59,
                error: String::new(),
            },
            tools_python: ToolsPythonFact {
                python: "C:\\userData\\py\\venv\\Scripts\\python.exe".into(),
                exists: true,
                checked: true,
                import_ok: true,
                error: String::new(),
            },
            claude: ClaudeFact {
                found: true,
                version: "2.1.0".into(),
                auth: ClaudeAuth::Subscription,
            },
            tv: TvFact {
                running: true,
                port: 9223,
                cdp: tv_cdp(),
            },
            free_port: None,
            vault: healthy_vault(),
            tv_session: TvSessionFact {
                set: true,
                verified_at: Some(now()),
            },
            scheduler: registered(&on()),
            timezone: "Europe/Kyiv".into(),
        }
    }

    fn get(checks: &[Check], id: CheckId) -> &Check {
        checks
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no check {id:?}"))
    }

    // ---- the list (ported from preflight.test.ts)

    #[test]
    fn one_order_the_wizards_with_uv_only_for_an_own_checkout() {
        let ids = |f: &Facts| {
            build_checks(&on(), f)
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>()
        };
        let without_uv: Vec<CheckId> = CheckId::ALL
            .into_iter()
            .filter(|c| *c != CheckId::Uv)
            .collect();
        assert_eq!(ids(&healthy()), without_uv);
        let mut ext = healthy();
        ext.runtime.mode = RuntimeMode::External;
        assert_eq!(ids(&ext), CheckId::ALL);
        // The wizard's pages list the checks in the same order.
        let pages: Vec<CheckId> = crate::wizard::StepId::ALL
            .iter()
            .flat_map(|s| s.checks().iter().copied())
            .collect();
        assert_eq!(pages, CheckId::ALL);
    }

    #[test]
    fn is_all_green_on_a_healthy_machine() {
        let checks = build_checks(&on(), &healthy());
        for c in &checks {
            assert_eq!(c.state, CheckState::Ok, "{c:?}");
        }
        assert!(all_ok(&checks));
        assert_eq!(get(&checks, CheckId::Tvmcp).vars["tools"], "59");
    }

    #[test]
    fn every_failing_check_has_a_fix_or_an_explanation() {
        let mut f = healthy();
        f.runtime.mode = RuntimeMode::External;
        f.uv.found = false;
        f.claude.auth = ClaudeAuth::None;
        f.tv = TvFact {
            running: true,
            port: 9223,
            cdp: CdpProbe::default(),
        };
        f.vault.path = String::new();
        f.tv_session = TvSessionFact::default();
        f.scheduler.tasks.clear();
        f.timezone = "Europe/Berlin".into();
        for c in build_checks(&on(), &f) {
            if matches!(c.state, CheckState::Ok | CheckState::Skipped) {
                continue;
            }
            match &c.fix {
                Some(_) => assert!(
                    c.fix_label_key
                        .as_deref()
                        .is_some_and(|k| k.starts_with("preflight.fix.")),
                    "{:?}",
                    c.id
                ),
                None => assert!(
                    c.note_key.is_some(),
                    "{:?} has neither a fix nor a note",
                    c.id
                ),
            }
        }
    }

    #[test]
    fn uv_found_is_ok_missing_offers_the_install_line_for_the_platform() {
        let mut f = healthy();
        f.runtime.mode = RuntimeMode::External;
        assert_eq!(
            get(&build_checks(&on(), &f), CheckId::Uv).state,
            CheckState::Ok
        );
        f.uv.found = false;
        let uv = get(&build_checks(&on(), &f), CheckId::Uv).clone();
        assert_eq!(uv.state, CheckState::Fail);
        assert!(matches!(&uv.fix, Some(Fix::Copy { text, .. }) if text.contains("install.ps1")));
        f.windows = false;
        let mac = get(&build_checks(&on(), &f), CheckId::Uv).clone();
        assert!(matches!(&mac.fix, Some(Fix::Copy { text, .. }) if text.contains("install.sh")));
    }

    #[test]
    fn the_checkout_fails_differently_for_each_problem_and_is_never_cloned_by_the_app() {
        let cases = [
            (
                CloneFact {
                    path: String::new(),
                    ..healthy().clone
                },
                "preflight.clone.unset",
            ),
            (
                CloneFact {
                    dir_exists: false,
                    ..healthy().clone
                },
                "preflight.clone.missing",
            ),
            (
                CloneFact {
                    pyproject: Pyproject::Missing,
                    ..healthy().clone
                },
                "preflight.clone.noPyproject",
            ),
            (
                CloneFact {
                    pyproject: Pyproject::WrongName,
                    project_name: "something-else".into(),
                    ..healthy().clone
                },
                "preflight.clone.wrongName",
            ),
        ];
        for (clone, key) in cases {
            let mut f = healthy();
            f.runtime.mode = RuntimeMode::External;
            f.clone = clone;
            let c = get(&build_checks(&on(), &f), CheckId::Tvmcp).clone();
            assert_eq!((c.state, c.detail_key.as_str()), (CheckState::Fail, key));
        }
        let mut f = healthy();
        f.runtime.mode = RuntimeMode::External;
        f.clone = CloneFact {
            path: "C:\\Projects\\tvmcp".into(),
            dir_exists: false,
            ..CloneFact::default()
        };
        let c = get(&build_checks(&on(), &f), CheckId::Tvmcp).clone();
        assert_eq!(
            c.fix,
            Some(Fix::Copy { text: "git clone https://github.com/Miha21222/tradingview-mcp.git \"C:\\Projects\\tvmcp\"".into(), explain_key: "preflight.clone.note".into() })
        );
        assert_eq!(c.fix_label_key.as_deref(), Some("preflight.fix.copyClone"));
    }

    #[test]
    fn boot_not_attempted_is_unchecked_and_a_failure_shows_the_first_line() {
        let mut f = healthy();
        f.boot = BootFact {
            ran: false,
            ..BootFact::default()
        };
        let c = tvmcp_check(&f);
        assert_eq!(
            (c.state, c.detail_key.as_str()),
            (CheckState::Skipped, "preflight.boot.skipped")
        );
        f.boot = BootFact {
            ran: true,
            ok: false,
            tools: 0,
            error: "\n  ModuleNotFoundError: No module named 'httpx'\n  at line 3\n".into(),
        };
        let c = tvmcp_check(&f);
        assert_eq!(c.state, CheckState::Fail);
        assert_eq!(
            c.vars["error"],
            "ModuleNotFoundError: No module named 'httpx'"
        );
        // Bundled: the provisioning button, not a uv command the owner has no uv for.
        assert_eq!(c.fix, Some(Fix::Task(TaskId::ProvisionRuntime)));
        f.runtime.mode = RuntimeMode::External;
        let c = tvmcp_check(&f);
        assert!(
            matches!(&c.fix, Some(Fix::Copy { text, .. }) if text == "uv --directory \"C:\\tvmcp\" sync")
        );
    }

    #[test]
    fn the_bundled_runtime_provisions_warns_when_stale_and_explains_a_broken_payload() {
        let mut f = healthy();
        f.runtime.provisioned = false;
        f.boot.ran = false;
        let c = tvmcp_check(&f);
        assert_eq!(
            (c.state, c.detail_key.as_str()),
            (CheckState::Fail, "preflight.runtime.missing")
        );
        assert_eq!(c.fix, Some(Fix::Task(TaskId::ProvisionRuntime)));
        f.runtime.stale = true;
        f.boot.ran = true;
        let c = tvmcp_check(&f);
        assert_eq!(
            (c.state, c.detail_key.as_str()),
            (CheckState::Warn, "preflight.runtime.stale")
        );
        f.runtime.payload_ok = false;
        let c = tvmcp_check(&f);
        assert_eq!(
            (c.state, c.fix.clone(), c.note_key.as_deref()),
            (
                CheckState::Fail,
                None,
                Some("preflight.runtime.payloadNote")
            )
        );
    }

    #[test]
    fn the_folded_row_takes_the_worst_state_and_the_first_fix() {
        // Both the runtime and the Python fail: the runtime's row (first on a tie).
        let mut f = healthy();
        f.runtime.provisioned = false;
        f.tools_python.exists = false;
        assert_eq!(tvmcp_check(&f).detail_key, "preflight.runtime.missing");
        // The runtime only warns, the Python fails: the Python's detail, the runtime's fix (first with one).
        let mut g = healthy();
        g.runtime.stale = true;
        g.runtime.provisioned = false;
        g.tools_python.import_ok = false;
        g.tools_python.error = "ModuleNotFoundError: No module named 'tvmcp'\n".into();
        let c = tvmcp_check(&g);
        assert_eq!(
            (c.state, c.detail_key.as_str()),
            (CheckState::Fail, "preflight.toolsPython.importFailed")
        );
        assert_eq!(
            c.vars["error"],
            "ModuleNotFoundError: No module named 'tvmcp'"
        );
        assert_eq!(
            (c.fix, c.note_key.as_deref()),
            (
                Some(Fix::Task(TaskId::ProvisionRuntime)),
                Some("preflight.runtime.note")
            )
        );
        // No interpreter at all and nothing else has a fix: no fix.
        let mut h = healthy();
        h.tools_python = ToolsPythonFact::default();
        let c = tvmcp_check(&h);
        assert_eq!((c.state, c.fix), (CheckState::Fail, None));
    }

    #[test]
    fn claude_install_then_login_each_a_task() {
        for (auth, key) in [
            (ClaudeAuth::Subscription, "subscription"),
            (ClaudeAuth::ApiKey, "api-key"),
            (ClaudeAuth::SetupToken, "setup-token"),
        ] {
            let [i, a] = claude_checks(&ClaudeFact {
                found: true,
                version: "2.1.0".into(),
                auth,
            });
            assert_eq!(i.state, CheckState::Ok);
            assert_eq!(
                (a.state, a.detail_key),
                (CheckState::Ok, format!("preflight.claude.{key}"))
            );
        }
        let [_, a] = claude_checks(&ClaudeFact {
            found: true,
            version: "2.1.0".into(),
            auth: ClaudeAuth::None,
        });
        assert_eq!(
            (a.state, a.fix),
            (CheckState::Fail, Some(Fix::Task(TaskId::ClaudeLogin)))
        );
        assert_eq!(a.note_key.as_deref(), Some("preflight.claude.note"));
        let [i, a] = claude_checks(&ClaudeFact::default());
        assert_eq!(
            (i.state, i.fix),
            (CheckState::Fail, Some(Fix::Task(TaskId::ClaudeInstall)))
        );
        assert_eq!(
            a.state,
            CheckState::Skipped,
            "no login question before the install"
        );
        assert!(TaskId::ClaudeLogin.wants_stdin() && !TaskId::ClaudeInstall.wants_stdin());
    }

    // ---- TradingView and the port

    fn foreign() -> CdpProbe {
        CdpProbe {
            up: true,
            browser: "Chrome/140.0.7259.5".into(),
            chart_tab: false,
            tradingview: false,
            targets: vec![CdpTarget {
                url: "https://claude.ai/code".into(),
                title: "wmux browser panel".into(),
            }],
        }
    }

    #[test]
    fn tradingview_not_running_launches_flagless_explains_never_kills() {
        let [c, _] = tv_checks(
            &TvFact {
                running: false,
                port: 9223,
                cdp: CdpProbe::default(),
            },
            None,
        );
        assert_eq!(
            (c.state, c.fix),
            (CheckState::Fail, Some(Fix::LaunchTradingView))
        );
        let [c, _] = tv_checks(
            &TvFact {
                running: true,
                port: 9223,
                cdp: CdpProbe::default(),
            },
            None,
        );
        assert_eq!(
            (c.detail_key.as_str(), c.fix, c.note_key.as_deref()),
            (
                "preflight.tv.flagless",
                None,
                Some("preflight.tv.flaglessNote")
            )
        );
        let [c, _] = tv_checks(
            &TvFact {
                running: true,
                port: 9223,
                cdp: CdpProbe {
                    chart_tab: false,
                    ..tv_cdp()
                },
            },
            None,
        );
        assert_eq!(c.state, CheckState::Warn);
    }

    #[test]
    fn a_foreign_port_owner_is_named_and_a_free_port_offered() {
        let [tvc, p] = tv_checks(
            &TvFact {
                running: false,
                port: 9222,
                cdp: foreign(),
            },
            Some(9223),
        );
        assert_eq!(tvc.state, CheckState::Fail);
        assert_eq!(
            (p.state, p.detail_key.as_str()),
            (CheckState::Fail, "preflight.port.conflict")
        );
        assert_eq!(p.vars["owner"], "wmux browser panel — claude.ai");
        assert_eq!(p.vars["browser"], "Chrome/140.0.7259.5");
        assert_eq!(p.fix, Some(Fix::SetCdpPort(9223)));
        let [_, p] = tv_checks(
            &TvFact {
                running: false,
                port: 9222,
                cdp: foreign(),
            },
            None,
        );
        assert_eq!(
            (p.fix, p.note_key.as_deref()),
            (None, Some("preflight.port.noFreeNote"))
        );
        let [tvc, _] = tv_checks(
            &TvFact {
                running: true,
                port: 9222,
                cdp: foreign(),
            },
            None,
        );
        assert_eq!(tvc.detail_key, "preflight.tv.otherOwner");
        assert!(tvc.vars["owner"].contains("wmux"));
        let [_, free] = tv_checks(
            &TvFact {
                running: false,
                port: 9223,
                cdp: CdpProbe::default(),
            },
            None,
        );
        assert_eq!(free.state, CheckState::Ok);
    }

    #[test]
    fn cdp_owner_degrades_gracefully() {
        assert_eq!(cdp_owner(&CdpProbe::default()), "");
        let with = |url: &str, title: &str| CdpProbe {
            targets: vec![CdpTarget {
                url: url.into(),
                title: title.into(),
            }],
            ..foreign()
        };
        assert_eq!(cdp_owner(&with("", "Some App")), "Some App");
        assert_eq!(
            cdp_owner(&with("http://localhost:3000/x", "")),
            "localhost:3000"
        );
        assert_eq!(
            cdp_owner(&CdpProbe {
                targets: vec![],
                ..foreign()
            }),
            "Chrome/140.0.7259.5"
        );
    }

    #[test]
    fn worst_state_fail_first() {
        let checks = build_checks(&on(), &healthy());
        assert_eq!(worst_state(&checks), CheckState::Ok);
        let mut w = checks.clone();
        w.push(check(CheckId::Uv, CheckState::Warn, "x").done());
        assert_eq!(worst_state(&w), CheckState::Warn);
        w.push(check(CheckId::Uv, CheckState::Skipped, "x").done());
        assert_eq!(worst_state(&w), CheckState::Warn);
        w.push(check(CheckId::Uv, CheckState::Fail, "y").done());
        assert_eq!(worst_state(&w), CheckState::Fail);
        assert_eq!(worst_state(&[]), CheckState::Ok);
    }

    // ---- the vault, the session, the schedule, the clock

    #[test]
    fn vault_create_complete_or_pick_never_write_into_a_stranger() {
        let bare = VaultFact {
            has_skill: false,
            has_scale_tool: false,
            has_scale_config: false,
            template_version: String::new(),
            ..healthy_vault()
        };
        let missing = vault_check(&VaultFact {
            exists: false,
            non_empty: false,
            ..bare.clone()
        });
        assert_eq!(
            (missing.state, missing.fix),
            (CheckState::Fail, Some(Fix::Task(TaskId::VaultCreate)))
        );
        let empty = vault_check(&VaultFact {
            non_empty: false,
            ..bare.clone()
        });
        assert_eq!(empty.detail_key, "preflight.vault.empty");
        assert_eq!(
            empty.fix_label_key.as_deref(),
            Some("preflight.fix.vaultCreate")
        );
        let stranger = vault_check(&bare);
        assert_eq!(
            (stranger.detail_key.as_str(), stranger.fix),
            ("preflight.vault.notVault", Some(Fix::PickFolder))
        );
        let partial = vault_check(&VaultFact {
            has_skill: false,
            ..healthy_vault()
        });
        assert_eq!(
            (partial.detail_key.as_str(), partial.fix.clone()),
            (
                "preflight.vault.incomplete",
                Some(Fix::Task(TaskId::VaultCreate))
            )
        );
        assert_eq!(
            partial.fix_label_key.as_deref(),
            Some("preflight.fix.vaultComplete")
        );
        let no_template = vault_check(&VaultFact {
            exists: false,
            non_empty: false,
            template_available: false,
            ..bare
        });
        assert_eq!(
            (no_template.fix, no_template.note_key.as_deref()),
            (None, Some("preflight.vault.noTemplateNote"))
        );
        let ok = vault_check(&healthy_vault());
        assert_eq!(
            (
                ok.state,
                ok.detail_key.as_str(),
                ok.vars["version"].as_str()
            ),
            (CheckState::Ok, "preflight.vault.okTemplate", "2026-10-12")
        );
        assert_eq!(
            vault_check(&VaultFact {
                template_version: String::new(),
                ..healthy_vault()
            })
            .detail_key,
            "preflight.vault.ok"
        );
        assert_eq!(
            vault_check(&VaultFact {
                path: " ".into(),
                ..healthy_vault()
            })
            .fix,
            Some(Fix::PickFolder)
        );
    }

    #[test]
    fn session_explains_then_offers_the_cheap_check_then_is_green() {
        let none = tv_session_check(&TvSessionFact::default());
        assert_eq!(
            (none.state, none.fix, none.note_key.as_deref()),
            (CheckState::Fail, None, Some("preflight.tvSession.howNote"))
        );
        let unverified = tv_session_check(&TvSessionFact {
            set: true,
            verified_at: None,
        });
        assert_eq!(
            (unverified.state, unverified.fix),
            (CheckState::Warn, Some(Fix::Task(TaskId::SessionCheck)))
        );
        let ok = tv_session_check(&TvSessionFact {
            set: true,
            verified_at: Some(now()),
        });
        assert_eq!(
            (ok.state, ok.vars["time"].as_str()),
            (CheckState::Ok, "2026-11-27 09:00")
        );
    }

    #[test]
    fn scheduler_green_only_with_every_planned_job() {
        let s = on();
        let v = healthy_vault();
        let full = registered(&s);
        let n = Some(now());
        let ok = scheduler_check(&s, &full, &v, n);
        assert_eq!((ok.state, ok.vars["count"].as_str()), (CheckState::Ok, "4"));
        let some = SchedulerFact {
            tasks: full.tasks[..2].to_vec(),
            ..full.clone()
        };
        let c = scheduler_check(&s, &some, &v, n);
        assert_eq!(
            (c.state, c.detail_key.as_str(), c.fix.clone()),
            (
                CheckState::Warn,
                "preflight.scheduler.partial",
                Some(Fix::Task(TaskId::SchedulerRegister))
            )
        );
        assert_eq!(
            c.vars["found"],
            "Strategy Desk EU 0930, Strategy Desk US 1500"
        );
        let none = scheduler_check(
            &s,
            &SchedulerFact {
                tasks: vec![],
                ..full.clone()
            },
            &v,
            n,
        );
        assert_eq!(
            (none.detail_key.as_str(), none.fix),
            (
                "preflight.scheduler.none",
                Some(Fix::Task(TaskId::SchedulerRegister))
            )
        );
        let no_vault = scheduler_check(
            &s,
            &SchedulerFact {
                tasks: vec![],
                ..full.clone()
            },
            &VaultFact::default(),
            n,
        );
        assert_eq!(
            (no_vault.detail_key.as_str(), no_vault.fix),
            ("preflight.scheduler.noVault", None)
        );
        let nothing_to_run = scheduler_check(
            &s,
            &SchedulerFact {
                tasks: vec![],
                exe: String::new(),
                ..full.clone()
            },
            &v,
            n,
        );
        assert_eq!(nothing_to_run.detail_key, "preflight.scheduler.noVault");
        assert_eq!(
            scheduler_check(&s, &SchedulerFact::default(), &v, n).state,
            CheckState::Skipped
        );
        // A window switched off: its job is now one too many → re-register.
        let no_us = Settings {
            schedule: ScheduleSettings {
                us: false,
                ..s.schedule
            },
            ..s.clone()
        };
        let c = scheduler_check(&no_us, &full, &v, n);
        assert_eq!(
            (c.detail_key.as_str(), c.fix),
            (
                "preflight.scheduler.partial",
                Some(Fix::Task(TaskId::SchedulerRegister))
            )
        );
    }

    #[test]
    fn scheduler_disabled_stale_and_the_old_apps_jobs() {
        let s = on();
        let v = healthy_vault();
        let n = Some(now());
        let full = registered(&s);
        let mut dis = full.clone();
        dis.tasks[3].state = "Disabled".into();
        let c = scheduler_check(&s, &dis, &v, n);
        assert_eq!(
            (c.detail_key.as_str(), c.vars["list"].as_str()),
            ("preflight.scheduler.disabled", "Strategy Desk Review 2000")
        );
        // A job that runs another copy of the exe (moved install) is stale.
        let mut moved = full.clone();
        moved.tasks[1].command = "\"D:\\old\\strategy-desk.exe\" run --window us".into();
        let c = scheduler_check(&s, &moved, &v, n);
        assert_eq!(
            (c.detail_key.as_str(), c.vars["list"].as_str()),
            ("preflight.scheduler.stale", "Strategy Desk US 1500")
        );
        assert_eq!(c.fix, Some(Fix::Task(TaskId::SchedulerRegister)));
        // The old app's three jobs alone are a working schedule (they run the vault's script).
        assert_eq!(
            scheduler_check(&s, &legacy_jobs(), &v, n).state,
            CheckState::Ok
        );
        let mut asar = legacy_jobs();
        asar.tasks[1].command = "-Claude \"C:\\Program Files\\Strategy Desk\\resources\\app.asar\\node_modules\\x\\claude.exe\"".into();
        let c = scheduler_check(&s, &asar, &v, n);
        assert_eq!(
            (c.detail_key.as_str(), c.vars["list"].as_str()),
            ("preflight.scheduler.stale", "Trading Macro US")
        );
        let mut old_partial = legacy_jobs();
        old_partial.tasks.remove(1);
        assert_eq!(
            scheduler_check(&s, &old_partial, &v, n).detail_key,
            "preflight.scheduler.partial"
        );
        // Both sets: the skill would run twice; registering removes the old ones.
        let mut both = full.clone();
        both.tasks.push(ScheduledTask {
            name: "Trading Macro US".into(),
            state: "Ready".into(),
            command: String::new(),
        });
        let c = scheduler_check(&s, &both, &v, n);
        assert_eq!(
            (c.detail_key.as_str(), c.vars["list"].as_str()),
            ("preflight.scheduler.legacy", "Trading Macro US")
        );
        assert_eq!(c.fix, Some(Fix::Task(TaskId::SchedulerRegister)));
    }

    #[test]
    fn timezone_kyiv_under_three_names() {
        for tz in KYIV_ZONES {
            assert_eq!(timezone_check(tz).state, CheckState::Ok);
        }
        let c = timezone_check("Europe/Berlin");
        assert_eq!(
            (c.state, c.vars["tz"].as_str(), c.fix),
            (CheckState::Warn, "Europe/Berlin", None)
        );
        assert_eq!(c.note_key.as_deref(), Some("preflight.timezone.note"));
        assert_eq!(timezone_check("").state, CheckState::Skipped);
    }

    // ---- the old JSON

    #[test]
    fn facts_read_the_old_preflight_json() {
        let old = serde_json::json!({
            "uv": {"found": true, "version": "uv 0.9.2"},
            "clone": {"path": "", "repo": "x", "dirExists": false, "pyproject": "wrong-name", "projectName": ""},
            "runtime": {"mode": "external", "tvmcpDir": "C:\\t", "payloadOk": true, "projectName": "tvmcp", "provisioned": true, "stale": false, "pythonPath": "p", "uvPath": "u", "lockHash": "h", "chromium": null},
            "kb": {"mode": "off"}, "boot": {"ran": true, "ok": true, "tools": 59, "error": ""}, "claude": "api-key",
            "tv": {"running": true, "port": 9223, "cdp": {"up": true, "browser": "Chrome", "chartTab": true, "tradingview": true, "targets": [{"url": "u", "title": "t"}]}},
            "freePort": 9224, "workflows": {"loaded": [], "failed": []}, "platform": "win32",
            "vault": {"path": "C:\\v", "exists": true, "nonEmpty": true, "hasSkill": true, "hasScaleTool": true, "hasScaleConfig": true, "templateVersion": "2", "templateAvailable": true},
            "toolsPython": {"python": "p", "exists": true, "checked": true, "importOk": true, "error": ""},
            "tvSession": {"set": true, "verifiedAt": 0},
            "scheduler": {"queried": true, "tasks": [{"name": "Trading Macro EU", "state": "Ready", "arguments": "-File s.ps1"}], "script": "s.ps1"},
            "timezone": "Europe/Kyiv"
        });
        let f: Facts = serde_json::from_value(old).unwrap();
        assert!(f.windows && f.now.is_none());
        assert_eq!(
            (f.runtime.mode, f.clone.pyproject),
            (RuntimeMode::External, Pyproject::WrongName)
        );
        assert_eq!(
            f.claude,
            ClaudeFact {
                found: true,
                version: String::new(),
                auth: ClaudeAuth::ApiKey
            }
        );
        assert_eq!(
            (f.free_port, f.tv.cdp.chart_tab, f.tv_session.verified_at),
            (Some(9224), true, None)
        );
        assert_eq!(
            (
                f.scheduler.exe.as_str(),
                f.scheduler.tasks[0].command.as_str()
            ),
            ("s.ps1", "-File s.ps1")
        );
        assert!(f.vault.has_scale_config && f.tools_python.import_ok);
        let ms: Facts = serde_json::from_value(serde_json::json!({"tvSession": {"set": true, "verifiedAt": 1_795_591_860_000_i64}, "claude": {"found": false}})).unwrap();
        assert!(ms.tv_session.verified_at.is_some());
        assert!(!ms.claude.found);
        assert!(serde_json::from_value::<Facts>(serde_json::json!({})).is_ok());
    }

    #[test]
    fn ids_spell_like_the_contract_and_the_old_tasks() {
        assert_eq!(
            serde_json::to_value(CheckId::ClaudeAuth).unwrap(),
            "claude_auth"
        );
        assert_eq!(
            serde_json::to_value(CheckState::Skipped).unwrap(),
            "skipped"
        );
        assert_eq!(
            serde_json::to_value(TaskId::ProvisionRuntime).unwrap(),
            "provision-runtime"
        );
        for t in TaskId::ALL {
            assert_eq!(TaskId::parse(t.as_str()), Some(t));
            assert_eq!(serde_json::to_value(t).unwrap(), t.as_str());
        }
        let json = |f: Fix| serde_json::to_value(f).unwrap();
        assert_eq!(
            json(Fix::SetCdpPort(9224)),
            serde_json::json!({"kind": "set_cdp_port", "value": 9224})
        );
        assert_eq!(
            json(Fix::Task(TaskId::VaultCreate)),
            serde_json::json!({"kind": "task", "value": "vault-create"})
        );
        assert_eq!(
            json(Fix::LaunchTradingView),
            serde_json::json!({"kind": "launch_trading_view"})
        );
        let copy = Fix::Copy {
            text: "x".into(),
            explain_key: "k".into(),
        };
        assert_eq!(
            json(copy.clone()),
            serde_json::json!({"kind": "copy", "value": {"text": "x", "explain_key": "k"}})
        );
        assert_eq!(
            serde_json::from_value::<Fix>(json(copy.clone())).unwrap(),
            copy
        );
        assert_eq!(first_line(""), "—");
    }
}
