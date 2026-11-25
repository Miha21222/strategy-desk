//! `AppHandle` / `Command` / `Snapshot` (CONTRACTS.md «desk-app ↔ desk-ui»).
//!
//! The UI never waits on anything: `send()` pushes onto an unbounded channel,
//! `snapshot()` clones a handful of `Arc`s under a read lock that is only ever
//! held for a field assignment. All work happens on a tokio runtime in one
//! background thread; disk reads and `load_day` run on the blocking pool.
//!
//! Wired in this slice: `OpenDay` / `Today` (desk-core's `load_day` over the
//! files of `day_files`), the watcher (only the changed file is read again),
//! the day strip (`MacroIndex`) and `LoadShot`. Every other command answers
//! with a `UserError` (`error.not_implemented`).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use chrono::{NaiveDate, Utc};
use desk_core::day::load_day;
use desk_core::vault::{DayAnswers, HumanRead};
use desk_core::{Day, DayInput, MacroIndex, Region};
use tokio::sync::mpsc;

use crate::day::{self, DayMark};
use crate::shots::{self, LoadedShot, ShotRefused};
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
    /// Read a screenshot the open day names (vault-relative, a `MacroShot.path`)
    /// into `Snapshot::shots`, off the UI thread.
    LoadShot(String),
    Chat(ChatCommand),
    CheckStatus,
    LaunchTradingView,
    RunTask(TaskId),
}

/// A screenshot as the UI may find it in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotEntry {
    Ready(Arc<LoadedShot>),
    /// Not shown (missing, not a PNG, outside `indices-kb/shots/`); the tile stays empty.
    Refused(ShotRefused),
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
    pub day: Option<Arc<Day>>,
    pub days_index: Arc<Vec<DayMark>>,
    /// The `MacroIndex` behind `days_index`: `ok` / `error` (`no-vault`,
    /// `no-state-dir`) for the empty-strip message.
    pub index: Option<Arc<MacroIndex>>,
    /// Screenshots asked for with `LoadShot`, by vault-relative path. Only
    /// the open day's are kept.
    pub shots: Arc<BTreeMap<String, ShotEntry>>,
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

    fn read<T>(&self, f: impl FnOnce(&Snapshot) -> T) -> T {
        f(&self.snap.read().unwrap_or_else(|e| e.into_inner()))
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
            keys: HashSet::new(),
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
        self.shared.read(Snapshot::clone)
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
        // Closing the channel ends the actor (the watcher only holds a weak
        // sender), then the thread is joined.
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
    /// The open day's files, kept so a change re-reads one file only.
    input: DayInput,
    /// `day_files(open)` as a set.
    keys: HashSet<String>,
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
            Command::LoadShot(rel) => self.load_shot(rel).await,
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

    fn read_failed(&mut self, e: impl std::fmt::Display) {
        self.error(UserError::new(keys::VAULT_READ_FAILED).var("detail", e.to_string()));
    }

    /// Publish a day, keeping only the screenshots it still names.
    fn publish_day(&self, day: Day) {
        let paths: HashSet<String> = day.shot_paths().into_iter().map(str::to_owned).collect();
        let shots = self.shared.read(|s| Arc::clone(&s.shots));
        let kept: BTreeMap<String, ShotEntry> = shots
            .iter()
            .filter(|(k, _)| paths.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let shots = (kept.len() != shots.len()).then(|| Arc::new(kept));
        let day = Arc::new(day);
        self.shared.publish(|s| {
            s.day = Some(day);
            if let Some(shots) = shots {
                s.shots = shots;
            }
        });
    }

    async fn open_day(&mut self, date: NaiveDate) {
        let Some(vault) = self.vault.clone() else {
            self.error(UserError::new(keys::VAULT_NOT_SET));
            return;
        };
        let res = tokio::task::spawn_blocking(move || {
            let input = day::read_day(&vault, date);
            let built = load_day(&input, date);
            (input, built)
        })
        .await;
        match res {
            Ok((input, built)) => {
                self.open = Some(date);
                self.input = input;
                self.keys = day::day_keys(date);
                self.publish_day(built);
            }
            Err(e) => self.read_failed(e),
        }
    }

    async fn changed(&mut self, names: Changed) {
        let index_moved = names.iter().any(|n| {
            n.starts_with("scale_") || n.starts_with("macro_") || n.starts_with("review_")
        });
        if let (Some(vault), Some(date)) = (self.vault.clone(), self.open) {
            let wanted: Vec<String> = names
                .iter()
                .map(|n| day::state_key(n))
                .filter(|k| self.keys.contains(k))
                .collect();
            if !wanted.is_empty() {
                // Only the changed files are read again (TZ item 12); the
                // day is rebuilt from the stored input.
                let mut input = std::mem::take(&mut self.input);
                let res = tokio::task::spawn_blocking(move || {
                    let mut moved = false;
                    for k in &wanted {
                        moved |= day::reread(&vault, k, &mut input);
                    }
                    let built = moved.then(|| load_day(&input, date));
                    (input, built)
                })
                .await;
                match res {
                    Ok((input, built)) => {
                        self.input = input;
                        // Only the day still open is published (commands are
                        // handled in order, so this always holds today; kept
                        // for when reads stop blocking the loop).
                        if let (Some(built), true) = (built, self.open == Some(date)) {
                            self.publish_day(built);
                        }
                    }
                    Err(e) => {
                        // The input went with the failed task: read the day again.
                        self.read_failed(e);
                        self.open_day(date).await;
                    }
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
        match tokio::task::spawn_blocking(move || day::read_index(&vault)).await {
            Ok((index, marks)) => {
                let unchanged = self
                    .shared
                    .read(|s| *s.days_index == marks && s.index.as_deref() == Some(&index));
                if !unchanged {
                    let (marks, index) = (Arc::new(marks), Arc::new(index));
                    self.shared.publish(|s| {
                        s.days_index = marks;
                        s.index = Some(index);
                    });
                }
            }
            Err(e) => self.read_failed(e),
        }
    }

    async fn load_shot(&mut self, rel: String) {
        let Some(vault) = self.vault.clone() else {
            self.error(UserError::new(keys::VAULT_NOT_SET));
            return;
        };
        let key = rel.replace('\\', "/");
        let res = tokio::task::spawn_blocking(move || shots::load_shot(&vault, &rel)).await;
        let entry = match res {
            Ok(Ok(s)) => ShotEntry::Ready(Arc::new(s)),
            Ok(Err(why)) => ShotEntry::Refused(why),
            Err(e) => return self.read_failed(e),
        };
        let current = self.shared.read(|s| Arc::clone(&s.shots));
        if current.get(&key) == Some(&entry) {
            return;
        }
        let mut next = (*current).clone();
        next.insert(key, entry);
        let next = Arc::new(next);
        self.shared.publish(|s| s.shots = next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault_write::state_dir;
    use desk_core::HumanSide;
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

    fn side(s: &Snapshot) -> Option<HumanSide> {
        s.day.as_ref()?.eu.human.as_ref().map(|h| h.side)
    }

    const READ_LONG: &str = r#"{"date":"2026-11-25","region":"eu","side":"LONG","at":"2026-11-25 09:31","before_scale":true}"#;
    const READ_SHORT: &str = r#"{"date":"2026-11-25","region":"eu","side":"SHORT","at":"2026-11-25 09:32","before_scale":true}"#;

    #[test]
    fn opens_a_day_follows_the_watcher_and_repaints_per_rev() {
        let vault = tempfile::tempdir().unwrap();
        let sd = state_dir(vault.path());
        fs::create_dir_all(&sd).unwrap();
        fs::write(sd.join("scale_2026-11-25_eu.json"), "{}").unwrap();
        fs::write(sd.join("human_read_2026-11-25_eu.json"), READ_LONG).unwrap();
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
        assert!(s0.index.as_ref().is_some_and(|i| i.ok));

        let before = app.snapshot().rev;
        let repaints_before = repaints.load(Ordering::SeqCst);
        app.send(Command::OpenDay(d("2026-11-25")));
        let s1 = wait_until(&app, "the day", |s| s.day.is_some());
        assert_eq!(s1.day.as_ref().unwrap().date, d("2026-11-25"));
        assert_eq!(side(&s1), Some(HumanSide::Long));

        // The skill (or the owner) rewrites a file: only that file is read again.
        fs::write(sd.join("human_read_2026-11-25_eu.json"), READ_SHORT).unwrap();
        let s2 = wait_until(&app, "the change", |s| side(s) == Some(HumanSide::Short));

        // A touch with the same bytes and another day's file: no new rev.
        fs::write(sd.join("human_read_2026-11-25_eu.json"), READ_SHORT).unwrap();
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
    fn loads_shots_off_the_ui_thread_and_refuses_bad_ones() {
        let vault = tempfile::tempdir().unwrap();
        let shots = vault.path().join("indices-kb/shots/2026-11-25");
        fs::create_dir_all(&shots).unwrap();
        fs::write(
            shots.join("eu_h1.png"),
            [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2],
        )
        .unwrap();
        let app = AppHandle::start(AppConfig {
            vault: Some(vault.path().into()),
            watch: fast(),
        })
        .unwrap();
        app.send(Command::LoadShot(
            "indices-kb/shots/2026-11-25/eu_h1.png".into(),
        ));
        app.send(Command::LoadShot(
            "indices-kb/shots/2026-11-25/none.png".into(),
        ));
        let s = wait_until(&app, "two shots", |s| s.shots.len() == 2);
        match &s.shots["indices-kb/shots/2026-11-25/eu_h1.png"] {
            ShotEntry::Ready(shot) => assert_eq!(shot.png.len(), 10),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            s.shots["indices-kb/shots/2026-11-25/none.png"],
            ShotEntry::Refused(ShotRefused::Missing)
        );
        // Asking again for the same bytes changes nothing.
        let rev = s.rev;
        app.send(Command::LoadShot(
            "indices-kb/shots/2026-11-25/eu_h1.png".into(),
        ));
        app.send(Command::CheckStatus); // a marker: one more rev, from the error
        let s = wait_until(&app, "the marker", |s| !s.errors.is_empty());
        assert_eq!(s.rev, rev + 1);
        // Opening a day that names none of them drops them.
        app.send(Command::OpenDay(d("2026-11-25")));
        wait_until(&app, "eviction", |s| s.day.is_some() && s.shots.is_empty());
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
        assert!((s.day.as_ref().unwrap().date - today).num_days().abs() <= 1);
        assert!(s.errors.is_empty());
        assert_eq!(
            s.index.as_ref().and_then(|i| i.error.as_deref()),
            Some("no-state-dir")
        );
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
