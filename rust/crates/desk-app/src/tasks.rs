//! Long operations the desk does FOR the owner, each behind one press —
//! port of `src/shared/tasks.ts` + `src/main/tasks.ts` without the course
//! knowledge base, the official MCP and Codex.
//!
//! The rules the old app kept, structurally:
//! * THE UI NAMES A TASK ID, NEVER A COMMAND. Every command is built here.
//! * Nothing runs at launch. A task starts from a button; the command it
//!   runs is shown (`TaskState::command`) and its output streams line by line.
//! * stdin is closed for every task but `claude_login`, which waits for the
//!   code the login page shows ([`Ctl::Input`]).

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use crate::core_wip::Settings;
use crate::macro_env::{macro_env_vars, MacroEnvInput, Platform, SESSION_CHECK_CODE};
use crate::runtime::{provision_spec, runtime_env, tools_python, RuntimePaths, SpawnSpec};
use crate::scheduler;

/// The closed list (CONTRACTS «… (этап 6)»).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    pub const ALL: [TaskId; 7] = [
        TaskId::ClaudeInstall,
        TaskId::ClaudeLogin,
        TaskId::ProvisionRuntime,
        TaskId::VaultCreate,
        TaskId::SessionCheck,
        TaskId::SchedulerRegister,
        TaskId::SchedulerUnregister,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TaskId::ClaudeInstall => "claude_install",
            TaskId::ClaudeLogin => "claude_login",
            TaskId::ProvisionRuntime => "provision_runtime",
            TaskId::VaultCreate => "vault_create",
            TaskId::SessionCheck => "session_check",
            TaskId::SchedulerRegister => "scheduler_register",
            TaskId::SchedulerUnregister => "scheduler_unregister",
        }
    }

    pub fn parse(s: &str) -> Option<TaskId> {
        TaskId::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// Generous: a cold `uv sync` downloads a Python and ~290 packages.
    pub fn timeout(self) -> Duration {
        let min = |m: u64| Duration::from_secs(m * 60);
        match self {
            TaskId::ClaudeInstall => min(15),
            TaskId::ClaudeLogin => min(10),
            TaskId::ProvisionRuntime => min(45),
            TaskId::VaultCreate => min(2),
            TaskId::SessionCheck => min(1),
            TaskId::SchedulerRegister => min(2),
            TaskId::SchedulerUnregister => min(1),
        }
    }
}

/// Output kept per task: a progress view, not a log file.
pub const MAX_TASK_LINES: usize = 300;
/// One displayed line, in characters.
pub const MAX_LINE_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TaskState {
    pub running: bool,
    /// `None` = never finished in this run of the desk.
    pub ok: Option<bool>,
    /// What runs, shown to the owner (never carries a secret).
    pub command: String,
    pub lines: VecDeque<String>,
    /// An i18n key (`task.refused.*`, `task.error.*`) or empty.
    pub error_key: String,
    /// Raw detail beside the key (an exit code, an OS error).
    pub error_detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TasksView {
    pub tasks: BTreeMap<TaskId, TaskState>,
}

impl TasksView {
    pub fn get(&self, id: TaskId) -> Option<&TaskState> {
        self.tasks.get(&id)
    }

    pub fn start(&mut self, id: TaskId, command: String) {
        self.tasks.insert(
            id,
            TaskState {
                running: true,
                command,
                ..TaskState::default()
            },
        );
    }

    /// `splitLines` + the cap.
    pub fn line(&mut self, id: TaskId, text: &str) {
        let t = self.tasks.entry(id).or_default();
        let l: String = text
            .replace('\r', "")
            .trim_end()
            .chars()
            .take(MAX_LINE_CHARS)
            .collect();
        if l.trim().is_empty() {
            return;
        }
        t.lines.push_back(l);
        while t.lines.len() > MAX_TASK_LINES {
            t.lines.pop_front();
        }
    }

    pub fn end(&mut self, id: TaskId, ok: bool, key: &str, detail: &str) {
        let t = self.tasks.entry(id).or_default();
        t.running = false;
        t.ok = Some(ok);
        t.error_key = key.into();
        t.error_detail = detail.chars().take(MAX_LINE_CHARS).collect();
    }

    pub fn any_running(&self) -> bool {
        self.tasks.values().any(|t| t.running)
    }
}

/// What to do after a clean finish (run by the app, which owns the state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum After {
    Nothing,
    WriteMarker,
    SessionVerified,
    ScheduleOn,
    ScheduleOff,
}

#[derive(Clone)]
pub enum Plan {
    Spawn {
        spec: SpawnSpec,
        /// What the owner sees; differs from `spec` only where the real
        /// argv is unreadable (PowerShell's -EncodedCommand).
        shown: String,
        env: BTreeMap<String, String>,
        cwd: PathBuf,
        stdin: bool,
        after: After,
    },
    /// Laid down in-process, file by file (the vault template).
    VaultCreate { target: PathBuf },
}

impl std::fmt::Debug for Plan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Env values may hold the sessionid: keys only.
            Plan::Spawn {
                shown,
                env,
                stdin,
                after,
                ..
            } => f
                .debug_struct("Spawn")
                .field("shown", shown)
                .field("env_keys", &env.keys().collect::<Vec<_>>())
                .field("stdin", stdin)
                .field("after", after)
                .finish(),
            Plan::VaultCreate { target } => f
                .debug_struct("VaultCreate")
                .field("target", target)
                .finish(),
        }
    }
}

impl Plan {
    pub fn shown(&self) -> String {
        match self {
            Plan::Spawn { shown, .. } => shown.clone(),
            Plan::VaultCreate { target } => format!(
                "copy the vault template -> \"{}\" (existing files are kept)",
                target.display()
            ),
        }
    }
}

/// Everything a plan may need; gathered by the app.
pub struct PlanCtx<'a> {
    pub settings: &'a Settings,
    pub vault: Option<&'a Path>,
    pub runtime: &'a RuntimePaths,
    pub claude: Option<&'a Path>,
    /// The stored sessionid (env only, never shown).
    pub tv_session: Option<&'a str>,
    /// `strategy-desk.exe`, for the scheduled jobs.
    pub exe: Option<&'a Path>,
    pub data_dir: &'a Path,
    pub platform: Platform,
    /// The built-in vault template has files.
    pub has_template: bool,
}

/// Official installers (TZ item 17: the wizard installs Claude Code itself).
pub const CLAUDE_INSTALL_WINDOWS: &str = "irm https://claude.ai/install.ps1 | iex";
pub const CLAUDE_INSTALL_UNIX: &str = "curl -fsSL https://claude.ai/install.sh | bash";

/// The plan of a task, or the i18n key of why it cannot run.
pub fn plan(id: TaskId, c: &PlanCtx<'_>) -> Result<Plan, &'static str> {
    let cwd = c.data_dir.to_path_buf();
    let windows = c.platform == Platform::Windows;
    match id {
        TaskId::ClaudeInstall => {
            let (spec, shown) = if windows {
                (
                    scheduler::powershell(CLAUDE_INSTALL_WINDOWS),
                    format!("powershell -Command \"{CLAUDE_INSTALL_WINDOWS}\""),
                )
            } else {
                let s = SpawnSpec {
                    program: "bash".into(),
                    args: vec!["-c".into(), CLAUDE_INSTALL_UNIX.into()],
                };
                let shown = s.display();
                (s, shown)
            };
            Ok(Plan::Spawn {
                spec,
                shown,
                env: BTreeMap::new(),
                cwd,
                stdin: false,
                after: After::Nothing,
            })
        }
        TaskId::ClaudeLogin => {
            let claude = c.claude.ok_or("task.refused.no_claude")?;
            let spec = SpawnSpec {
                program: claude.to_path_buf(),
                args: vec!["auth".into(), "login".into(), "--claudeai".into()],
            };
            Ok(Plan::Spawn {
                shown: spec.display(),
                spec,
                env: BTreeMap::new(),
                cwd,
                stdin: true,
                after: After::Nothing,
            })
        }
        TaskId::ProvisionRuntime => {
            let spec = provision_spec(c.runtime).ok_or("task.refused.no_payload")?;
            Ok(Plan::Spawn {
                shown: spec.display(),
                spec,
                env: runtime_env(c.runtime),
                cwd,
                stdin: false,
                after: After::WriteMarker,
            })
        }
        TaskId::VaultCreate => {
            if !c.has_template {
                return Err("task.refused.no_template");
            }
            let target = c.vault.ok_or("task.refused.no_vault")?;
            Ok(Plan::VaultCreate {
                target: target.to_path_buf(),
            })
        }
        TaskId::SessionCheck => {
            let session = c.tv_session.ok_or("task.refused.no_session")?;
            let python = tools_python(c.runtime, &c.settings.tvmcp_path, c.platform)
                .filter(|p| p.is_file())
                .ok_or("task.refused.no_python")?;
            let mut env = macro_env_vars(&MacroEnvInput {
                vault: c
                    .vault
                    .map(|v| v.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                python: python.to_string_lossy().into_owned(),
                claude: String::new(),
                session_id: Some(session.to_string()),
                cdp_port: c.settings.cdp_port,
                platform: c.platform,
            });
            env.extend(runtime_env(c.runtime));
            let spec = SpawnSpec {
                program: python,
                args: vec!["-c".into(), SESSION_CHECK_CODE.into()],
            };
            Ok(Plan::Spawn {
                shown: spec.display(),
                spec,
                env,
                cwd,
                stdin: false,
                after: After::SessionVerified,
            })
        }
        TaskId::SchedulerRegister | TaskId::SchedulerUnregister => {
            if !windows {
                return Err("task.refused.windows_only");
            }
            let (script, after) = if id == TaskId::SchedulerRegister {
                let exe = c.exe.ok_or("task.refused.no_exe")?;
                let today = chrono::Utc::now()
                    .with_timezone(&chrono_tz::Europe::Kyiv)
                    .date_naive();
                (
                    scheduler::register_script(exe, &scheduler::trigger_times(today, c.settings)),
                    After::ScheduleOn,
                )
            } else {
                (scheduler::unregister_script(), After::ScheduleOff)
            };
            let shown = format!("powershell -Command @'\n{script}\n'@");
            Ok(Plan::Spawn {
                spec: scheduler::powershell(&script),
                shown,
                env: BTreeMap::new(),
                cwd,
                stdin: false,
                after,
            })
        }
    }
}

// ---------------------------------------------------------------- the runner

/// What a running task reports back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskEvent {
    Line(TaskId, String),
    End {
        id: TaskId,
        ok: bool,
        key: &'static str,
        detail: String,
    },
}

pub type Forward = Arc<dyn Fn(TaskEvent) + Send + Sync>;

#[derive(Debug)]
pub enum Ctl {
    /// One line on stdin (only for a plan with `stdin: true`).
    Input(String),
    Kill,
}

/// Handle to a running task.
pub struct Running {
    ctl: mpsc::UnboundedSender<Ctl>,
}

impl Running {
    pub fn input(&self, line: &str) {
        let _ = self.ctl.send(Ctl::Input(line.trim().to_string()));
    }

    pub fn kill(&self) {
        let _ = self.ctl.send(Ctl::Kill);
    }
}

/// Start a planned task inside the tokio runtime. Never fails: a spawn error
/// comes back as an `End` event, because the UI has a place for that.
pub fn start(
    id: TaskId,
    plan: Plan,
    template: Vec<crate::vault_template::Entry<'static>>,
    forward: Forward,
) -> Running {
    let (tx, rx) = mpsc::unbounded_channel();
    match plan {
        Plan::VaultCreate { target } => {
            tokio::spawn(async move {
                let f = Arc::clone(&forward);
                let res = tokio::task::spawn_blocking(move || {
                    let push = |l: String| f(TaskEvent::Line(id, l));
                    crate::vault_template::copy(&template, &target, &mut { push })
                })
                .await;
                let end = match res {
                    Ok(Ok(_)) => TaskEvent::End {
                        id,
                        ok: true,
                        key: "",
                        detail: String::new(),
                    },
                    Ok(Err(e)) => TaskEvent::End {
                        id,
                        ok: false,
                        key: "task.error.failed",
                        detail: e.to_string(),
                    },
                    Err(e) => TaskEvent::End {
                        id,
                        ok: false,
                        key: "task.error.failed",
                        detail: e.to_string(),
                    },
                };
                forward(end);
            });
        }
        Plan::Spawn {
            spec,
            env,
            cwd,
            stdin,
            ..
        } => {
            tokio::spawn(supervise(id, spec, env, cwd, stdin, rx, forward));
        }
    }
    Running { ctl: tx }
}

async fn supervise(
    id: TaskId,
    spec: SpawnSpec,
    env: BTreeMap<String, String>,
    cwd: PathBuf,
    stdin: bool,
    mut ctl: mpsc::UnboundedReceiver<Ctl>,
    forward: Forward,
) {
    let _ = std::fs::create_dir_all(&cwd);
    let mut cmd = tokio::process::Command::new(&spec.program);
    cmd.args(&spec.args)
        .envs(&env)
        .current_dir(&cwd)
        .stdin(if stdin {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let key = if e.kind() == std::io::ErrorKind::NotFound {
                "task.error.not_found"
            } else {
                "task.error.spawn"
            };
            forward(TaskEvent::End {
                id,
                ok: false,
                key,
                detail: e.to_string(),
            });
            return;
        }
    };
    let mut child_in = child.stdin.take();
    let readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|r| {
        let f = Arc::clone(&forward);
        tokio::spawn(async move {
            // A chunk is not a line: `uv sync` output is read line by line,
            // and the last unterminated line is kept too (it is usually the
            // whole diagnosis of a failure).
            let mut lines = BufReader::new(r);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match lines.read_until(b'\n', &mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => f(TaskEvent::Line(
                        id,
                        String::from_utf8_lossy(&buf).into_owned(),
                    )),
                }
            }
        })
    })
    .collect();
    let deadline = tokio::time::sleep(id.timeout());
    tokio::pin!(deadline);
    let mut killed: Option<&'static str> = None;
    let status = loop {
        tokio::select! {
            st = child.wait() => break st.ok(),
            c = ctl.recv() => match c {
                Some(Ctl::Input(line)) => {
                    if let Some(w) = child_in.as_mut() {
                        let _ = w.write_all(format!("{line}\n").as_bytes()).await;
                        let _ = w.flush().await;
                    }
                }
                Some(Ctl::Kill) => {
                    killed = Some("task.error.stopped");
                    let _ = child.start_kill();
                }
                None => {}
            },
            _ = &mut deadline, if killed.is_none() => {
                killed = Some("task.error.timeout");
                let _ = child.start_kill();
            }
        }
    };
    for r in readers {
        let _ = tokio::time::timeout(Duration::from_secs(5), r).await;
    }
    let end = match (killed, status) {
        (Some(key), _) => TaskEvent::End {
            id,
            ok: false,
            key,
            detail: String::new(),
        },
        (None, Some(st)) if st.success() => TaskEvent::End {
            id,
            ok: true,
            key: "",
            detail: String::new(),
        },
        (None, st) => TaskEvent::End {
            id,
            ok: false,
            key: "task.error.exit",
            detail: st.map_or_else(|| "unknown exit".to_string(), |s| s.to_string()),
        },
    };
    forward(end);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        s: &'a Settings,
        rt: &'a RuntimePaths,
        data: &'a Path,
        platform: Platform,
    ) -> PlanCtx<'a> {
        PlanCtx {
            settings: s,
            vault: Some(Path::new("/v")),
            runtime: rt,
            claude: Some(Path::new("/bin/claude")),
            tv_session: Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"),
            exe: Some(Path::new(r"C:\SD\strategy-desk.exe")),
            data_dir: data,
            platform,
            has_template: true,
        }
    }

    #[test]
    fn ids_round_trip() {
        for t in TaskId::ALL {
            assert_eq!(TaskId::parse(t.as_str()), Some(t));
        }
        assert_eq!(TaskId::parse("rm -rf /"), None);
    }

    #[test]
    fn plans_per_task() {
        let s = Settings::default();
        let rt = RuntimePaths::under(Path::new("/d/py"), Some("/app/tvmcp".into()), None);
        let data = Path::new("/d");
        let c = ctx(&s, &rt, data, Platform::Posix);
        match plan(TaskId::ClaudeLogin, &c).unwrap() {
            Plan::Spawn { spec, stdin, .. } => {
                assert_eq!(spec.args, ["auth", "login", "--claudeai"]);
                assert!(stdin, "the login waits for the code");
            }
            p => panic!("{p:?}"),
        }
        for id in [TaskId::ClaudeInstall, TaskId::ProvisionRuntime] {
            assert!(
                matches!(plan(id, &c).unwrap(), Plan::Spawn { stdin: false, .. }),
                "{id:?}"
            );
        }
        assert_eq!(
            plan(TaskId::SchedulerRegister, &c).unwrap_err(),
            "task.refused.windows_only"
        );
        assert_eq!(
            plan(TaskId::SessionCheck, &c).unwrap_err(),
            "task.refused.no_python",
            "the venv does not exist here"
        );
        let ext = RuntimePaths::under(Path::new("/d/py"), None, None);
        let c2 = ctx(&s, &ext, data, Platform::Posix);
        assert_eq!(
            plan(TaskId::ProvisionRuntime, &c2).unwrap_err(),
            "task.refused.no_payload"
        );
        let mut c3 = ctx(&s, &rt, data, Platform::Posix);
        c3.has_template = false;
        assert_eq!(
            plan(TaskId::VaultCreate, &c3).unwrap_err(),
            "task.refused.no_template"
        );
        c3.claude = None;
        assert_eq!(
            plan(TaskId::ClaudeLogin, &c3).unwrap_err(),
            "task.refused.no_claude"
        );
    }

    #[test]
    fn scheduler_plans_on_windows_show_the_script_and_no_secret() {
        let s = Settings::default();
        let rt = RuntimePaths::under(Path::new("/d/py"), None, None);
        let c = ctx(&s, &rt, Path::new("/d"), Platform::Windows);
        let p = plan(TaskId::SchedulerRegister, &c).unwrap();
        let shown = p.shown();
        assert!(
            shown.contains("Register-ScheduledTask -TaskName 'Strategy Desk eu 0930'"),
            "{shown}"
        );
        assert!(!shown.contains("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"));
        assert!(!format!("{p:?}").contains("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"));
        match p {
            Plan::Spawn { spec, after, .. } => {
                assert_eq!(spec.args[4], "-EncodedCommand");
                assert_eq!(after, After::ScheduleOn);
            }
            p => panic!("{p:?}"),
        }
        match plan(TaskId::ClaudeInstall, &c).unwrap() {
            Plan::Spawn { shown, .. } => {
                assert!(shown.contains("irm https://claude.ai/install.ps1 | iex"))
            }
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn the_view_caps_lines() {
        let mut v = TasksView::default();
        v.start(TaskId::ProvisionRuntime, "uv sync".into());
        for i in 0..(MAX_TASK_LINES + 20) {
            v.line(TaskId::ProvisionRuntime, &format!("line {i}\r\n"));
        }
        v.line(TaskId::ProvisionRuntime, "   ");
        let t = v.get(TaskId::ProvisionRuntime).unwrap();
        assert_eq!(t.lines.len(), MAX_TASK_LINES);
        assert_eq!(t.lines.back().map(String::as_str), Some("line 319"));
        assert!(v.any_running());
        v.end(
            TaskId::ProvisionRuntime,
            false,
            "task.error.exit",
            "exit status: 2",
        );
        assert!(!v.any_running());
        assert_eq!(v.get(TaskId::ProvisionRuntime).unwrap().ok, Some(false));
    }

    #[tokio::test]
    async fn a_task_streams_lines_takes_input_and_ends() {
        // The test binary as the child: a «login» that echoes one stdin line.
        let exe = std::env::current_exe().unwrap();
        let data = tempfile::tempdir().unwrap();
        let mut env = BTreeMap::new();
        env.insert(FAKE.into(), "echo".into());
        let plan = Plan::Spawn {
            spec: SpawnSpec {
                program: exe,
                args: vec![
                    "--exact".into(),
                    "tasks::tests::fake_task_entry".into(),
                    "--nocapture".into(),
                    "-q".into(),
                ],
            },
            shown: "fake".into(),
            env,
            cwd: data.path().to_path_buf(),
            stdin: true,
            after: After::Nothing,
        };
        let (tx, mut rx) = mpsc::unbounded_channel();
        let running = start(
            TaskId::ClaudeLogin,
            plan,
            Vec::new(),
            Arc::new(move |e| {
                let _ = tx.send(e);
            }),
        );
        let mut lines = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(20), rx.recv())
                .await
                .unwrap()
                .unwrap()
            {
                TaskEvent::Line(_, l) => {
                    if l.contains("Paste code here") {
                        running.input(" CODE-123 ");
                    }
                    lines.push(l);
                }
                TaskEvent::End {
                    ok, key, detail, ..
                } => {
                    assert!(ok, "{key} {detail} {lines:?}");
                    break;
                }
            }
        }
        assert!(
            lines.iter().any(|l| l.contains("got CODE-123")),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l.contains("on stderr")), "{lines:?}");
    }

    #[tokio::test]
    async fn a_missing_program_and_a_kill_end_the_task() {
        let data = tempfile::tempdir().unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let fwd: Forward = Arc::new(move |e| {
            let _ = tx.send(e);
        });
        let missing = Plan::Spawn {
            spec: SpawnSpec {
                program: data.path().join("no-such-program"),
                args: vec![],
            },
            shown: String::new(),
            env: BTreeMap::new(),
            cwd: data.path().to_path_buf(),
            stdin: false,
            after: After::Nothing,
        };
        let _r = start(TaskId::ClaudeInstall, missing, Vec::new(), Arc::clone(&fwd));
        let end = tokio::time::timeout(Duration::from_secs(20), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(
                end,
                TaskEvent::End {
                    ok: false,
                    key: "task.error.not_found",
                    ..
                }
            ),
            "{end:?}"
        );

        let mut env = BTreeMap::new();
        env.insert(FAKE.into(), "hang".into());
        let hang = Plan::Spawn {
            spec: SpawnSpec {
                program: std::env::current_exe().unwrap(),
                args: vec![
                    "--exact".into(),
                    "tasks::tests::fake_task_entry".into(),
                    "--nocapture".into(),
                    "-q".into(),
                ],
            },
            shown: String::new(),
            env,
            cwd: data.path().to_path_buf(),
            stdin: false,
            after: After::Nothing,
        };
        let r = start(TaskId::ProvisionRuntime, hang, Vec::new(), fwd);
        r.kill();
        loop {
            match tokio::time::timeout(Duration::from_secs(20), rx.recv())
                .await
                .unwrap()
                .unwrap()
            {
                TaskEvent::End { ok, key, .. } => {
                    assert!(!ok);
                    assert_eq!(key, "task.error.stopped");
                    break;
                }
                TaskEvent::Line(..) => {}
            }
        }
    }

    const FAKE: &str = "DESK_TEST_FAKE_TASK";

    #[test]
    fn fake_task_entry() {
        use std::io::{BufRead, Write};
        let Ok(mode) = std::env::var(FAKE) else {
            return;
        };
        if mode == "hang" {
            std::thread::sleep(Duration::from_secs(60));
            std::process::exit(0);
        }
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "Opening the browser…");
        let _ = writeln!(out, "Paste code here if prompted > ");
        let _ = out.flush();
        let mut line = String::new();
        let _ = std::io::stdin().lock().read_line(&mut line);
        let _ = writeln!(out, "got {}", line.trim());
        let _ = out.flush();
        eprintln!("a warning on stderr");
        std::process::exit(0);
    }
}
