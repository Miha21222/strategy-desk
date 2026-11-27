//! «Диагностика» and the wizard's list: the probes behind
//! `desk_core::preflight::build_checks` (port of `src/main/preflight.ts`
//! without the knowledge base, the official MCP and Codex).
//!
//! Like the old app: nothing here installs, downloads or changes anything;
//! it caches nothing (a stale green light is worse than a slow check); every
//! probe has a deadline; a probe that cannot answer reports «not known».

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use desk_core::preflight::{
    BootFact, ClaudeAuth, ClaudeFact, CloneFact, Facts, Pyproject, RuntimeFact, RuntimeMode,
    ScheduledTask, SchedulerFact, ToolsPythonFact, TvFact, TvSessionFact, UvFact, VaultFact,
    PORT_CANDIDATES,
};
use desk_core::settings::Settings;
use serde_json::Value;

use crate::macro_env::{Platform, TOOLS_IMPORT_CODE};
use crate::runtime::{self, Mode, RuntimePaths};
use crate::status::{cdp_probe, output, tv_running};
use crate::vault_template::VAULT_MARKERS;

/// What the probes need besides the settings.
pub struct Inputs {
    pub settings: Settings,
    pub vault: Option<PathBuf>,
    pub runtime: RuntimePaths,
    pub claude: Option<PathBuf>,
    pub has_tv_session: bool,
    pub exe: Option<PathBuf>,
    pub has_template: bool,
    pub now: DateTime<Utc>,
}

const SHORT: Duration = Duration::from_secs(10);
/// tvmcp's `--check` imports numba and friends: seconds, not milliseconds.
const BOOT: Duration = Duration::from_secs(60);

pub async fn facts(i: Inputs) -> Facts {
    let p = Platform::host();
    let port = i.settings.cdp_port;
    let tv = tokio::task::spawn_blocking(tv_running);
    let (uv, cdp, claude, scheduler) = tokio::join!(
        uv_fact(),
        cdp_probe(port, Duration::from_millis(1500)),
        claude_fact(i.claude.clone()),
        scheduler_fact(i.exe.as_deref()),
    );
    let runtime = runtime_fact(&i.runtime, p);
    let clone = clone_fact(&i.settings.tvmcp_path);
    let tools_python = tools_python_fact(&i.runtime, &i.settings.tvmcp_path, p).await;
    let bootable = match i.runtime.mode() {
        Mode::Bundled => runtime.provisioned || runtime.stale,
        Mode::External => uv.found && clone.pyproject == Pyproject::Ok && clone.dir_exists,
    };
    let boot = if bootable {
        boot_fact(&i.runtime, &i.settings, p).await
    } else {
        BootFact::default()
    };
    let free_port = if cdp.up && !cdp.tradingview {
        free_port(port).await
    } else {
        None
    };
    Facts {
        now: Some(i.now),
        windows: cfg!(windows),
        uv,
        runtime,
        clone,
        boot,
        tools_python,
        claude,
        tv: TvFact {
            running: tv.await.unwrap_or(false),
            port,
            cdp,
        },
        free_port,
        vault: vault_fact(i.vault.as_deref(), i.has_template),
        tv_session: TvSessionFact {
            set: i.has_tv_session,
            verified_at: i.settings.tv_session_verified_at,
        },
        scheduler,
        timezone: timezone().await,
    }
}

async fn uv_fact() -> UvFact {
    match output(Path::new("uv"), &["--version"], SHORT).await {
        Some(v) => UvFact {
            found: true,
            version: v.lines().next().unwrap_or("").to_string(),
        },
        None => UvFact::default(),
    }
}

/// `claude --version`, then `claude auth status` (JSON).
pub async fn claude_fact(bin: Option<PathBuf>) -> ClaudeFact {
    let Some(bin) = bin else {
        return ClaudeFact::default();
    };
    let Some(version) = output(&bin, &["--version"], SHORT).await else {
        return ClaudeFact::default();
    };
    let auth = output(&bin, &["auth", "status"], SHORT)
        .await
        .and_then(|o| serde_json::from_str::<Value>(&o).ok())
        .map_or(ClaudeAuth::None, |v| auth_of(&v));
    ClaudeFact {
        found: true,
        version: version.lines().next().unwrap_or("").to_string(),
        auth,
    }
}

/// `{"loggedIn": true, "authMethod": "claude.ai" | "api_key" | …}`.
pub fn auth_of(v: &Value) -> ClaudeAuth {
    if v.get("loggedIn").and_then(Value::as_bool) != Some(true) {
        return ClaudeAuth::None;
    }
    let m = v
        .get("authMethod")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if m.contains("api") {
        ClaudeAuth::ApiKey
    } else if m.contains("token") {
        ClaudeAuth::SetupToken
    } else {
        ClaudeAuth::Subscription
    }
}

pub fn runtime_fact(paths: &RuntimePaths, p: Platform) -> RuntimeFact {
    let f = runtime::fact(paths, p);
    let dir = paths.payload_dir.clone().unwrap_or_default();
    let project_name = pyproject_name(&dir.join("pyproject.toml")).unwrap_or_default();
    let lock_hash = std::fs::read(dir.join("uv.lock"))
        .map(|b| runtime::lock_hash(&b))
        .unwrap_or_default();
    let python = runtime::tools_python(paths, "", p)
        .map(|x| x.to_string_lossy().into_owned())
        .unwrap_or_default();
    RuntimeFact {
        mode: match f.mode {
            Mode::Bundled => RuntimeMode::Bundled,
            Mode::External => RuntimeMode::External,
        },
        tvmcp_dir: dir.to_string_lossy().into_owned(),
        // External mode has no payload to be broken.
        payload_ok: f.payload_ok && (f.mode == Mode::External || project_name == "tvmcp"),
        project_name,
        provisioned: f.provisioned,
        stale: f.stale,
        python_path: python,
        lock_hash,
    }
}

/// `name = "…"` of a pyproject (the first one: `[project]` comes first in tvmcp's).
pub fn pyproject_name(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "name").then(|| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
    })
}

pub fn clone_fact(path: &str) -> CloneFact {
    let p = path.trim();
    if p.is_empty() {
        return CloneFact {
            pyproject: Pyproject::Missing,
            ..CloneFact::default()
        };
    }
    let dir = Path::new(p);
    let name = pyproject_name(&dir.join("pyproject.toml"));
    let pyproject = match name.as_deref() {
        None => Pyproject::Missing,
        Some("tvmcp") => Pyproject::Ok,
        Some(_) => Pyproject::WrongName,
    };
    CloneFact {
        path: p.into(),
        dir_exists: dir.is_dir(),
        pyproject,
        project_name: name.unwrap_or_default(),
    }
}

async fn tools_python_fact(paths: &RuntimePaths, tvmcp_path: &str, p: Platform) -> ToolsPythonFact {
    let Some(py) = runtime::tools_python(paths, tvmcp_path, p) else {
        return ToolsPythonFact::default();
    };
    let exists = py.is_file();
    let mut f = ToolsPythonFact {
        python: py.to_string_lossy().into_owned(),
        exists,
        ..ToolsPythonFact::default()
    };
    if exists {
        f.checked = true;
        match output(&py, &["-c", TOOLS_IMPORT_CODE], SHORT * 3).await {
            Some(_) => f.import_ok = true,
            None => f.error = "import pandas, tvmcp failed".into(),
        }
    }
    f
}

/// `<python> -m tvmcp --check` (bundled) or `uv --directory <clone> run python -m tvmcp --check`.
async fn boot_fact(paths: &RuntimePaths, s: &Settings, p: Platform) -> BootFact {
    let (program, args): (PathBuf, Vec<String>) = match paths.mode() {
        Mode::Bundled => match runtime::tools_python(paths, "", p) {
            Some(py) => (py, vec!["-m".into(), "tvmcp".into(), "--check".into()]),
            None => return BootFact::default(),
        },
        Mode::External => (
            PathBuf::from("uv"),
            [
                "--directory",
                s.tvmcp_path.trim(),
                "run",
                "python",
                "-m",
                "tvmcp",
                "--check",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        ),
    };
    let mut cmd = tokio::process::Command::new(&program);
    cmd.args(&args)
        .envs(runtime::runtime_env(paths))
        .env("TV_CDP_URL", format!("http://127.0.0.1:{}", s.cdp_port))
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = match tokio::time::timeout(BOOT, cmd.output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return BootFact {
                ran: true,
                ok: false,
                tools: 0,
                error: e.to_string(),
            }
        }
        Err(_) => {
            return BootFact {
                ran: true,
                ok: false,
                tools: 0,
                error: "timeout".into(),
            }
        }
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let tools = tools_registered(&text);
    let ok = out.status.success() && tools.is_some();
    let error = if ok {
        String::new()
    } else {
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    };
    BootFact {
        ran: true,
        ok,
        tools: tools.unwrap_or(0),
        error,
    }
}

/// `(\d+) tools registered`.
pub fn tools_registered(text: &str) -> Option<u32> {
    text.lines().find_map(|l| {
        let i = l.find(" tools registered")?;
        l[..i]
            .rsplit(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    })
}

async fn free_port(taken: u16) -> Option<u16> {
    for p in PORT_CANDIDATES.into_iter().filter(|p| *p != taken) {
        if !cdp_probe(p, Duration::from_millis(500)).await.up {
            return Some(p);
        }
    }
    None
}

pub fn vault_fact(vault: Option<&Path>, template_available: bool) -> VaultFact {
    let Some(v) = vault.filter(|v| !v.as_os_str().is_empty()) else {
        return VaultFact {
            template_available,
            ..VaultFact::default()
        };
    };
    let has = |rel: &str| {
        rel.split('/')
            .fold(v.to_path_buf(), |p, s| p.join(s))
            .is_file()
    };
    let [skill, tool, config] = VAULT_MARKERS;
    let exists = v.is_dir();
    VaultFact {
        path: v.to_string_lossy().into_owned(),
        exists,
        non_empty: exists && std::fs::read_dir(v).is_ok_and(|mut d| d.next().is_some()),
        has_skill: has(skill),
        has_scale_tool: has(tool),
        has_scale_config: has(config),
        template_version: std::fs::read(v.join("template.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|j| {
                j.get("version")
                    .map(|x| x.as_str().map_or_else(|| x.to_string(), str::to_owned))
            })
            .unwrap_or_default(),
        template_available,
    }
}

/// The jobs as `Get-ScheduledTask` reports them (Windows only; read-only).
async fn scheduler_fact(exe: Option<&Path>) -> SchedulerFact {
    let exe = exe
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    if !cfg!(windows) {
        return SchedulerFact {
            queried: false,
            tasks: Vec::new(),
            exe,
        };
    }
    let spec = crate::scheduler::powershell(&crate::scheduler::query_script());
    let args: Vec<&str> = spec.args.iter().map(String::as_str).collect();
    let Some(out) = output(&spec.program, &args, Duration::from_secs(20)).await else {
        return SchedulerFact {
            queried: false,
            tasks: Vec::new(),
            exe,
        };
    };
    let tasks = crate::scheduler::parse_query(&out)
        .into_iter()
        .map(|j| ScheduledTask {
            name: j.name,
            state: j.state,
            command: j.action,
        })
        .collect();
    SchedulerFact {
        queried: true,
        tasks,
        exe,
    }
}

/// The PC's zone: `tzutil /g` on Windows (`FLE Standard Time`), the IANA
/// name elsewhere (`TZ`, `/etc/timezone`, the `/etc/localtime` link).
async fn timezone() -> String {
    if cfg!(windows) {
        return output(Path::new("tzutil"), &["/g"], SHORT)
            .await
            .unwrap_or_default();
    }
    if let Some(tz) = std::env::var("TZ").ok().filter(|t| !t.trim().is_empty()) {
        return tz.trim_start_matches(':').to_string();
    }
    if let Ok(t) = std::fs::read_to_string("/etc/timezone") {
        return t.trim().to_string();
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|p| {
            let s = p.to_string_lossy().into_owned();
            s.split_once("zoneinfo/").map(|(_, z)| z.to_string())
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn auth_and_tools_lines() {
        assert_eq!(
            auth_of(&json!({"loggedIn": false, "authMethod": "none"})),
            ClaudeAuth::None
        );
        assert_eq!(
            auth_of(&json!({"loggedIn": true, "authMethod": "claude.ai"})),
            ClaudeAuth::Subscription
        );
        assert_eq!(
            auth_of(&json!({"loggedIn": true, "authMethod": "api_key"})),
            ClaudeAuth::ApiKey
        );
        assert_eq!(
            tools_registered("boot ok\n42 tools registered (hybrid)\n"),
            Some(42)
        );
        assert_eq!(tools_registered("Traceback…"), None);
    }

    #[test]
    fn clone_and_vault_facts() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(clone_fact("").pyproject, Pyproject::Missing);
        std::fs::write(
            d.path().join("pyproject.toml"),
            "[project]\nname = \"other\"\n",
        )
        .unwrap();
        let c = clone_fact(&d.path().to_string_lossy());
        assert_eq!(
            (c.pyproject, c.dir_exists, c.project_name.as_str()),
            (Pyproject::WrongName, true, "other")
        );
        std::fs::write(
            d.path().join("pyproject.toml"),
            "[project]\nname = 'tvmcp'\n",
        )
        .unwrap();
        assert_eq!(
            clone_fact(&d.path().to_string_lossy()).pyproject,
            Pyproject::Ok
        );

        let v = tempfile::tempdir().unwrap();
        let f = vault_fact(Some(v.path()), true);
        assert!(f.exists && !f.non_empty && !f.has_skill && f.template_available);
        std::fs::create_dir_all(v.path().join("indices-kb").join("tools")).unwrap();
        std::fs::write(
            v.path().join("indices-kb").join("tools").join("scale.py"),
            b"",
        )
        .unwrap();
        std::fs::write(v.path().join("template.json"), br#"{"version":"2026.11"}"#).unwrap();
        let f = vault_fact(Some(v.path()), false);
        assert!(f.non_empty && f.has_scale_tool && !f.has_scale_config && !f.template_available);
        assert_eq!(f.template_version, "2026.11");
        assert_eq!(vault_fact(None, false).path, "");
    }

    #[tokio::test]
    async fn the_whole_gathering_never_fails_and_builds_checks() {
        let d = tempfile::tempdir().unwrap();
        let settings = Settings {
            cdp_port: 1,
            ..Settings::default()
        };
        let rt = RuntimePaths::under(&d.path().join("py"), None, None);
        let f = facts(Inputs {
            settings: settings.clone(),
            vault: None,
            runtime: rt,
            claude: Some(d.path().join("no-claude")),
            has_tv_session: false,
            exe: None,
            has_template: false,
            now: Utc::now(),
        })
        .await;
        assert!(!f.claude.found);
        assert!(!f.tv.cdp.up);
        let checks = desk_core::preflight::build_checks(&settings, &f);
        assert!(!checks.is_empty());
        assert!(
            !desk_core::preflight::all_ok(&checks),
            "a bare PC is not ready"
        );
    }
}
