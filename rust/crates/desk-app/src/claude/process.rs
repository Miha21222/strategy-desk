//! The `claude -p` child: spawn, user turns on stdin, stop, death.
//!
//! One supervisor task per process owns the child, reads stdout line by line
//! through the [`Translator`] and forwards [`DriverEvent`]s. A separate writer
//! task owns stdin, so a slow child can never block the supervisor (and with
//! it the reading of the child's own stdout).
//!
//! Lifecycle rules (docs/rust/CONTRACTS.md «Драйвер Claude»):
//! * `stop()` = interrupt the turn (stream-json control request); if its
//!   `result` does not come within `stop_grace`, kill the process. Either way
//!   the turn ends with `Done { Interrupted }`, no `Error`.
//! * `shutdown()` = close stdin (the CLI exits on EOF), kill after the grace.
//! * The process dies while a turn runs = `Error { chat.error.process_died }`
//!   then `Done { ProcessDied }`.
//! * When the process is gone the event channel closes.

use std::io;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::parse::{control_error_line, interrupt_line, truncate, user_line, Translator};
use super::{keys, ClaudeConfig, DriverEvent, StopReason};

/// How much of stderr is kept to explain a death.
const STDERR_TAIL: usize = 8 * 1024;
/// After the process exited, how long to wait for its last stdout bytes.
const DRAIN_AFTER_EXIT: Duration = Duration::from_millis(500);

/// The session's process is gone; start a new one (with `--resume`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the claude process is gone")]
pub struct SessionGone;

enum Ctl {
    Send(String),
    Stop,
    Shutdown,
}

/// Handle to one running `claude` process. Cheap to clone; every method is
/// non-blocking. Dropping every handle shuts the process down.
#[derive(Clone)]
pub struct ClaudeSession {
    ctl: mpsc::UnboundedSender<Ctl>,
    pid: Option<u32>,
}

impl ClaudeSession {
    /// Start the process. Must be called inside a tokio runtime.
    ///
    /// Fails only when the OS refuses to start the program (`NotFound` = Claude
    /// Code is not installed where we looked).
    pub fn spawn(cfg: &ClaudeConfig) -> io::Result<(Self, mpsc::UnboundedReceiver<DriverEvent>)> {
        let mut cmd = Command::new(&cfg.program);
        cmd.args(&cfg.program_args)
            .args(cfg.args())
            .current_dir(&cfg.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for k in &cfg.env_remove {
            cmd.env_remove(k);
        }
        cmd.envs(&cfg.env);
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW: no console flashing up behind the desk.
            cmd.creation_flags(0x0800_0000);
        }
        // Program and arg count only: never the env (sessionid).
        tracing::debug!(program = %cfg.program.display(), args = cfg.args().len(), "claude: spawning");
        let mut child = cmd.spawn()?;
        let pid = child.id();
        let io = (child.stdin.take(), child.stdout.take(), child.stderr.take());
        let (Some(stdin), Some(stdout), Some(stderr)) = io else {
            // Unreachable with Stdio::piped() on all three; handled anyway.
            return Err(io::Error::other("claude: child pipes missing"));
        };
        let (ctl_tx, ctl_rx) = mpsc::unbounded_channel();
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        tokio::spawn(supervise(
            child,
            stdin,
            stdout,
            stderr,
            ctl_rx,
            ev_tx,
            cfg.stop_grace,
        ));
        Ok((Self { ctl: ctl_tx, pid }, ev_rx))
    }

    /// Queue one user turn.
    pub fn send(&self, text: impl Into<String>) -> Result<(), SessionGone> {
        self.ctl
            .send(Ctl::Send(text.into()))
            .map_err(|_| SessionGone)
    }

    /// Interrupt the running turn (no-op when idle).
    pub fn stop(&self) -> Result<(), SessionGone> {
        self.ctl.send(Ctl::Stop).map_err(|_| SessionGone)
    }

    /// End the process: EOF on stdin, kill after the grace period.
    pub fn shutdown(&self) {
        // An Err here means the supervisor is already gone: nothing to stop.
        let _ = self.ctl.send(Ctl::Shutdown);
    }

    /// False once the supervisor finished (the process is gone).
    pub fn is_alive(&self) -> bool {
        !self.ctl.is_closed()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }
}

async fn writer(mut stdin: ChildStdin, mut rx: mpsc::UnboundedReceiver<String>) {
    while let Some(line) = rx.recv().await {
        if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
            // The child is going away; its stdout EOF tells the supervisor.
            break;
        }
    }
    // Dropping stdin here is the EOF that ends a `-p` session.
}

async fn stderr_tail(mut stderr: ChildStderr, tail: Arc<Mutex<Vec<u8>>>) {
    let mut buf = [0u8; 4096];
    loop {
        match stderr.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                // A poisoned lock only means another task panicked mid-push;
                // the bytes are still usable for a diagnostic.
                let mut t = tail.lock().unwrap_or_else(|e| e.into_inner());
                t.extend_from_slice(&buf[..n]);
                if t.len() > STDERR_TAIL {
                    let cut = t.len() - STDERR_TAIL;
                    t.drain(..cut);
                }
            }
        }
    }
}

struct State {
    tr: Translator,
    ev: mpsc::UnboundedSender<DriverEvent>,
    writer: Option<mpsc::UnboundedSender<String>>,
    /// Turns sent and not yet answered by a `result` (the CLI queues turns).
    pending: u32,
    stop_requested: bool,
    shutting_down: bool,
    killed: bool,
    kill_at: Option<Instant>,
    stop_seq: u64,
    scratch: Vec<DriverEvent>,
}

impl State {
    fn line(&mut self, bytes: &[u8]) {
        let line = String::from_utf8_lossy(bytes);
        let p = self.tr.feed(&line, &mut self.scratch);
        for e in self.scratch.drain(..) {
            // A dropped receiver only means nobody listens any more.
            let _ = self.ev.send(e);
        }
        if p.turn_done {
            self.pending = self.pending.saturating_sub(1);
            if self.stop_requested {
                self.stop_requested = false;
                if !self.shutting_down {
                    self.kill_at = None; // the interrupt worked; keep the process
                }
            }
        }
        if let Some(id) = p.control_request {
            if let Some(w) = &self.writer {
                let _ = w.send(control_error_line(&id));
            }
        }
    }

    fn emit(&self, e: DriverEvent) {
        let _ = self.ev.send(e);
    }
}

async fn supervise(
    mut child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut ctl: mpsc::UnboundedReceiver<Ctl>,
    ev: mpsc::UnboundedSender<DriverEvent>,
    grace: Duration,
) {
    let (wtx, wrx) = mpsc::unbounded_channel();
    tokio::spawn(writer(stdin, wrx));
    let tail = Arc::new(Mutex::new(Vec::new()));
    let tail_task = tokio::spawn(stderr_tail(stderr, Arc::clone(&tail)));

    let mut s = State {
        tr: Translator::new(),
        ev,
        writer: Some(wtx),
        pending: 0,
        stop_requested: false,
        shutting_down: false,
        killed: false,
        kill_at: None,
        stop_seq: 0,
        scratch: Vec::new(),
    };
    let mut reader = BufReader::new(stdout);
    let mut buf = Vec::new();
    let mut stdout_open = true;
    let mut ctl_open = true;

    let status: Option<ExitStatus> = loop {
        let kill_at = s.kill_at;
        tokio::select! {
            // `read_until` keeps partial bytes in `buf` when another branch
            // wins, so the next call continues the same line.
            r = reader.read_until(b'\n', &mut buf), if stdout_open => match r {
                Ok(0) | Err(_) => {
                    stdout_open = false;
                    // Nothing more can be heard from it: do not keep it around.
                    if s.kill_at.is_none() && !s.killed {
                        s.kill_at = Some(Instant::now() + grace);
                    }
                }
                Ok(_) => {
                    s.line(&buf);
                    buf.clear();
                }
            },
            c = ctl.recv(), if ctl_open => match c {
                Some(Ctl::Send(text)) => match &s.writer {
                    Some(w) if !s.shutting_down => {
                        let _ = w.send(user_line(&text));
                        s.pending += 1;
                    }
                    _ => {
                        // Sent after shutdown: answer it so the UI is not left waiting.
                        s.emit(DriverEvent::Error { key: keys::PROCESS_DIED.into(), detail: "shutting down".into() });
                        s.emit(DriverEvent::Done { stop: StopReason::ProcessDied });
                    }
                },
                Some(Ctl::Stop) => {
                    if s.pending > 0 && !s.stop_requested {
                        s.stop_seq += 1;
                        if let Some(w) = &s.writer {
                            let _ = w.send(interrupt_line(&format!("desk-stop-{}", s.stop_seq)));
                        }
                        s.tr.mark_interrupted();
                        s.stop_requested = true;
                        s.kill_at = Some(Instant::now() + grace);
                    }
                }
                Some(Ctl::Shutdown) | None => {
                    if c.is_none() {
                        ctl_open = false;
                    }
                    if !s.shutting_down {
                        s.shutting_down = true;
                        s.writer = None; // closes stdin once queued lines are written
                        s.kill_at = Some(Instant::now() + grace);
                    }
                }
            },
            _ = async { tokio::time::sleep_until(kill_at.unwrap_or_else(Instant::now)).await }, if kill_at.is_some() && !s.killed => {
                tracing::debug!("claude: grace period over, killing the process");
                let _ = child.start_kill();
                s.killed = true;
                s.kill_at = None;
            },
            st = child.wait() => break st.ok(),
        }
    };

    // The exit can overtake the last stdout lines (the final `result`).
    let drain_until = Instant::now() + DRAIN_AFTER_EXIT;
    while stdout_open {
        match tokio::time::timeout_at(drain_until, reader.read_until(b'\n', &mut buf)).await {
            Ok(Ok(n)) if n > 0 => {
                s.line(&buf);
                buf.clear();
            }
            _ => stdout_open = false,
        }
    }
    // stderr closes with the process; give it the same short window.
    let _ = tokio::time::timeout_at(drain_until, tail_task).await;

    let requested = s.stop_requested || s.shutting_down;
    let ok = status.is_some_and(|st| st.success());
    if s.pending > 0 {
        if requested {
            s.emit(DriverEvent::Done {
                stop: StopReason::Interrupted,
            });
        } else {
            s.emit(DriverEvent::Error {
                key: keys::PROCESS_DIED.into(),
                detail: death_detail(status, &tail),
            });
            s.emit(DriverEvent::Done {
                stop: StopReason::ProcessDied,
            });
        }
    } else if !ok && !requested && !s.killed {
        s.emit(DriverEvent::Error {
            key: keys::PROCESS_EXITED.into(),
            detail: death_detail(status, &tail),
        });
    }
    tracing::debug!(?status, "claude: process finished");
}

fn death_detail(status: Option<ExitStatus>, tail: &Mutex<Vec<u8>>) -> String {
    let st = status.map_or_else(|| "exit status unknown".to_string(), |s| s.to_string());
    let t = tail.lock().unwrap_or_else(|e| e.into_inner());
    let text = String::from_utf8_lossy(&t);
    let text = text.trim();
    if text.is_empty() {
        st
    } else {
        // Keep the END of stderr: the last line is the one that explains.
        let chars: Vec<char> = text.chars().collect();
        let start = chars.len().saturating_sub(2000);
        let end: String = chars[start..].iter().collect();
        format!("{st}\n{}", truncate(&end, 2000))
    }
}

#[cfg(test)]
mod tests {
    //! The fake `claude` is this very test binary, re-run with
    //! `--exact claude::process::tests::fake_claude_entry`: works on Linux and
    //! Windows with no script interpreter. libtest prints its «running 1 test»
    //! banner on stdout first, which doubles as a junk line the driver skips.

    use super::*;
    use std::collections::BTreeMap;
    use std::io::{BufRead, Write};
    use std::path::PathBuf;
    use tokio::time::timeout;

    const FAKE_MODE: &str = "DESK_FAKE_CLAUDE";
    const FAKE_FIXTURE: &str = "DESK_FAKE_FIXTURE";
    const FAKE_OUT: &str = "DESK_FAKE_OUT";
    const T: Duration = Duration::from_secs(20);

    #[test]
    fn fake_claude_entry() {
        if let Ok(mode) = std::env::var(FAKE_MODE) {
            fake_main(&mode);
        }
    }

    fn fake_main(mode: &str) -> ! {
        let fixture = std::env::var(FAKE_FIXTURE)
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        let lines: Vec<&str> = fixture.lines().filter(|l| !l.trim().is_empty()).collect();
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let mut out = std::io::stdout().lock();
        let mut emit = |l: &str| {
            let _ = writeln!(out, "{l}");
            let _ = out.flush();
            std::thread::sleep(Duration::from_millis(2));
        };
        let mut read = |want: &str| -> Option<String> {
            loop {
                let mut s = String::new();
                match input.read_line(&mut s) {
                    Ok(0) | Err(_) => return None,
                    Ok(_) if s.contains(want) => return Some(s),
                    Ok(_) => {}
                }
            }
        };
        let exit = |code: i32| -> ! { std::process::exit(code) };
        match mode {
            "replay" => {
                // One fixture turn (up to its `result`) per user line.
                let mut turns: Vec<Vec<&str>> = vec![Vec::new()];
                for l in &lines {
                    if let Some(t) = turns.last_mut() {
                        t.push(l);
                    }
                    if l.contains(r#""type":"result""#) {
                        turns.push(Vec::new());
                    }
                }
                for t in turns.iter().filter(|t| !t.is_empty()) {
                    if read(r#""type":"user""#).is_none() {
                        exit(0);
                    }
                    t.iter().for_each(|l| emit(l));
                }
                let _ = read("\u{0}never");
                exit(0)
            }
            "die_mid" => {
                if read(r#""type":"user""#).is_none() {
                    exit(0);
                }
                lines.iter().take(lines.len() / 2).for_each(|l| emit(l));
                eprintln!("fatal: simulated crash");
                exit(3)
            }
            "interrupt" => {
                if read(r#""type":"user""#).is_none() {
                    exit(0);
                }
                lines.iter().take(4).for_each(|l| emit(l));
                if read(r#""subtype":"interrupt""#).is_none() {
                    exit(0);
                }
                lines.iter().skip(4).for_each(|l| emit(l));
                let _ = read("\u{0}never");
                exit(0)
            }
            "deaf" => {
                let _ = read(r#""type":"user""#);
                lines.iter().take(4).for_each(|l| emit(l));
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            "control" => {
                if read(r#""type":"user""#).is_none() {
                    exit(0);
                }
                emit(
                    r#"{"type":"control_request","request_id":"cr-1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{}}}"#,
                );
                let reply = read(r#""type":"control_response""#).unwrap_or_default();
                if !reply.contains(r#""request_id":"cr-1""#)
                    || !reply.contains(r#""subtype":"error""#)
                {
                    exit(9);
                }
                lines.iter().for_each(|l| emit(l));
                let _ = read("\u{0}never");
                exit(0)
            }
            "args" => {
                let argv: Vec<String> =
                    std::env::args().skip_while(|a| a != "--").skip(1).collect();
                let env: BTreeMap<String, String> = ["DESK_PROBE", "CARGO_MANIFEST_DIR"]
                    .iter()
                    .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
                    .collect();
                let cwd = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let report = serde_json::json!({ "argv": argv, "env": env, "cwd": cwd });
                if let Ok(p) = std::env::var(FAKE_OUT) {
                    let _ = std::fs::write(p, report.to_string());
                }
                exit(0)
            }
            _ => exit(2),
        }
    }

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/claude/testdata")
            .join(name)
    }

    fn fake(mode: &str, fixture: Option<PathBuf>) -> ClaudeConfig {
        let exe = std::env::current_exe().expect("test exe path");
        let mut cfg = ClaudeConfig::new(exe, std::env::temp_dir());
        cfg.program_args = [
            "--exact",
            "claude::process::tests::fake_claude_entry",
            "--nocapture",
            "--test-threads=1",
            "-q",
            "--",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        cfg.env.insert(FAKE_MODE.into(), mode.into());
        if let Some(f) = fixture {
            cfg.env.insert(FAKE_FIXTURE.into(), f.display().to_string());
        }
        cfg.stop_grace = Duration::from_millis(300);
        cfg
    }

    fn expected(fixture: &str) -> Vec<DriverEvent> {
        let mut t = Translator::new();
        let mut out = Vec::new();
        for l in std::fs::read_to_string(testdata(fixture))
            .expect("fixture")
            .lines()
        {
            t.feed(l, &mut out);
        }
        out
    }

    /// Events up to and including the next `Done`.
    async fn turn(rx: &mut mpsc::UnboundedReceiver<DriverEvent>) -> Vec<DriverEvent> {
        let mut v = Vec::new();
        loop {
            let e = timeout(T, rx.recv())
                .await
                .expect("timed out waiting for Done")
                .expect("channel closed before Done");
            let done = matches!(e, DriverEvent::Done { .. });
            v.push(e);
            if done {
                return v;
            }
        }
    }

    /// Everything left until the channel closes.
    async fn rest(rx: &mut mpsc::UnboundedReceiver<DriverEvent>) -> Vec<DriverEvent> {
        let mut v = Vec::new();
        while let Some(e) = timeout(T, rx.recv())
            .await
            .expect("timed out waiting for close")
        {
            v.push(e);
        }
        v
    }

    #[tokio::test]
    async fn replays_a_turn_and_shuts_down_cleanly() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("replay", Some(testdata("text_turn.jsonl")))).unwrap();
        s.send("привет").unwrap();
        assert_eq!(turn(&mut rx).await, expected("text_turn.jsonl"));
        s.shutdown();
        assert_eq!(rest(&mut rx).await, vec![]);
        assert!(!s.is_alive());
        assert_eq!(s.send("again"), Err(SessionGone));
    }

    #[tokio::test]
    async fn two_turns_in_one_process() {
        let dir = tempfile::tempdir().unwrap();
        let both = dir.path().join("both.jsonl");
        let text = std::fs::read_to_string(testdata("text_turn.jsonl")).unwrap()
            + &std::fs::read_to_string(testdata("tool_turn.jsonl")).unwrap();
        std::fs::write(&both, text).unwrap();
        let (s, mut rx) = ClaudeSession::spawn(&fake("replay", Some(both))).unwrap();
        s.send("one").unwrap();
        let first = turn(&mut rx).await;
        assert_eq!(first, expected("text_turn.jsonl"));
        s.send("two").unwrap();
        let second = turn(&mut rx).await;
        // The second fixture has its own session id, so it starts with a
        // SessionStarted of its own, exactly like a fresh translator.
        assert_eq!(second, expected("tool_turn.jsonl"));
        drop(s);
        assert_eq!(rest(&mut rx).await, vec![]);
    }

    #[tokio::test]
    async fn death_mid_answer_is_error_then_done() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("die_mid", Some(testdata("tool_turn.jsonl")))).unwrap();
        s.send("go").unwrap();
        let ev = turn(&mut rx).await;
        let n = ev.len();
        assert!(
            matches!(&ev[n - 2], DriverEvent::Error { key, detail } if key == keys::PROCESS_DIED && detail.contains("simulated crash")),
            "{ev:?}"
        );
        assert_eq!(
            ev[n - 1],
            DriverEvent::Done {
                stop: StopReason::ProcessDied
            }
        );
        assert!(ev.iter().any(|e| matches!(e, DriverEvent::ToolCall { .. })));
        assert_eq!(rest(&mut rx).await, vec![]);
    }

    #[tokio::test]
    async fn stop_interrupts_and_keeps_the_process() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("interrupt", Some(testdata("interrupted.jsonl")))).unwrap();
        s.send("long review").unwrap();
        // Wait for the first delta, then press «Стоп».
        loop {
            let e = timeout(T, rx.recv()).await.unwrap().unwrap();
            if matches!(e, DriverEvent::TextDelta { .. }) {
                break;
            }
        }
        s.stop().unwrap();
        let ev = turn(&mut rx).await;
        assert_eq!(
            ev.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::Interrupted
            })
        );
        assert!(
            !ev.iter().any(|e| matches!(e, DriverEvent::Error { .. })),
            "{ev:?}"
        );
        // Longer than the grace: the process was not killed.
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert!(s.is_alive());
        s.shutdown();
        assert_eq!(rest(&mut rx).await, vec![]);
    }

    #[tokio::test]
    async fn stop_kills_a_process_that_does_not_answer() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("deaf", Some(testdata("interrupted.jsonl")))).unwrap();
        s.send("x").unwrap();
        loop {
            let e = timeout(T, rx.recv()).await.unwrap().unwrap();
            if matches!(e, DriverEvent::TextDelta { .. }) {
                break;
            }
        }
        let t0 = std::time::Instant::now();
        s.stop().unwrap();
        let ev = turn(&mut rx).await;
        assert_eq!(
            ev,
            vec![DriverEvent::Done {
                stop: StopReason::Interrupted
            }]
        );
        assert!(t0.elapsed() < Duration::from_secs(5), "{:?}", t0.elapsed());
        assert_eq!(rest(&mut rx).await, vec![]);
    }

    #[tokio::test]
    async fn shutdown_kills_a_process_that_ignores_eof() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("deaf", Some(testdata("interrupted.jsonl")))).unwrap();
        s.send("x").unwrap();
        s.shutdown();
        let ev = rest(&mut rx).await;
        assert_eq!(
            ev.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::Interrupted
            })
        );
        assert!(!ev.iter().any(|e| matches!(e, DriverEvent::Error { .. })));
    }

    #[tokio::test]
    async fn control_requests_are_refused_not_left_hanging() {
        let (s, mut rx) =
            ClaudeSession::spawn(&fake("control", Some(testdata("text_turn.jsonl")))).unwrap();
        s.send("x").unwrap();
        assert_eq!(
            turn(&mut rx).await.last(),
            Some(&DriverEvent::Done {
                stop: StopReason::EndTurn
            })
        );
        s.shutdown();
        assert_eq!(rest(&mut rx).await, vec![]);
    }

    #[tokio::test]
    async fn passes_flags_env_and_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("report.json");
        let mut cfg = fake("args", None);
        cfg.cwd = dir.path().to_path_buf();
        cfg.resume = Some("sess-42".into());
        cfg.append_system_prompt = Some("Отвечай по-русски.".into());
        cfg.env.insert(FAKE_OUT.into(), out.display().to_string());
        cfg.env.insert("DESK_PROBE".into(), "значение".into());
        let parent_has_manifest = std::env::var("CARGO_MANIFEST_DIR").is_ok();
        cfg.env_remove.push("CARGO_MANIFEST_DIR".into());
        let (_s, mut rx) = ClaudeSession::spawn(&cfg).unwrap();
        // Exit 0 while idle: no events at all.
        assert_eq!(rest(&mut rx).await, vec![]);
        let report: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        let argv: Vec<String> = serde_json::from_value(report["argv"].clone()).unwrap();
        assert_eq!(argv, cfg.args());
        assert!(argv.contains(&"--resume=sess-42".to_string()));
        assert_eq!(report["env"]["DESK_PROBE"], "значение");
        if parent_has_manifest {
            assert!(report["env"].get("CARGO_MANIFEST_DIR").is_none());
        }
        let cwd = std::fs::canonicalize(report["cwd"].as_str().unwrap()).unwrap();
        assert_eq!(cwd, std::fs::canonicalize(dir.path()).unwrap());
    }

    #[tokio::test]
    async fn idle_death_with_failure_is_reported() {
        // An unknown mode makes the fake exit 2 before any turn.
        let (_s, mut rx) = ClaudeSession::spawn(&fake("no-such-mode", None)).unwrap();
        let ev = rest(&mut rx).await;
        assert!(
            matches!(ev.as_slice(), [DriverEvent::Error { key, .. }] if key == keys::PROCESS_EXITED),
            "{ev:?}"
        );
    }

    #[tokio::test]
    async fn missing_program_is_not_found() {
        let cfg = ClaudeConfig::new(
            "/definitely/not/here/claude-desk-test",
            std::env::temp_dir(),
        );
        let err = ClaudeSession::spawn(&cfg).err().expect("spawn must fail");
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
