//! `AppHandle` / `Command` / `Snapshot` (CONTRACTS.md «desk-app ↔ desk-ui»).
//!
//! The UI never waits on anything: `send()` pushes onto an unbounded channel,
//! `snapshot()` clones a handful of `Arc`s under a read lock that is only ever
//! held for a field assignment. All work happens on a tokio runtime in one
//! background thread; disk reads go to the blocking pool.
//!
//! Only `OpenDay` / `Today` and the watcher are wired in this slice; every
//! other command answers with a `UserError` (`error.not_implemented`).

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use chrono::{NaiveDate, Utc};
use tokio::sync::mpsc;

use crate::core_stub::{DayAnswers, DayData, HumanRead, Region};
use crate::day::{self, DayInput};
use crate::watch::{Changed, StateWatcher, WatchOptions};

/// How many errors the snapshot keeps (oldest dropped first).
pub const ERRORS_KEPT: usize = 20;

pub type ChatId = String;

/// A chat command from the UI.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatCommand {
    Send { chat_id: ChatId, text: String },
    Stop { chat_id: ChatId },
    New { kind: ChatKind },
}

/// `kind` of a chat in the MVP: the day's macro chat, or «+ Новый чат».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatKind {
    Macro,
    Ask,
}

/// A task id from a fixed list — never a command line (rule of the old app).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    OpenDay(NaiveDate),
    Today,
    WriteHumanRead {
        date: NaiveDate,
        region: Region,
        read: HumanRead,
    },
    WriteDayAnswers {
        date: NaiveDate,
        answers: DayAnswers,
    },
    Chat(ChatCommand),
    CheckStatus,
    LaunchTradingView,
    RunTask(TaskId),
}

/// One day in the day strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayMark {
    pub date: NaiveDate,
    /// A window review exists (`scale_` / `macro_` file).
    pub window_review: bool,
    /// The 20:00 day review exists (`review_` file).
    pub day_review: bool,
}

/// TODO(stage 5): chats list, messages, the live turn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatsView {}

/// TODO(stage 6): TradingView / CDP / Claude lights.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatusLights {}

/// TODO(stage 6): running and finished tasks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TasksView {}

/// An error for the UI: an i18n key plus variables, never a sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserError {
    pub key: String,
    pub vars: Vec<(String, String)>,
}

impl UserError {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            vars: Vec::new(),
        }
    }

    pub fn var(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.vars.push((k.into(), v.into()));
        self
    }
}

/// The picture the UI draws. Cheap to clone: `Arc`s and a counter.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub day: Option<Arc<DayData>>,
    pub days_index: Arc<Vec<DayMark>>,
    pub chats: Arc<ChatsView>,
    pub status: Arc<StatusLights>,
    pub tasks: Arc<TasksView>,
    pub errors: Arc<Vec<UserError>>,
    /// Bumps on every change.
    pub rev: u64,
}

/// i18n keys this module produces.
pub mod keys {
    pub const NOT_IMPLEMENTED: &str = "error.not_implemented";
    pub const VAULT_NOT_SET: &str = "error.vault.not_set";
    pub const VAULT_READ_FAILED: &str = "error.vault.read_failed";
    pub const WATCH_FAILED: &str = "error.vault.watch_failed";
}

/// What the background starts with. TODO(stage 6): read from the store.
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub vault: Option<PathBuf>,
    pub watch: WatchOptions,
}

type Repaint = Arc<dyn Fn() + Send + Sync>;

struct Shared {
    snap: RwLock<Snapshot>,
    repaint: RwLock<Option<Repaint>>,
}

impl Shared {
    /// Apply a change and wake the UI. Callers only call this with a real
    /// change, so `rev` and the repaint move together.
    fn publish(&self, f: impl FnOnce(&mut Snapshot)) {
        {
            // A poisoned lock means a panic mid-assignment of Arcs; the
            // snapshot is still a valid value, keep serving it.
            let mut s = self.snap.write().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.rev += 1;
        }
        let cb = self
            .repaint
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(cb) = cb {
            cb();
        }
    }
}

enum Msg {
    Cmd(Command),
    Changed(Changed),
}

/// The UI's only door into desk-app.
pub struct AppHandle {
    tx: Option<mpsc::UnboundedSender<Msg>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl AppHandle {
    /// Start the background thread and its runtime.
    pub fn start(cfg: AppConfig) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            snap: RwLock::new(Snapshot::default()),
            repaint: RwLock::new(None),
        });
        let (tx, rx) = mpsc::unbounded_channel();
        // current_thread: the app's own work is small and mostly waiting;
        // blocking file reads go to tokio's blocking pool.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let actor = Actor {
            shared: Arc::clone(&shared),
            vault: cfg.vault,
            watch_opts: cfg.watch,
            tx: Some(tx.clone()),
            watcher: None,
            open: None,
            input: DayInput::new(),
            errors: Vec::new(),
        };
        let thread = std::thread::Builder::new()
            .name("desk-app".into())
            .spawn(move || {
                rt.block_on(actor.run(rx));
                // Child processes are `kill_on_drop`; give tasks a moment.
                rt.shutdown_timeout(std::time::Duration::from_secs(1));
            })?;
        Ok(Self {
            tx: Some(tx),
            shared,
            thread: Some(thread),
        })
    }

    /// Never blocks. After shutdown the command is dropped.
    pub fn send(&self, cmd: Command) {
        if let Some(tx) = &self.tx {
            if tx.send(Msg::Cmd(cmd)).is_err() {
                tracing::warn!("app: command after the background stopped");
            }
        }
    }

    /// The latest picture.
    pub fn snapshot(&self) -> Snapshot {
        self.shared
            .snap
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Called (from the background thread) after every `rev` change.
    pub fn set_repaint(&self, f: Box<dyn Fn() + Send + Sync>) {
        *self
            .shared
            .repaint
            .write()
            .unwrap_or_else(|e| e.into_inner()) = Some(Arc::from(f));
    }
}

impl Drop for AppHandle {
    fn drop(&mut self) {
        // Closing the channel ends the actor; the watcher's own sender is
        // dropped with it, so nothing else keeps the loop alive.
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Actor {
    shared: Arc<Shared>,
    vault: Option<PathBuf>,
    watch_opts: WatchOptions,
    /// Only until the watcher has its weak copy: a strong sender here
    /// would keep the loop alive after the UI dropped its handle.
    tx: Option<mpsc::UnboundedSender<Msg>>,
    watcher: Option<StateWatcher>,
    open: Option<NaiveDate>,
    input: DayInput,
    errors: Vec<UserError>,
}

impl Actor {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Msg>) {
        self.start_watcher();
        self.tx = None;
        self.refresh_index().await;
        while let Some(msg) = rx.recv().await {
            match msg {
                Msg::Cmd(c) => self.command(c).await,
                Msg::Changed(names) => self.changed(names).await,
            }
        }
        self.watcher = None;
    }

    fn start_watcher(&mut self) {
        let (Some(vault), Some(tx)) = (self.vault.clone(), self.tx.as_ref()) else {
            return;
        };
        let tx = tx.downgrade();
        match StateWatcher::start_with(&vault, self.watch_opts, move |names| {
            if let Some(tx) = tx.upgrade() {
                let _ = tx.send(Msg::Changed(names));
            }
        }) {
            Ok(w) => self.watcher = Some(w),
            Err(e) => self.error(UserError::new(keys::WATCH_FAILED).var("detail", e.to_string())),
        }
    }

    async fn command(&mut self, cmd: Command) {
        match cmd {
            Command::OpenDay(date) => self.open_day(date).await,
            Command::Today => {
                let today = Utc::now()
                    .with_timezone(&chrono_tz::Europe::Kyiv)
                    .date_naive();
                self.open_day(today).await;
            }
            // TODO(stage 4): vault_write::write_human_read / write_day_answers
            // in spawn_blocking; the watcher then redraws.
            Command::WriteHumanRead { .. } => self.todo("write_human_read"),
            Command::WriteDayAnswers { .. } => self.todo("write_day_answers"),
            // TODO(stage 5): claude::ClaudeSession per chat, events folded into ChatsView.
            Command::Chat(_) => self.todo("chat"),
            // TODO(stage 6): status probes (sysinfo, CDP with a 1.5 s timeout).
            Command::CheckStatus => self.todo("check_status"),
            // TODO(stage 6): tv-launch port.
            Command::LaunchTradingView => self.todo("launch_tradingview"),
            // TODO(stage 6): task runner.
            Command::RunTask(_) => self.todo("run_task"),
        }
    }

    fn todo(&mut self, what: &str) {
        self.error(UserError::new(keys::NOT_IMPLEMENTED).var("command", what));
    }

    fn error(&mut self, e: UserError) {
        self.errors.push(e);
        if self.errors.len() > ERRORS_KEPT {
            let cut = self.errors.len() - ERRORS_KEPT;
            self.errors.drain(..cut);
        }
        let errors = Arc::new(self.errors.clone());
        self.shared.publish(|s| s.errors = errors);
    }

    async fn open_day(&mut self, date: NaiveDate) {
        let Some(vault) = self.vault.clone() else {
            self.error(UserError::new(keys::VAULT_NOT_SET));
            return;
        };
        let res = tokio::task::spawn_blocking(move || day::read_day(&vault, date)).await;
        match res {
            Ok(Ok(input)) => {
                self.open = Some(date);
                self.input = input;
                let data = Arc::new(day::build(date, &self.input));
                self.shared.publish(|s| s.day = Some(data));
            }
            Ok(Err(e)) => {
                self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()))
            }
            Err(e) => {
                self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()))
            }
        }
    }

    async fn changed(&mut self, names: Changed) {
        let (Some(vault), Some(date)) = (self.vault.clone(), self.open) else {
            self.refresh_index().await;
            return;
        };
        let wanted: Vec<String> = names
            .iter()
            .filter(|n| day::concerns(n, date))
            .cloned()
            .collect();
        let index_moved = names.iter().any(|n| {
            n.starts_with("scale_") || n.starts_with("macro_") || n.starts_with("review_")
        });
        if !wanted.is_empty() {
            // Only the changed files are read again (TZ item 12).
            let mut input = std::mem::take(&mut self.input);
            let res = tokio::task::spawn_blocking(move || {
                let mut moved = false;
                for n in &wanted {
                    moved |= day::reread_state_file(&vault, n, &mut input);
                }
                (input, moved)
            })
            .await;
            match res {
                Ok((input, moved)) => {
                    self.input = input;
                    // The day may have changed while the read ran: only a
                    // still-open day is published.
                    if moved && self.open == Some(date) {
                        let data = Arc::new(day::build(date, &self.input));
                        self.shared.publish(|s| s.day = Some(data));
                    }
                }
                Err(e) => {
                    self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()))
                }
            }
        }
        if index_moved {
            self.refresh_index().await;
        }
    }

    async fn refresh_index(&mut self) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        match tokio::task::spawn_blocking(move || day::list_days(&vault)).await {
            Ok(Ok(days)) => {
                let marks: Vec<DayMark> = days
                    .into_iter()
                    .map(|(date, window_review, day_review)| DayMark {
                        date,
                        window_review,
                        day_review,
                    })
                    .collect();
                let unchanged = *self
                    .shared
                    .snap
                    .read()
                    .unwrap_or_else(|e| e.into_inner())
                    .days_index
                    == marks;
                if !unchanged {
                    let marks = Arc::new(marks);
                    self.shared.publish(|s| s.days_index = marks);
                }
            }
            Ok(Err(e)) => {
                self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()))
            }
            Err(e) => {
                self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault_write::state_dir;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    fn fast() -> WatchOptions {
        WatchOptions {
            debounce: Duration::from_millis(100),
            max_wait: Duration::from_millis(500),
            recheck: Duration::from_millis(200),
        }
    }

    fn wait_until(app: &AppHandle, what: &str, f: impl Fn(&Snapshot) -> bool) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let s = app.snapshot();
            if f(&s) {
                return s;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: rev {}",
                s.rev
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn file<'a>(s: &'a Snapshot, key: &str) -> Option<&'a serde_json::Value> {
        s.day.as_ref()?.get("files")?.get(key)
    }

    #[test]
    fn opens_a_day_follows_the_watcher_and_repaints_per_rev() {
        let vault = tempfile::tempdir().unwrap();
        let sd = state_dir(vault.path());
        fs::create_dir_all(&sd).unwrap();
        fs::write(sd.join("scale_2026-11-25_eu.json"), r#"{"v":1}"#).unwrap();
        fs::write(sd.join("review_2026-11-24.json"), "{}").unwrap();

        let app = AppHandle::start(AppConfig {
            vault: Some(vault.path().into()),
            watch: fast(),
        })
        .unwrap();
        let repaints = Arc::new(AtomicU64::new(0));
        let r = Arc::clone(&repaints);
        app.set_repaint(Box::new(move || {
            r.fetch_add(1, Ordering::SeqCst);
        }));
        let s0 = wait_until(&app, "the index", |s| s.days_index.len() == 2);
        assert_eq!(
            s0.days_index[0],
            DayMark {
                date: d("2026-11-25"),
                window_review: true,
                day_review: false
            }
        );

        let before = app.snapshot().rev;
        let repaints_before = repaints.load(Ordering::SeqCst);
        app.send(Command::OpenDay(d("2026-11-25")));
        let s1 = wait_until(&app, "the day", |s| s.day.is_some());
        assert_eq!(s1.day.as_ref().unwrap()["date"], "2026-11-25");
        assert_eq!(
            file(&s1, "indices-kb/state/scale_2026-11-25_eu.json").unwrap()["v"],
            1
        );

        // The skill rewrites a file: only that file is read again.
        fs::write(sd.join("scale_2026-11-25_eu.json"), r#"{"v":2}"#).unwrap();
        fs::write(
            sd.join("human_read_2026-11-25_eu.json"),
            r#"{"side":"LONG"}"#,
        )
        .unwrap();
        let s2 = wait_until(&app, "the change", |s| {
            file(s, "indices-kb/state/scale_2026-11-25_eu.json").is_some_and(|v| v["v"] == 2)
                && file(s, "indices-kb/state/human_read_2026-11-25_eu.json").is_some()
        });

        // A touch with the same bytes and another day's file: no new rev.
        fs::write(sd.join("scale_2026-11-25_eu.json"), r#"{"v":2}"#).unwrap();
        fs::write(sd.join("drift_2026-11-20_eu.json"), "{}").unwrap();
        std::thread::sleep(Duration::from_millis(800));
        let s3 = app.snapshot();
        assert_eq!(s3.rev, s2.rev, "no change, no rev");
        assert_eq!(
            repaints.load(Ordering::SeqCst) - repaints_before,
            s3.rev - before,
            "one repaint per rev"
        );
        drop(app);
    }

    #[test]
    fn today_and_missing_vault() {
        let app = AppHandle::start(AppConfig::default()).unwrap();
        app.send(Command::Today);
        let s = wait_until(&app, "an error", |s| !s.errors.is_empty());
        assert_eq!(s.errors[0], UserError::new(keys::VAULT_NOT_SET));
        assert!(s.day.is_none());

        let vault = tempfile::tempdir().unwrap();
        let app = AppHandle::start(AppConfig {
            vault: Some(vault.path().into()),
            watch: fast(),
        })
        .unwrap();
        app.send(Command::Today);
        let s = wait_until(&app, "today", |s| s.day.is_some());
        let today = Utc::now()
            .with_timezone(&chrono_tz::Europe::Kyiv)
            .date_naive();
        // Midnight in Kyiv between the two lines would flake; compare loosely.
        let shown = d(s.day.as_ref().unwrap()["date"].as_str().unwrap());
        assert!((shown - today).num_days().abs() <= 1);
        assert!(s.errors.is_empty());
    }

    #[test]
    fn unwired_commands_answer_with_a_user_error() {
        let app = AppHandle::start(AppConfig::default()).unwrap();
        app.send(Command::Chat(ChatCommand::Send {
            chat_id: "c1".into(),
            text: "hi".into(),
        }));
        app.send(Command::CheckStatus);
        app.send(Command::RunTask(TaskId("provision-runtime".into())));
        let s = wait_until(&app, "three errors", |s| s.errors.len() == 3);
        assert!(s.errors.iter().all(|e| e.key == keys::NOT_IMPLEMENTED));
        assert_eq!(
            s.errors[0].vars,
            vec![("command".to_string(), "chat".to_string())]
        );
        for _ in 0..(ERRORS_KEPT + 5) {
            app.send(Command::LaunchTradingView);
        }
        wait_until(&app, "the cap", |s| {
            s.errors
                .last()
                .is_some_and(|e| e.vars[0].1 == "launch_tradingview")
                && s.errors.len() == ERRORS_KEPT
        });
    }

    #[test]
    fn send_never_blocks_and_snapshot_is_cheap() {
        let app = AppHandle::start(AppConfig::default()).unwrap();
        let t0 = Instant::now();
        for _ in 0..10_000 {
            app.send(Command::CheckStatus);
        }
        for _ in 0..10_000 {
            let _ = app.snapshot();
        }
        assert!(t0.elapsed() < Duration::from_secs(2), "{:?}", t0.elapsed());
    }
}
