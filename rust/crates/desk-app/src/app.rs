//! `AppHandle` / `Command` / `Snapshot` (CONTRACTS.md «desk-app ↔ desk-ui»).
//!
//! The UI never waits on anything: `send()` pushes onto an unbounded channel,
//! `snapshot()` clones a handful of `Arc`s under a read lock that is only ever
//! held for a field assignment. All work happens on a tokio runtime in one
//! background thread; disk reads, writes and `load_day` run on the blocking pool.
//!
//! Wired: `OpenDay` / `Today` (desk-core's `load_day` over `day_files`), the
//! watcher (only the changed file is read again), the day strip
//! (`MacroIndex`), `LoadShot`, the owner's two writes and `SetVault`. Chat,
//! status, TradingView and tasks answer `error.not_implemented`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, Utc};
use desk_core::day::load_day;
use desk_core::vault::DayAnswerRegions;
use desk_core::{Day, DayInput, HumanSide, MacroIndex, Region};
use tokio::sync::mpsc;

use crate::chat::manager::{ChatMsg, Chats, Forward};
use crate::chat::store::{default_data_dir, ChatStore};
use crate::chat::{ChatCommand, ChatsView, ClaudeLaunch};
use crate::day::{self, DayMark};
use crate::shots::{self, LoadedShot, ShotRefused};
use crate::vault_write::{self, WriteError};
use crate::watch::{Changed, StateWatcher, WatchOptions};

/// How many errors the snapshot keeps (oldest dropped first).
pub const ERRORS_KEPT: usize = 20;

/// Bench hook (CONTRACTS «Хуки для замеров»): one stderr JSON line per new rev.
pub const BENCH_WATCH_ENV: &str = "DESK_BENCH_WATCH";

/// A task id from a fixed list — never a command line (rule of the old app).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    OpenDay(NaiveDate),
    Today,
    /// «Твой взгляд» → `state/human_read_<D>_<region>.json`.
    WriteHumanRead {
        date: NaiveDate,
        region: Region,
        side: HumanSide,
        before_scale: bool,
    },
    /// «Твой день» → `state/day_answers_<D>.json`.
    WriteDayAnswers {
        date: NaiveDate,
        regions: DayAnswerRegions,
        note: String,
    },
    /// Read a screenshot the open day names (vault-relative, a `MacroShot.path`)
    /// into `Snapshot::shots`, off the UI thread.
    LoadShot(String),
    /// Read another vault: watcher, day strip and the open day follow.
    SetVault(PathBuf),
    Chat(ChatCommand),
    CheckStatus,
    LaunchTradingView,
    RunTask(TaskId),
}

/// A screenshot as the UI may find it in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShotState {
    Ready(Arc<LoadedShot>),
    /// Not shown (missing, not a PNG, outside `indices-kb/shots/`); the tile stays empty.
    Refused(ShotRefused),
}

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
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub day: Option<Arc<Day>>,
    pub days_index: Arc<Vec<DayMark>>,
    /// The `MacroIndex` behind `days_index`; carries `no-vault` / `no-state-dir`.
    pub index: Arc<MacroIndex>,
    /// Screenshots asked for with `LoadShot`, by vault-relative path. Only
    /// the open day's are kept.
    pub shots: Arc<HashMap<String, ShotState>>,
    pub chats: Arc<ChatsView>,
    pub status: Arc<StatusLights>,
    pub tasks: Arc<TasksView>,
    pub errors: Arc<Vec<UserError>>,
    /// Bumps on every change.
    pub rev: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            day: None,
            days_index: Arc::default(),
            index: Arc::new(no_vault_index()),
            shots: Arc::default(),
            chats: Arc::default(),
            status: Arc::default(),
            tasks: Arc::default(),
            errors: Arc::default(),
            rev: 0,
        }
    }
}

fn no_vault_index() -> MacroIndex {
    MacroIndex::new("", None::<std::iter::Empty<&str>>)
}

/// i18n keys this module produces (plus `WriteError::key`).
pub mod keys {
    pub const NOT_IMPLEMENTED: &str = "error.not_implemented";
    pub const VAULT_NOT_SET: &str = "error.vault.not_set";
    pub const VAULT_READ_FAILED: &str = "error.vault.read_failed";
    pub const VAULT_WRITE_FAILED: &str = "error.vault.write_failed";
    pub const WATCH_FAILED: &str = "error.vault.watch_failed";
}

/// What the background starts with. TODO(stage 6): read from the store.
#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub vault: Option<PathBuf>,
    pub watch: WatchOptions,
    /// Where chats are stored; `None` = `<platform data dir>/strategy-desk-rs`.
    pub data_dir: Option<PathBuf>,
    /// How a chat's `claude` is started (tests and benches; empty in production).
    pub claude: ClaudeLaunch,
}

/// Chat snapshots are published at most this often (≤ 30/s, CONTRACTS).
pub const CHAT_FRAME: Duration = Duration::from_millis(34);

type Repaint = Arc<dyn Fn() + Send + Sync>;

struct Shared {
    snap: RwLock<Snapshot>,
    repaint: RwLock<Option<Repaint>>,
    bench_watch: bool,
}

impl Shared {
    /// Apply a change and wake the UI. Callers only call this with a real
    /// change, so `rev` and the repaint move together.
    fn publish(&self, f: impl FnOnce(&mut Snapshot)) {
        let rev = {
            // A poisoned lock means a panic mid-assignment of Arcs; the
            // snapshot is still a valid value, keep serving it.
            let mut s = self.snap.write().unwrap_or_else(|e| e.into_inner());
            f(&mut s);
            s.rev += 1;
            s.rev
        };
        if self.bench_watch {
            let ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            eprintln!("{}", bench_line(rev, ms));
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

/// `{"bench":"watch","rev":N,"t_unix_ms":…}` (docs/rust/PERF.md).
fn bench_line(rev: u64, t_unix_ms: u128) -> String {
    format!(r#"{{"bench":"watch","rev":{rev},"t_unix_ms":{t_unix_ms}}}"#)
}

enum Msg {
    Cmd(Command),
    Chat(ChatMsg),
    /// A watcher's report; `gen` names the vault it watched.
    Changed(u64, Changed),
}

/// The UI's only door into desk-app.
pub struct AppHandle {
    tx: Option<mpsc::UnboundedSender<Msg>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl AppHandle {
    /// The desk on one vault (desk-ui passes `--vault` / `DESK_VAULT`).
    pub fn new(vault: PathBuf) -> std::io::Result<Self> {
        Self::start(AppConfig {
            vault: Some(vault),
            ..AppConfig::default()
        })
    }

    /// Start the background thread and its runtime.
    pub fn start(cfg: AppConfig) -> std::io::Result<Self> {
        let bench_watch = std::env::var_os(BENCH_WATCH_ENV).is_some_and(|v| v == "1");
        let shared = Arc::new(Shared {
            snap: RwLock::new(Snapshot::default()),
            repaint: RwLock::new(None),
            bench_watch,
        });
        let (tx, rx) = mpsc::unbounded_channel();
        // current_thread: the app's own work is small and mostly waiting;
        // blocking file I/O goes to tokio's blocking pool.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let actor = Actor {
            shared: Arc::clone(&shared),
            vault: cfg.vault,
            watch_opts: cfg.watch,
            data_dir: cfg.data_dir,
            launch: cfg.claude,
            chats: None,
            // Weak: the loop must end when the UI drops its handle.
            weak: tx.downgrade(),
            watcher: None,
            watch_gen: 0,
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
        // Closing the channel ends the actor (everything else holds only a
        // weak sender), then the thread is joined.
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
    data_dir: Option<PathBuf>,
    launch: ClaudeLaunch,
    /// Built inside the runtime (its writer is a task).
    chats: Option<Chats>,
    weak: mpsc::WeakUnboundedSender<Msg>,
    watcher: Option<StateWatcher>,
    /// Bumped per watcher, so a late report about the previous vault is dropped.
    watch_gen: u64,
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
        self.refresh_index().await;
        self.start_chats().await;
        let mut last_chat_publish = tokio::time::Instant::now() - CHAT_FRAME;
        loop {
            let due = self
                .chats
                .as_ref()
                .filter(|c| c.is_dirty())
                .map(|_| (last_chat_publish + CHAT_FRAME).max(tokio::time::Instant::now()));
            tokio::select! {
                // Biased: under a flood of driver events the frame still comes.
                biased;
                _ = async { tokio::time::sleep_until(due.unwrap_or_else(tokio::time::Instant::now)).await }, if due.is_some() => {
                    self.publish_chats();
                    last_chat_publish = tokio::time::Instant::now();
                }
                msg = rx.recv() => match msg {
                    None => break,
                    Some(Msg::Cmd(c)) => self.command(c).await,
                    Some(Msg::Chat(m)) => {
                        if let Some(chats) = self.chats.as_mut() {
                            if let Err(e) = chats.on_msg(m, Utc::now()) {
                                self.error(e);
                            }
                        }
                    }
                    Some(Msg::Changed(g, names)) if g == self.watch_gen => self.changed(names).await,
                    Some(Msg::Changed(..)) => {}
                },
            }
        }
        self.watcher = None;
        if let Some(chats) = self.chats.take() {
            chats.close().await;
        }
    }

    async fn start_chats(&mut self) {
        let Some(dir) = self.data_dir.clone().or_else(default_data_dir) else {
            self.error(
                UserError::new(crate::chat::model::keys::STORE).var("detail", "no data dir"),
            );
            return;
        };
        let weak = self.weak.clone();
        let forward: Forward = Arc::new(move |m| {
            if let Some(tx) = weak.upgrade() {
                let _ = tx.send(Msg::Chat(m));
            }
        });
        let mut chats = Chats::new(ChatStore::new(&dir), self.launch.clone(), forward);
        if let Err(e) = chats.load_list().await {
            self.error(e);
        }
        self.chats = Some(chats);
    }

    fn publish_chats(&mut self) {
        if let Some(view) = self.chats.as_mut().and_then(Chats::take_view) {
            self.shared.publish(|s| s.chats = view);
        }
    }

    fn start_watcher(&mut self) {
        // Stop the old one first: its thread is joined on drop.
        self.watcher = None;
        self.watch_gen += 1;
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let (tx, gen) = (self.weak.clone(), self.watch_gen);
        match StateWatcher::start_with(&vault, self.watch_opts, move |names| {
            if let Some(tx) = tx.upgrade() {
                let _ = tx.send(Msg::Changed(gen, names));
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
            Command::SetVault(v) => self.set_vault(v).await,
            Command::WriteHumanRead {
                date,
                region,
                side,
                before_scale,
            } => {
                self.write(move |vault| {
                    vault_write::write_human_read(
                        vault,
                        date,
                        region,
                        side,
                        before_scale,
                        Utc::now(),
                    )
                    .map(drop)
                })
                .await
            }
            Command::WriteDayAnswers {
                date,
                regions,
                note,
            } => {
                self.write(move |vault| {
                    vault_write::write_day_answers(vault, date, regions, &note, Utc::now())
                        .map(drop)
                })
                .await
            }
            Command::Chat(c) => {
                let now = Utc::now();
                let today = now.with_timezone(&chrono_tz::Europe::Kyiv).date_naive();
                let vault = self.vault.clone();
                let res = match self.chats.as_mut() {
                    Some(chats) => chats.command(c, vault.as_deref(), today, now).await,
                    None => Err(UserError::new(crate::chat::model::keys::STORE)),
                };
                if let Err(e) = res {
                    self.error(e);
                }
            }
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

    /// One of the owner's two writes. The screen follows through the watcher,
    /// the same way it follows the skill's writes.
    async fn write(
        &mut self,
        f: impl FnOnce(&std::path::Path) -> Result<(), WriteError> + Send + 'static,
    ) {
        let Some(vault) = self.vault.clone() else {
            self.error(UserError::new(keys::VAULT_NOT_SET));
            return;
        };
        match tokio::task::spawn_blocking(move || f(&vault)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let mut err = UserError::new(e.key());
                if matches!(e, WriteError::Io(_) | WriteError::Core(_)) {
                    err = err.var("detail", e.to_string());
                }
                self.error(err);
            }
            Err(e) => {
                self.error(UserError::new(keys::VAULT_WRITE_FAILED).var("detail", e.to_string()))
            }
        }
    }

    async fn set_vault(&mut self, vault: PathBuf) {
        if self.vault.as_deref() == Some(vault.as_path()) {
            return;
        }
        self.vault = Some(vault);
        if let Some(chats) = self.chats.as_mut() {
            chats.vault_changed();
        }
        self.start_watcher();
        let reopen = self.open.take();
        self.input = DayInput::new();
        self.keys.clear();
        self.shared.publish(|s| {
            s.day = None;
            s.shots = Arc::default();
        });
        self.refresh_index().await;
        if let Some(d) = reopen {
            self.open_day(d).await;
        }
    }

    /// Publish a day, keeping only the screenshots it still names.
    fn publish_day(&self, day: Day) {
        let paths: HashSet<&str> = day.shot_paths().into_iter().collect();
        let shots = self.shared.read(|s| Arc::clone(&s.shots));
        let kept: HashMap<String, ShotState> = shots
            .iter()
            .filter(|(k, _)| paths.contains(k.as_str()))
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
                        if let Some(built) = built {
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
                    .read(|s| *s.days_index == marks && *s.index == index);
                if !unchanged {
                    let (marks, index) = (Arc::new(marks), Arc::new(index));
                    self.shared.publish(|s| {
                        s.days_index = marks;
                        s.index = index;
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
            Ok(Ok(s)) => ShotState::Ready(Arc::new(s)),
            Ok(Err(why)) => ShotState::Refused(why),
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
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    /// Tests never touch the real data dir: chats go to a temp folder.
    fn test_cfg(vault: Option<PathBuf>) -> AppConfig {
        AppConfig {
            vault,
            watch: fast(),
            data_dir: Some(tempfile::tempdir().unwrap().keep()),
            claude: ClaudeLaunch::default(),
        }
    }

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

    /// The repaint guarantee lives in `Shared::publish`: every rev bump calls
    /// the callback once, and nothing else bumps the rev. Tested on `Shared`
    /// itself, because counting callbacks against snapshots taken from
    /// another thread races the callback that runs just after the bump.
    #[test]
    fn one_repaint_per_rev() {
        let shared = Shared {
            snap: RwLock::new(Snapshot::default()),
            repaint: RwLock::new(None),
            bench_watch: false,
        };
        shared.publish(|_| {}); // before a callback is set: rev moves, nothing to call
        let calls = Arc::new(AtomicU64::new(0));
        let c = Arc::clone(&calls);
        *shared.repaint.write().unwrap() = Some(Arc::new(move || {
            c.fetch_add(1, Ordering::SeqCst);
        }));
        for _ in 0..5 {
            shared.publish(|s| s.errors = Arc::new(vec![UserError::new("x")]));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        assert_eq!(shared.read(|s| s.rev), 6);
    }

    #[test]
    fn opens_a_day_and_follows_the_watcher() {
        let vault = tempfile::tempdir().unwrap();
        let sd = state_dir(vault.path());
        fs::create_dir_all(&sd).unwrap();
        fs::write(sd.join("scale_2026-11-25_eu.json"), "{}").unwrap();
        fs::write(sd.join("human_read_2026-11-25_eu.json"), READ_LONG).unwrap();
        fs::write(sd.join("review_2026-11-24.json"), "{}").unwrap();
        // Writes into the watched folder are atomic, as the skill's and the
        // owner's are: a plain fs::write can be seen half-done (truncated).
        let put = |name: &str, text: &str| {
            crate::vault_write::atomic_write(&sd, name, text.as_bytes()).unwrap();
        };

        let app = AppHandle::start(test_cfg(Some(vault.path().into()))).unwrap();
        let s0 = wait_until(&app, "the index", |s| s.days_index.len() == 2);
        assert_eq!(
            s0.days_index[0],
            DayMark {
                date: d("2026-11-25"),
                window_review: true,
                day_review: false
            }
        );
        assert!(s0.index.ok);

        app.send(Command::OpenDay(d("2026-11-25")));
        let s1 = wait_until(&app, "the day", |s| s.day.is_some());
        assert_eq!(s1.day.as_ref().unwrap().date, d("2026-11-25"));
        assert_eq!(side(&s1), Some(HumanSide::Long));

        // The skill (or the owner) rewrites a file: only that file is read again.
        put("human_read_2026-11-25_eu.json", READ_SHORT);
        let s2 = wait_until(&app, "the change", |s| side(s) == Some(HumanSide::Short));

        // A rewrite with the same bytes and another day's file: no new rev.
        put("human_read_2026-11-25_eu.json", READ_SHORT);
        put("drift_2026-11-20_eu.json", "{}");
        // An absence: this window can only fail when a no-op IS published.
        std::thread::sleep(Duration::from_millis(800));
        assert_eq!(app.snapshot().rev, s2.rev, "no change, no rev");
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
        let app = AppHandle::start(test_cfg(Some(vault.path().into()))).unwrap();
        app.send(Command::LoadShot(
            "indices-kb/shots/2026-11-25/eu_h1.png".into(),
        ));
        app.send(Command::LoadShot(
            "indices-kb/shots/2026-11-25/none.png".into(),
        ));
        let s = wait_until(&app, "two shots", |s| s.shots.len() == 2);
        match &s.shots["indices-kb/shots/2026-11-25/eu_h1.png"] {
            ShotState::Ready(shot) => assert_eq!(shot.png.len(), 10),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            s.shots["indices-kb/shots/2026-11-25/none.png"],
            ShotState::Refused(ShotRefused::Missing)
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
        let app = AppHandle::start(test_cfg(None)).unwrap();
        app.send(Command::Today);
        let s = wait_until(&app, "an error", |s| !s.errors.is_empty());
        assert_eq!(s.errors[0], UserError::new(keys::VAULT_NOT_SET));
        assert!(s.day.is_none());

        let vault = tempfile::tempdir().unwrap();
        let app = AppHandle::start(test_cfg(Some(vault.path().into()))).unwrap();
        app.send(Command::Today);
        let s = wait_until(&app, "today", |s| s.day.is_some());
        let today = Utc::now()
            .with_timezone(&chrono_tz::Europe::Kyiv)
            .date_naive();
        // Midnight in Kyiv between the two lines would flake; compare loosely.
        assert!((s.day.as_ref().unwrap().date - today).num_days().abs() <= 1);
        assert!(s.errors.is_empty());
        assert_eq!(s.index.error.as_deref(), Some("no-state-dir"));
    }

    #[test]
    fn unwired_commands_answer_with_a_user_error() {
        let app = AppHandle::start(test_cfg(None)).unwrap();
        app.send(Command::Chat(ChatCommand::Send {
            chat_id: "c1".into(),
            text: "hi".into(),
        }));
        app.send(Command::CheckStatus);
        app.send(Command::RunTask(TaskId("provision-runtime".into())));
        let s = wait_until(&app, "three errors", |s| s.errors.len() == 3);
        assert_eq!(
            s.errors[0],
            UserError::new(crate::chat::model::keys::UNKNOWN_CHAT)
        );
        assert!(s.errors[1..].iter().all(|e| e.key == keys::NOT_IMPLEMENTED));
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
        let app = AppHandle::start(test_cfg(None)).unwrap();
        let t0 = Instant::now();
        for _ in 0..10_000 {
            app.send(Command::CheckStatus);
        }
        for _ in 0..10_000 {
            let _ = app.snapshot();
        }
        // A smoke bound, not a benchmark (desk-bench measures): generous so a
        // loaded 2-core runner cannot trip it, still far below «blocks».
        assert!(t0.elapsed() < Duration::from_secs(10), "{:?}", t0.elapsed());
    }

    #[test]
    fn owner_writes_land_and_the_watcher_shows_them() {
        let vault = tempfile::tempdir().unwrap();
        fs::create_dir_all(state_dir(vault.path())).unwrap();
        let app = AppHandle::start(test_cfg(Some(vault.path().into()))).unwrap();
        app.send(Command::OpenDay(d("2026-11-25")));
        wait_until(&app, "the day", |s| s.day.is_some());
        app.send(Command::WriteHumanRead {
            date: d("2026-11-25"),
            region: Region::Eu,
            side: HumanSide::Short,
            before_scale: false,
        });
        let s = wait_until(&app, "the read", |s| side(s) == Some(HumanSide::Short));
        assert_eq!(
            s.day
                .as_ref()
                .unwrap()
                .eu
                .human
                .as_ref()
                .map(|h| h.before_scale),
            Some(false)
        );
        app.send(Command::WriteDayAnswers {
            date: d("2026-11-25"),
            regions: DayAnswerRegions::default(),
            note: " терпение\nпомогло ".into(),
        });
        let s = wait_until(&app, "the answers", |s| {
            s.day.as_ref().is_some_and(|x| x.answers.is_some())
        });
        assert_eq!(
            s.day
                .as_ref()
                .unwrap()
                .answers
                .as_ref()
                .map(|a| a.note.as_str()),
            Some("терпение помогло")
        );
        assert!(s.errors.is_empty(), "{:?}", s.errors);
    }

    #[test]
    fn bench_line_is_json() {
        let v: serde_json::Value = serde_json::from_str(&bench_line(7, 1_795_615_364_778)).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "bench": "watch", "rev": 7, "t_unix_ms": 1_795_615_364_778u64 })
        );
    }

    #[test]
    fn write_errors_are_i18n_keys() {
        let app = AppHandle::start(test_cfg(None)).unwrap();
        app.send(Command::WriteHumanRead {
            date: d("2026-11-25"),
            region: Region::Us,
            side: HumanSide::Long,
            before_scale: true,
        });
        let s = wait_until(&app, "no vault", |s| !s.errors.is_empty());
        assert_eq!(s.errors[0], UserError::new(keys::VAULT_NOT_SET));
        let vault = tempfile::tempdir().unwrap();
        let app = AppHandle::start(test_cfg(Some(vault.path().into()))).unwrap();
        let far = NaiveDate::from_ymd_opt(10000, 1, 1).unwrap();
        app.send(Command::WriteDayAnswers {
            date: far,
            regions: DayAnswerRegions::default(),
            note: String::new(),
        });
        let s = wait_until(&app, "bad date", |s| !s.errors.is_empty());
        assert_eq!(s.errors[0], UserError::new("error.vault.bad_date"));
        assert!(!state_dir(vault.path()).exists(), "nothing written");
    }

    #[test]
    fn set_vault_moves_everything() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        fs::create_dir_all(state_dir(a.path())).unwrap();
        fs::create_dir_all(state_dir(b.path())).unwrap();
        fs::write(
            state_dir(b.path()).join("human_read_2026-11-25_eu.json"),
            READ_LONG,
        )
        .unwrap();
        fs::write(state_dir(b.path()).join("scale_2026-11-25_eu.json"), "{}").unwrap();
        let app = AppHandle::start(test_cfg(Some(a.path().into()))).unwrap();
        app.send(Command::OpenDay(d("2026-11-25")));
        let s = wait_until(&app, "day in a", |s| s.day.is_some());
        assert_eq!(side(&s), None);
        assert!(s.days_index.is_empty());
        app.send(Command::SetVault(b.path().into()));
        let s = wait_until(&app, "day in b", |s| {
            side(s) == Some(HumanSide::Long) && s.days_index.len() == 1
        });
        assert_eq!(s.index.vault, b.path().to_string_lossy());
        // The new vault is watched, the old one no longer.
        fs::write(
            state_dir(a.path()).join("human_read_2026-11-25_eu.json"),
            READ_SHORT,
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(side(&app.snapshot()), Some(HumanSide::Long));
        fs::write(
            state_dir(b.path()).join("human_read_2026-11-25_eu.json"),
            READ_SHORT,
        )
        .unwrap();
        wait_until(&app, "b watched", |s| side(s) == Some(HumanSide::Short));
    }
}
