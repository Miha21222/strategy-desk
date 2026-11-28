//! The headless run the Task Scheduler starts:
//! `strategy-desk run --window eu|us|review [--date YYYY-MM-DD]` (TZ item 18).
//!
//! The binary parses its argv and calls [`run`]; everything else is here:
//! the settings, the sessionid from the credential store, the macro env,
//! `due_window` (the scheduler wakes the desk at every minute a window may
//! have; only the right one does work), then `claude -p <run_command>` in
//! the vault, its output into `<data>/logs/run-<D>-<w>.log`, exit code 0/1.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use desk_core::windows::WindowId;

use crate::chat::launch::{find_claude, ClaudeLaunch, MACRO_PROMPT};
use crate::claude::config::{AUTH_ENV_REMOVED, MACRO_ALLOWED, MACRO_DISALLOWED};
use crate::claude::{DriverEvent, Translator};
use crate::macro_env::{macro_env_vars, MacroEnvInput, Platform};
use crate::paths;
use crate::runtime::{self, runtime_env, tools_python};
use crate::secrets::{tv_session, SecretStore};
use crate::settings_store::SettingsStore;
use desk_core::schedule::due_window;

/// A scheduled review gets this long before it is killed.
pub const RUN_TIMEOUT: Duration = Duration::from_secs(90 * 60);

pub struct RunArgs<'a> {
    pub window: WindowId,
    /// `--date`: a run by hand for that day; skips the «is it my minute» check.
    pub date: Option<NaiveDate>,
    pub now: DateTime<Utc>,
    /// `None` = the platform data dir.
    pub data_dir: Option<PathBuf>,
    pub secrets: &'a dyn SecretStore,
    /// Tests: the fake claude. Production: empty (settings, DESK_CLAUDE_BIN, PATH).
    pub claude: ClaudeLaunch,
}

/// `--window eu --date 2026-11-26` → (window, date). `Err` = usage text.
pub fn parse_args<I: IntoIterator<Item = String>>(
    args: I,
) -> Result<(WindowId, Option<NaiveDate>), String> {
    let mut window = None;
    let mut date = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--window" => {
                let v = it.next().unwrap_or_default();
                window = WindowId::ALL.into_iter().find(|w| w.as_str() == v);
                if window.is_none() {
                    return Err(format!("unknown window {v:?}: eu | us | review"));
                }
            }
            "--date" => {
                let v = it.next().unwrap_or_default();
                date = Some(desk_core::names::parse_date(&v).map_err(|e| e.to_string())?);
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    let window = window.ok_or("--window eu|us|review is required")?;
    Ok((window, date))
}

/// claude's argv for one headless review.
pub fn claude_args(window: WindowId, date: NaiveDate) -> Vec<String> {
    vec![
        "-p".into(),
        desk_core::prompts::run_command(window, date),
        "--output-format".into(),
        "stream-json".into(),
        "--verbose".into(),
        "--permission-mode".into(),
        "dontAsk".into(),
        "--max-turns".into(),
        "200".into(),
        "--allowedTools".into(),
        MACRO_ALLOWED.join(","),
        "--disallowedTools".into(),
        MACRO_DISALLOWED.join(","),
        "--append-system-prompt".into(),
        MACRO_PROMPT.trim().into(),
    ]
}

/// `<logs>/run.lock`: one run at a time. A lock older than the run timeout
/// is a crashed run's and is taken over.
struct RunLock {
    path: PathBuf,
}

impl RunLock {
    fn take(dir: &Path, now: DateTime<Utc>) -> Option<Self> {
        let path = dir.join("run.lock");
        let _ = std::fs::create_dir_all(dir);
        for _ in 0..2 {
            match std::fs::File::create_new(&path) {
                Ok(mut f) => {
                    let _ = write!(f, "{} {}", std::process::id(), now.timestamp());
                    return Some(Self { path });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|t| {
                            t.split_whitespace()
                                .nth(1)
                                .and_then(|s| s.parse::<i64>().ok())
                        })
                        .is_none_or(|at| now.timestamp() - at > RUN_TIMEOUT.as_secs() as i64 + 600);
                    if !stale {
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for RunLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct Log {
    file: Option<std::fs::File>,
}

impl Log {
    fn open(data: &Path, date: NaiveDate, w: WindowId) -> (Self, PathBuf) {
        let dir = paths::logs_dir(data);
        let path = dir.join(format!(
            "run-{}-{}.log",
            date.format("%Y-%m-%d"),
            w.as_str()
        ));
        let file = std::fs::create_dir_all(&dir)
            .and_then(|_| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
            })
            .ok();
        (Self { file }, path)
    }

    fn line(&mut self, s: &str) {
        if let Some(f) = self.file.as_mut() {
            let _ = writeln!(f, "{s}");
        }
    }
}

/// The whole headless run. Returns the process exit code.
pub fn run(a: RunArgs<'_>) -> i32 {
    let Some(data) = paths::data_dir(a.data_dir.as_deref()) else {
        eprintln!("strategy-desk run: no data folder on this system");
        return 1;
    };
    let (settings, _, settings_err) = SettingsStore::new(&data).load(
        paths::electron_config().as_deref(),
        paths::default_vault().as_deref(),
    );
    let today = a.now.with_timezone(&chrono_tz::Europe::Kyiv).date_naive();
    let date = a.date.unwrap_or(today);
    let (mut log, _) = Log::open(&data, date, a.window);
    log.line(&format!(
        "== {} run --window {}{}",
        a.now.to_rfc3339(),
        a.window.as_str(),
        a.date.map(|d| format!(" --date {d}")).unwrap_or_default()
    ));
    if let Some(e) = settings_err {
        log.line(&format!("settings: {e} (defaults used)"));
    }
    if a.date.is_none() && due_window(a.now, &settings) != Some(a.window) {
        log.line("not this window's minute: nothing to do");
        return 0;
    }
    let Some(vault) = settings
        .vault
        .as_deref()
        .map(PathBuf::from)
        .filter(|v| v.is_dir())
    else {
        log.line("no vault: set one in the desk first");
        return 1;
    };
    let explicit = a.claude.program.clone().or_else(|| {
        (!settings.claude_bin.trim().is_empty()).then(|| PathBuf::from(settings.claude_bin.trim()))
    });
    let launch = ClaudeLaunch {
        program: explicit,
        ..a.claude.clone()
    };
    let Some(claude) = find_claude(&launch) else {
        log.line("claude not found (settings, DESK_CLAUDE_BIN, PATH)");
        return 1;
    };
    let rt = runtime::resolve(
        &paths::runtime_root(&data),
        paths::exe_dir().as_deref(),
        &settings.tvmcp_path,
    );
    let python = tools_python(&rt, &settings.tvmcp_path, Platform::host()).filter(|p| p.is_file());
    let session = tv_session(a.secrets);
    // A review without TradingView data is not a review (TEST_PLAN №18 I).
    if session.is_none() {
        log.line("no TradingView sessionid in the Credential Manager: set it in the desk (wizard, step «Сессия»)");
        return 1;
    }
    // Two runs at once (a late trigger meeting the next one): the second
    // leaves without work.
    let _lock = match RunLock::take(&paths::logs_dir(&data), a.now) {
        Some(l) => l,
        None => {
            log.line("another run is in progress: nothing to do");
            return 0;
        }
    };
    let mut env: BTreeMap<String, String> = macro_env_vars(&MacroEnvInput {
        vault: vault.to_string_lossy().into_owned(),
        python: python
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        claude: claude.to_string_lossy().into_owned(),
        session_id: session.clone(),
        cdp_port: settings.cdp_port,
        platform: Platform::host(),
    });
    env.extend(runtime_env(&rt));
    env.insert("DISABLE_AUTOUPDATER".into(), "1".into());
    env.extend(launch.env.clone());
    // Keys only: the cookie never reaches the log.
    log.line(&format!(
        "claude: {}  cwd: {}  env: {}",
        claude.display(),
        vault.display(),
        env.keys().cloned().collect::<Vec<_>>().join(",")
    ));

    let mut cmd = Command::new(&claude);
    cmd.args(&launch.program_args)
        .args(claude_args(a.window, date))
        .current_dir(&vault)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for k in AUTH_ENV_REMOVED {
        cmd.env_remove(k);
    }
    cmd.envs(&env);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log.line(&format!("cannot start claude: {e}"));
            return 1;
        }
    };
    let stderr = child.stderr.take().map(|e| {
        std::thread::spawn(move || {
            BufReader::new(e)
                .lines()
                .map_while(Result::ok)
                .take(2000)
                .collect::<Vec<_>>()
        })
    });
    // A watchdog: a hung review must not still be there for the next run.
    let stdout = child.stdout.take();
    let child = std::sync::Arc::new(std::sync::Mutex::new(child));
    let timed_out = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    let watchdog = {
        let (child, timed_out) = (
            std::sync::Arc::clone(&child),
            std::sync::Arc::clone(&timed_out),
        );
        std::thread::spawn(move || {
            if done_rx.recv_timeout(RUN_TIMEOUT).is_err() {
                timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
                // A poisoned lock still holds the child; kill it anyway.
                let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
            }
        })
    };
    let mut tr = Translator::new();
    let mut ev = Vec::new();
    let mut failed = false;
    if let Some(out) = stdout {
        for line in BufReader::new(out).split(b'\n').map_while(Result::ok) {
            tr.feed(&String::from_utf8_lossy(&line), &mut ev);
            for e in ev.drain(..) {
                match e {
                    DriverEvent::TextDelta { text } => log.line(&text),
                    DriverEvent::ToolCall { name, summary, .. } => {
                        log.line(&format!("[tool] {name}: {summary}"))
                    }
                    DriverEvent::ToolResult { ok: false, id } => {
                        log.line(&format!("[tool failed] {id}"))
                    }
                    DriverEvent::Error { key, detail } => {
                        failed = true;
                        log.line(&format!("[error] {key}: {detail}"));
                    }
                    DriverEvent::Usage { cost_usd, .. } => {
                        log.line(&format!("[cost] {}", cost_usd.unwrap_or(0.0)))
                    }
                    _ => {}
                }
            }
        }
    }
    let status = child.lock().unwrap_or_else(|e| e.into_inner()).wait();
    let _ = done_tx.send(());
    let _ = watchdog.join();
    let timed_out = timed_out.load(std::sync::atomic::Ordering::SeqCst);
    if timed_out {
        log.line("[timeout] the review ran too long and was stopped");
    }
    if let Some(h) = stderr {
        for l in h.join().unwrap_or_default() {
            log.line(&format!("[stderr] {l}"));
        }
    }
    let ok = !timed_out && !failed && status.as_ref().is_ok_and(|s| s.success());
    log.line(&format!(
        "== exit {}",
        status
            .map(|s| s.to_string())
            .unwrap_or_else(|e| e.to_string())
    ));
    i32::from(!ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use chrono::TimeZone;
    use desk_core::settings::Settings;

    const FAKE: &str = "DESK_TEST_FAKE_RUN";

    /// The fake `claude -p`: records argv and env keys, prints a result.
    #[test]
    fn fake_run_entry() {
        let Ok(mode) = std::env::var(FAKE) else {
            return;
        };
        let argv: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
        let keys: Vec<String> = std::env::vars()
            .map(|(k, _)| k)
            .filter(|k| k.starts_with("MACRO_") || k.starts_with("TV_"))
            .collect();
        let session = std::env::var("TV_SESSIONID").unwrap_or_default();
        if let Ok(p) = std::env::var("DESK_TEST_FAKE_OUT") {
            let _ = std::fs::write(p, serde_json::json!({ "argv": argv, "keys": keys, "session": session, "cwd": std::env::current_dir().unwrap_or_default() }).to_string());
        }
        println!(
            r#"{{"type":"assistant","message":{{"id":"m","content":[{{"type":"text","text":"Обзор готов."}}]}},"parent_tool_use_id":null}}"#
        );
        if mode == "fail" {
            println!(
                r#"{{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["boom"],"session_id":"s"}}"#
            );
            std::process::exit(1);
        }
        println!(
            r#"{{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.5,"session_id":"s","usage":{{"input_tokens":1,"output_tokens":1}}}}"#
        );
        std::process::exit(0);
    }

    struct Rig {
        data: tempfile::TempDir,
        vault: tempfile::TempDir,
        out: PathBuf,
    }

    fn rig() -> Rig {
        let data = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let s = Settings {
            vault: Some(vault.path().to_string_lossy().into_owned()),
            ..Settings::default()
        };
        SettingsStore::new(data.path()).save(&s).unwrap();
        let out = data.path().join("fake.json");
        Rig { data, vault, out }
    }

    fn launch(mode: &str, out: &Path) -> ClaudeLaunch {
        let mut l = ClaudeLaunch {
            program: Some(std::env::current_exe().unwrap()),
            program_args: [
                "--exact",
                "run::tests::fake_run_entry",
                "--nocapture",
                "-q",
                "--",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            ..ClaudeLaunch::default()
        };
        l.env.insert(FAKE.into(), mode.into());
        l.env
            .insert("DESK_TEST_FAKE_OUT".into(), out.display().to_string());
        l
    }

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        // November: Kyiv = UTC+2.
        Utc.with_ymd_and_hms(2026, 11, 26, h - 2, m, 0).unwrap()
    }

    #[test]
    fn args() {
        assert_eq!(
            parse_args(["--window".into(), "us".into()]).unwrap(),
            (WindowId::Us, None)
        );
        let (w, d) = parse_args([
            "--window".into(),
            "review".into(),
            "--date".into(),
            "2026-11-26".into(),
        ])
        .unwrap();
        assert_eq!(
            (w, d),
            (WindowId::Review, NaiveDate::from_ymd_opt(2026, 11, 26))
        );
        assert!(parse_args(["--window".into(), "asia".into()]).is_err());
        assert!(parse_args(Vec::<String>::new()).is_err());
        assert!(parse_args([
            "--window".into(),
            "eu".into(),
            "--date".into(),
            "2026-13-01".into()
        ])
        .is_err());
        let a = claude_args(WindowId::Eu, NaiveDate::from_ymd_opt(2026, 11, 26).unwrap());
        assert_eq!(&a[..2], ["-p", "/macro-review eu 2026-11-26"]);
        assert!(a.contains(&MACRO_ALLOWED.join(",")));
    }

    #[test]
    fn a_due_run_reviews_with_the_cookie_in_the_env_and_logs_without_it() {
        let r = rig();
        let secrets = MemoryStore::default();
        crate::secrets::set_tv_session(&secrets, "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4").unwrap();
        let code = run(RunArgs {
            window: WindowId::Eu,
            date: None,
            now: at(9, 31),
            data_dir: Some(r.data.path().to_path_buf()),
            secrets: &secrets,
            claude: launch("ok", &r.out),
        });
        assert_eq!(code, 0);
        let rec: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&r.out).unwrap()).unwrap();
        assert_eq!(rec["argv"][1], "/macro-review eu 2026-11-26");
        assert_eq!(
            rec["session"], "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4",
            "the cookie travels in the env"
        );
        assert_eq!(
            std::fs::canonicalize(rec["cwd"].as_str().unwrap()).unwrap(),
            std::fs::canonicalize(r.vault.path()).unwrap()
        );
        let log =
            std::fs::read_to_string(paths::logs_dir(r.data.path()).join("run-2026-11-26-eu.log"))
                .unwrap();
        assert!(log.contains("Обзор готов."), "{log}");
        assert!(log.contains("TV_SESSIONID"), "the key is listed");
        assert!(
            !log.contains("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"),
            "the value never is"
        );
    }

    #[test]
    fn no_sessionid_is_exit_1_and_a_second_run_waits_its_turn() {
        let r = rig();
        let args = |secrets: &'static MemoryStore| RunArgs {
            window: WindowId::Eu,
            date: NaiveDate::from_ymd_opt(2026, 11, 26),
            now: at(9, 31),
            data_dir: Some(r.data.path().to_path_buf()),
            secrets,
            claude: launch("ok", &r.out),
        };
        static NONE: std::sync::LazyLock<MemoryStore> =
            std::sync::LazyLock::new(MemoryStore::default);
        static SOME: std::sync::LazyLock<MemoryStore> = std::sync::LazyLock::new(|| {
            let s = MemoryStore::default();
            let _ = crate::secrets::set_tv_session(&s, "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
            s
        });
        assert_eq!(run(args(&NONE)), 1);
        assert!(!r.out.exists(), "no review without TradingView data");
        let log =
            std::fs::read_to_string(paths::logs_dir(r.data.path()).join("run-2026-11-26-eu.log"))
                .unwrap();
        assert!(log.contains("no TradingView sessionid"), "{log}");
        // A fresh lock of another run: this one leaves without work.
        let lock = paths::logs_dir(r.data.path()).join("run.lock");
        std::fs::write(&lock, format!("999 {}", at(9, 30).timestamp())).unwrap();
        assert_eq!(run(args(&SOME)), 0);
        assert!(!r.out.exists());
        // A lock from yesterday is a crashed run's: taken over, and released after.
        std::fs::write(&lock, format!("999 {}", at(9, 30).timestamp() - 86_400)).unwrap();
        assert_eq!(run(args(&SOME)), 0);
        assert!(r.out.exists());
        assert!(!lock.exists());
    }

    #[test]
    fn not_its_minute_does_nothing() {
        let r = rig();
        let code = run(RunArgs {
            window: WindowId::Us,
            date: None,
            now: at(9, 31),
            data_dir: Some(r.data.path().to_path_buf()),
            secrets: &MemoryStore::default(),
            claude: launch("ok", &r.out),
        });
        assert_eq!(code, 0);
        assert!(!r.out.exists(), "claude was not started");
    }

    #[test]
    fn a_failed_review_and_a_missing_vault_exit_1() {
        let r = rig();
        let args = |mode: &str| RunArgs {
            window: WindowId::Review,
            date: NaiveDate::from_ymd_opt(2026, 11, 25),
            now: at(12, 0),
            data_dir: Some(r.data.path().to_path_buf()),
            secrets: &*WITH_SESSION,
            claude: launch(mode, &r.out),
        };
        static WITH_SESSION: std::sync::LazyLock<MemoryStore> = std::sync::LazyLock::new(|| {
            let s = MemoryStore::default();
            let _ = crate::secrets::set_tv_session(&s, "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4");
            s
        });
        assert_eq!(run(args("fail")), 1);
        assert_eq!(run(args("ok")), 0, "--date skips the minute check");
        let s = Settings {
            vault: Some(r.data.path().join("gone").to_string_lossy().into_owned()),
            ..Settings::default()
        };
        SettingsStore::new(r.data.path()).save(&s).unwrap();
        assert_eq!(run(args("ok")), 1);
    }
}
