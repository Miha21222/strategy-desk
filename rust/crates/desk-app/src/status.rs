//! The sidebar lights: TradingView, CDP, Claude, vault (TZ item 16).
//!
//! Refreshed on events (start, the window getting focus, 5 minutes before a
//! window, after a task) and on the button — never on a timer loop like the
//! Electron app's 20 s poll. Each probe is cheap: a process list
//! (`sysinfo`, no `tasklist`), one HTTP GET with a 1.5 s timeout, two
//! `claude` invocations (`--version`, `auth status`), a few `stat`s.
//!
//! The probes gather [`Facts`]; [`lights`] (pure, tested) turns them into
//! what the UI draws — i18n keys and vars, never sentences.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::vault_template::VAULT_MARKERS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightState {
    #[default]
    Unknown,
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Light {
    pub state: LightState,
    pub key: String,
    pub vars: Vec<(String, String)>,
}

impl Light {
    fn new(state: LightState, key: &str) -> Self {
        Self {
            state,
            key: key.into(),
            vars: Vec::new(),
        }
    }

    fn var(mut self, k: &str, v: impl Into<String>) -> Self {
        self.vars.push((k.into(), v.into()));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StatusLights {
    pub tradingview: Light,
    pub cdp: Light,
    pub claude: Light,
    pub vault: Light,
    /// Unix ms of the probe; 0 = never probed.
    pub checked_at: i64,
}

/// Who answers on the CDP port: desk-core's shape, so the lights and the
/// diagnosis read the same probe.
pub use desk_core::preflight::{cdp_owner, CdpProbe, CdpTarget};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClaudeFact {
    /// The binary that was asked; `None` = not found.
    pub bin: Option<PathBuf>,
    /// `claude --version`; `None` = did not answer.
    pub version: Option<String>,
    /// `claude auth status` said `loggedIn: true`.
    pub logged_in: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultFact {
    NotSet,
    Missing,
    NotAVault,
    Ok,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub tv_running: bool,
    pub port: u16,
    pub cdp: CdpProbe,
    pub claude: ClaudeFact,
    pub vault: VaultFact,
}

/// The lights for a set of facts.
pub fn lights(f: &Facts, checked_at: i64) -> StatusLights {
    use LightState::*;
    let tradingview = if f.tv_running {
        Light::new(Ok, "status.tv.running")
    } else {
        Light::new(Warn, "status.tv.stopped")
    };
    let port = f.port.to_string();
    let cdp = match (&f.cdp, f.tv_running) {
        (c, _) if c.up && c.tradingview => Light::new(
            Ok,
            if c.chart_tab {
                "status.cdp.ok"
            } else {
                "status.cdp.no_chart"
            },
        )
        .var("port", &port),
        (c, _) if c.up => Light::new(Fail, "status.cdp.foreign")
            .var("port", &port)
            .var("owner", cdp_owner(c)),
        (_, true) => Light::new(Warn, "status.cdp.no_flag").var("port", &port),
        _ => Light::new(Fail, "status.cdp.down").var("port", &port),
    };
    let claude = match (&f.claude.bin, &f.claude.version) {
        (None, _) => Light::new(Fail, "status.claude.missing"),
        (Some(_), None) => Light::new(Fail, "status.claude.broken"),
        (Some(_), Some(v)) if f.claude.logged_in => {
            Light::new(Ok, "status.claude.ok").var("version", v.clone())
        }
        (Some(_), Some(v)) => {
            Light::new(Warn, "status.claude.logged_out").var("version", v.clone())
        }
    };
    let vault = match f.vault {
        VaultFact::NotSet => Light::new(Fail, "status.vault.not_set"),
        VaultFact::Missing => Light::new(Fail, "status.vault.missing"),
        VaultFact::NotAVault => Light::new(Warn, "status.vault.not_a_vault"),
        VaultFact::Ok => Light::new(Ok, "status.vault.ok"),
    };
    StatusLights {
        tradingview,
        cdp,
        claude,
        vault,
        checked_at,
    }
}

// ---------------------------------------------------------------- probes

/// TradingView Desktop's process, by name (no `tasklist`).
pub fn tv_running() -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes()
        .values()
        .any(|p| is_tradingview_name(&p.name().to_string_lossy()))
}

pub fn is_tradingview_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "tradingview.exe" || n == "tradingview"
}

/// One `GET http://127.0.0.1:<port><path>` with a deadline, JSON body.
/// Plain HTTP/1.1 over a socket: the DevTools endpoint is local and speaks
/// nothing else, so no HTTP client (and no TLS stack) is pulled in.
pub async fn get_json(port: u16, path: &str, timeout: Duration) -> Option<Value> {
    let fut = async {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .ok()?;
        let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n");
        s.write_all(req.as_bytes()).await.ok()?;
        let mut buf = Vec::new();
        s.take(4 * 1024 * 1024).read_to_end(&mut buf).await.ok()?;
        parse_http_json(&buf)
    };
    tokio::time::timeout(timeout, fut).await.ok().flatten()
}

/// Status 200 and a JSON body (Content-Length or close-delimited; chunked
/// is decoded too, in case a Chromium build ever sends it).
pub fn parse_http_json(raw: &[u8]) -> Option<Value> {
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..cut]).ok()?;
    let body = &raw[cut + 4..];
    let mut lines = head.lines();
    let status = lines.next()?;
    if status.split_whitespace().nth(1) != Some("200") {
        return None;
    }
    let chunked = lines.any(|l| {
        let l = l.to_ascii_lowercase();
        l.starts_with("transfer-encoding:") && l.contains("chunked")
    });
    if !chunked {
        return serde_json::from_slice(body).ok();
    }
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let nl = rest.windows(2).position(|w| w == b"\r\n")?;
        let size = usize::from_str_radix(
            std::str::from_utf8(&rest[..nl])
                .ok()?
                .split(';')
                .next()?
                .trim(),
            16,
        )
        .ok()?;
        rest = &rest[nl + 2..];
        if size == 0 {
            break;
        }
        out.extend_from_slice(rest.get(..size)?);
        rest = rest.get(size + 2..)?;
    }
    serde_json::from_slice(&out).ok()
}

/// What answers on the CDP port: `/json/version`, then `/json/list`.
pub async fn cdp_probe(port: u16, timeout: Duration) -> CdpProbe {
    let Some(version) = get_json(port, "/json/version", timeout).await else {
        return CdpProbe::default();
    };
    let list = get_json(port, "/json/list", timeout).await;
    cdp_from(&version, list.as_ref())
}

/// The pure half of `cdpProbe`: the first eight targets, and whether any of
/// them is TradingView (the port may be a browser panel's).
pub fn cdp_from(version: &Value, list: Option<&Value>) -> CdpProbe {
    let targets: Vec<CdpTarget> = list
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .take(8)
                .map(|p| {
                    let s = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    CdpTarget {
                        url: s("url"),
                        title: s("title"),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let has = |s: &str, pat: &str| s.to_ascii_lowercase().contains(pat);
    let chart_tab = targets.iter().any(|t| has(&t.url, "tradingview.com/chart"));
    let tradingview = chart_tab
        || targets
            .iter()
            .any(|t| has(&t.url, "tradingview") || has(&t.title, "tradingview"));
    let browser = version
        .get("Browser")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    CdpProbe {
        up: true,
        browser,
        chart_tab,
        tradingview,
        targets,
    }
}

/// Run a short command with a deadline; stdout on a zero exit.
pub async fn output(program: &Path, args: &[&str], timeout: Duration) -> Option<String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = tokio::time::timeout(timeout, cmd.output())
        .await
        .ok()?
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `claude --version` and `claude auth status` (JSON `loggedIn`).
pub async fn claude_fact(bin: Option<PathBuf>) -> ClaudeFact {
    let Some(bin) = bin else {
        return ClaudeFact::default();
    };
    let t = Duration::from_secs(10);
    let version = output(&bin, &["--version"], t)
        .await
        .map(|v| v.lines().next().unwrap_or("").to_string());
    let logged_in = match &version {
        Some(_) => output(&bin, &["auth", "status"], t)
            .await
            .and_then(|o| serde_json::from_str::<Value>(&o).ok())
            .and_then(|v| v.get("loggedIn").and_then(Value::as_bool))
            .unwrap_or(false),
        None => false,
    };
    ClaudeFact {
        bin: Some(bin),
        version,
        logged_in,
    }
}

pub fn vault_fact(vault: Option<&Path>) -> VaultFact {
    match vault {
        None => VaultFact::NotSet,
        Some(v) if v.as_os_str().is_empty() => VaultFact::NotSet,
        Some(v) if !v.is_dir() => VaultFact::Missing,
        Some(v)
            if VAULT_MARKERS.iter().any(|m| {
                m.split('/')
                    .fold(v.to_path_buf(), |p, s| p.join(s))
                    .is_file()
            }) =>
        {
            VaultFact::Ok
        }
        Some(_) => VaultFact::NotAVault,
    }
}

/// Every probe; the ones that wait run concurrently.
pub async fn probe(port: u16, claude: Option<PathBuf>, vault: Option<PathBuf>) -> Facts {
    let tv = tokio::task::spawn_blocking(tv_running);
    let (cdp, claude) = tokio::join!(
        cdp_probe(port, Duration::from_millis(1500)),
        claude_fact(claude)
    );
    Facts {
        tv_running: tv.await.unwrap_or(false),
        port,
        cdp,
        claude,
        vault: vault_fact(vault.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts() -> Facts {
        Facts {
            tv_running: true,
            port: 9223,
            cdp: CdpProbe {
                up: true,
                tradingview: true,
                chart_tab: true,
                ..CdpProbe::default()
            },
            claude: ClaudeFact {
                bin: Some("claude".into()),
                version: Some("2.1.332 (Claude Code)".into()),
                logged_in: true,
            },
            vault: VaultFact::Ok,
        }
    }

    #[test]
    fn lights_per_fact() {
        let l = lights(&facts(), 7);
        assert_eq!(
            (
                l.tradingview.state,
                l.cdp.state,
                l.claude.state,
                l.vault.state
            ),
            (
                LightState::Ok,
                LightState::Ok,
                LightState::Ok,
                LightState::Ok
            )
        );
        assert_eq!(
            l.claude.vars,
            vec![("version".into(), "2.1.332 (Claude Code)".into())]
        );
        let mut f = facts();
        f.cdp = CdpProbe {
            up: true,
            targets: vec![CdpTarget {
                url: String::new(),
                title: "FX Replay".into(),
            }],
            ..CdpProbe::default()
        };
        let l = lights(&f, 0);
        assert_eq!(l.cdp.key, "status.cdp.foreign");
        assert!(l.cdp.vars.contains(&("owner".into(), "FX Replay".into())));
        f.cdp = CdpProbe::default();
        assert_eq!(
            lights(&f, 0).cdp.key,
            "status.cdp.no_flag",
            "running without --remote-debugging-port"
        );
        f.tv_running = false;
        assert_eq!(lights(&f, 0).cdp.key, "status.cdp.down");
        assert_eq!(lights(&f, 0).tradingview.state, LightState::Warn);
        f.claude.logged_in = false;
        assert_eq!(lights(&f, 0).claude.key, "status.claude.logged_out");
        f.claude.version = None;
        assert_eq!(lights(&f, 0).claude.key, "status.claude.broken");
        f.claude.bin = None;
        assert_eq!(lights(&f, 0).claude.key, "status.claude.missing");
        f.vault = VaultFact::NotAVault;
        assert_eq!(lights(&f, 0).vault.state, LightState::Warn);
    }

    #[test]
    fn who_owns_the_port() {
        let v = json!({"Browser": "Chrome/126.0"});
        let tv = cdp_from(
            &v,
            Some(&json!([{"url": "https://www.tradingview.com/chart/abc/", "title": "DAX"}])),
        );
        assert!(tv.up && tv.tradingview && tv.chart_tab);
        assert_eq!(tv.targets.len(), 1);
        let fx = cdp_from(
            &v,
            Some(&json!([{"url": "https://app.fxreplay.com/x", "title": ""}])),
        );
        assert!(!fx.tradingview);
        assert_eq!(cdp_owner(&fx), "app.fxreplay.com");
        assert_eq!(
            cdp_owner(&cdp_from(
                &v,
                Some(&json!([{"url": "", "title": "Панель"}]))
            )),
            "Панель"
        );
        assert_eq!(cdp_owner(&cdp_from(&v, None)), "Chrome/126.0");
    }

    #[test]
    fn http_bodies() {
        let ok = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"Browser\":1}";
        assert_eq!(parse_http_json(ok), Some(json!({"Browser": 1})));
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n{\"a\":\r\n2\r\n1}\r\n0\r\n\r\n";
        assert_eq!(parse_http_json(chunked), Some(json!({"a": 1})));
        assert_eq!(parse_http_json(b"HTTP/1.1 404 Not Found\r\n\r\n{}"), None);
        assert_eq!(parse_http_json(b"garbage"), None);
    }

    #[tokio::test]
    async fn a_local_cdp_endpoint_is_probed_and_a_dead_port_times_out_fast() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            for _ in 0..2 {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let mut req = [0u8; 1024];
                let n = s.read(&mut req).await.unwrap_or(0);
                let path = String::from_utf8_lossy(&req[..n])
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                let body = if path == "/json/version" {
                    r#"{"Browser":"Chrome/126"}"#.to_string()
                } else {
                    r#"[{"url":"https://www.tradingview.com/chart/x/","title":"t"}]"#.to_string()
                };
                let _ = s
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len()
                        )
                        .as_bytes(),
                    )
                    .await;
            }
        });
        let p = cdp_probe(port, Duration::from_millis(1500)).await;
        assert!(p.up && p.tradingview && p.chart_tab, "{p:?}");
        // A listener that never answers: the 1.5 s cap holds (here 200 ms).
        let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let sport = silent.local_addr().unwrap().port();
        let t0 = std::time::Instant::now();
        assert_eq!(
            cdp_probe(sport, Duration::from_millis(200)).await,
            CdpProbe::default()
        );
        assert!(t0.elapsed() < Duration::from_secs(2));
        drop(silent);
    }

    #[test]
    fn names_and_vaults() {
        assert!(is_tradingview_name("TradingView.exe"));
        assert!(!is_tradingview_name("TradingViewHelper.exe"));
        let d = tempfile::tempdir().unwrap();
        assert_eq!(vault_fact(None), VaultFact::NotSet);
        assert_eq!(vault_fact(Some(&d.path().join("nope"))), VaultFact::Missing);
        assert_eq!(vault_fact(Some(d.path())), VaultFact::NotAVault);
        std::fs::create_dir_all(d.path().join("indices-kb/tools")).unwrap();
        std::fs::write(d.path().join("indices-kb/tools/scale.py"), b"").unwrap();
        assert_eq!(vault_fact(Some(d.path())), VaultFact::Ok);
    }
}
