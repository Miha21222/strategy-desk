//! Watches `<vault>/indices-kb/state` and reports which files changed.
//!
//! Replaces `watchVault` in `src/main/macro-state.ts` (TZ item 12): `notify`
//! names the file, so the app re-reads only that file instead of the whole day.
//!
//! * Debounced: the skill writes several files in a burst, each through a
//!   temp file, so one change is many events. The set goes out once nothing
//!   moved for `debounce` (300 ms), or at the latest `max_wait` after the
//!   first event, so a never-ending burst cannot starve the screen.
//! * Temp files (`.*`, `*.tmp`) and read-only access events are ignored —
//!   the app's own reads must not wake the app.
//! * Survives the directory going away: while `state/` is missing the nearest
//!   existing ancestor is watched for its return, and a slow re-check
//!   (`recheck`, 2 s: one `stat`) covers backends that say nothing. After a
//!   re-arm every file in the new directory is reported, because files may
//!   have landed before the watch was back.
//!
//! The watcher runs on its own thread (notify's callback API is sync) and
//! stops when the [`StateWatcher`] is dropped.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::vault_write::state_dir;

/// Changed file names (no directory part).
pub type Changed = BTreeSet<String>;

#[derive(Debug, Clone, Copy)]
pub struct WatchOptions {
    pub debounce: Duration,
    pub max_wait: Duration,
    pub recheck: Duration,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(300),
            max_wait: Duration::from_secs(1),
            recheck: Duration::from_secs(2),
        }
    }
}

enum Msg {
    Event(notify::Result<Event>),
    Stop,
}

/// A running watcher; dropping it stops the thread.
pub struct StateWatcher {
    tx: mpsc::Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

impl StateWatcher {
    /// Watch `<vault>/indices-kb/state` with the default timings.
    pub fn start(
        vault: &Path,
        on_change: impl FnMut(Changed) + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::start_with(vault, WatchOptions::default(), on_change)
    }

    pub fn start_with(
        vault: &Path,
        opts: WatchOptions,
        on_change: impl FnMut(Changed) + Send + 'static,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let dir = state_dir(vault);
        let events = tx.clone();
        // Arm before returning: a file written right after `start` must not
        // fall between «the caller read the folder» and «the watch is on».
        let mut lp = Loop::new(dir, opts, events, Box::new(on_change));
        lp.ensure_armed();
        lp.next_check = Instant::now() + opts.recheck;
        let thread = std::thread::Builder::new()
            .name("desk-watch".into())
            .spawn(move || lp.run(rx))?;
        Ok(Self {
            tx,
            thread: Some(thread),
        })
    }
}

impl Drop for StateWatcher {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(t) = self.thread.take() {
            // The loop wakes at least every `recheck`; joining is bounded.
            let _ = t.join();
        }
    }
}

// The watchers are held only to keep them alive: dropping one stops it.
#[allow(dead_code)]
enum Armed {
    None,
    /// Watching `state/` itself.
    State(RecommendedWatcher),
    /// `state/` is missing; watching this ancestor for its return.
    Ancestor(RecommendedWatcher, PathBuf),
}

struct Loop {
    dir: PathBuf,
    /// `dir` as the OS spells it, for backends that report canonical paths.
    canon: Option<PathBuf>,
    opts: WatchOptions,
    events: mpsc::Sender<Msg>,
    on_change: Box<dyn FnMut(Changed) + Send>,
    armed: Armed,
    /// `state/` was watched once: a later re-arm must report every file.
    was_armed: bool,
    pending: Changed,
    first: Option<Instant>,
    last: Option<Instant>,
    next_check: Instant,
}

impl Loop {
    fn new(
        dir: PathBuf,
        opts: WatchOptions,
        events: mpsc::Sender<Msg>,
        on_change: Box<dyn FnMut(Changed) + Send>,
    ) -> Self {
        Self {
            dir,
            canon: None,
            opts,
            events,
            on_change,
            armed: Armed::None,
            was_armed: false,
            pending: Changed::new(),
            first: None,
            last: None,
            next_check: Instant::now(),
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Msg>) {
        loop {
            let now = Instant::now();
            if now >= self.next_check {
                self.ensure_armed();
                self.next_check = now + self.opts.recheck;
            }
            let mut wake = self.next_check;
            if let (Some(first), Some(last)) = (self.first, self.last) {
                wake = wake
                    .min(last + self.opts.debounce)
                    .min(first + self.opts.max_wait);
            }
            match rx.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::Event(Ok(ev))) => self.event(ev),
                Ok(Msg::Event(Err(e))) => {
                    tracing::warn!(error = %e, "watch: backend error, re-arming");
                    self.armed = Armed::None;
                    self.next_check = Instant::now();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            self.flush_if_due();
        }
    }

    fn watcher(&self, path: &Path) -> notify::Result<RecommendedWatcher> {
        let tx = self.events.clone();
        let mut w = notify::recommended_watcher(move |res| {
            let _ = tx.send(Msg::Event(res));
        })?;
        w.watch(path, RecursiveMode::NonRecursive)?;
        Ok(w)
    }

    fn ensure_armed(&mut self) {
        let exists = self.dir.is_dir();
        match (&self.armed, exists) {
            (Armed::State(_), true) => {}
            (Armed::State(_), false) => {
                tracing::debug!("watch: state dir is gone");
                self.armed = Armed::None;
                self.ensure_armed();
            }
            (_, true) => match self.watcher(&self.dir) {
                Ok(w) => {
                    self.armed = Armed::State(w);
                    self.canon = std::fs::canonicalize(&self.dir).ok();
                    if self.was_armed {
                        self.resync();
                    }
                    self.was_armed = true;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "watch: cannot watch the state dir yet");
                    self.armed = Armed::None;
                }
            },
            (Armed::Ancestor(_, at), false)
                if at.is_dir() && self.nearest_ancestor().as_deref() == Some(at.as_path()) => {}
            (_, false) => {
                // Without `state/` the next appearance counts as a re-arm.
                self.was_armed = true;
                self.armed = match self.nearest_ancestor() {
                    Some(a) => match self.watcher(&a) {
                        Ok(w) => Armed::Ancestor(w, a),
                        Err(_) => Armed::None,
                    },
                    None => Armed::None,
                };
            }
        }
    }

    fn nearest_ancestor(&self) -> Option<PathBuf> {
        self.dir
            .ancestors()
            .skip(1)
            .find(|p| p.is_dir())
            .map(Path::to_path_buf)
    }

    /// Every file currently in `state/`.
    fn resync(&mut self) {
        let Ok(rd) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let names: Vec<String> = rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| interesting(n))
            .collect();
        for n in names {
            self.add(n);
        }
    }

    fn is_state_dir(&self, p: &Path) -> bool {
        p == self.dir || self.canon.as_deref() == Some(p)
    }

    fn event(&mut self, ev: Event) {
        if matches!(self.armed, Armed::Ancestor(..)) {
            // Something moved above `state/`: maybe it is back.
            self.next_check = Instant::now();
            return;
        }
        if ev.need_rescan() {
            self.resync();
        }
        match ev.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write)) => {}
            EventKind::Access(_) => return,
            _ => {}
        }
        for p in &ev.paths {
            if self.is_state_dir(p) {
                // The directory itself was removed, renamed or replaced.
                tracing::debug!(kind = ?ev.kind, "watch: event on the state dir itself, re-arming");
                self.armed = Armed::None;
                self.was_armed = true;
                self.next_check = Instant::now();
                continue;
            }
            let in_dir = p.parent().is_some_and(|d| self.is_state_dir(d));
            if let (true, Some(name)) = (in_dir, p.file_name().and_then(|n| n.to_str())) {
                if interesting(name) {
                    self.add(name.to_string());
                }
            }
        }
    }

    fn add(&mut self, name: String) {
        let now = Instant::now();
        self.pending.insert(name);
        self.first.get_or_insert(now);
        self.last = Some(now);
    }

    fn flush_if_due(&mut self) {
        let (Some(first), Some(last)) = (self.first, self.last) else {
            return;
        };
        let now = Instant::now();
        if now >= last + self.opts.debounce || now >= first + self.opts.max_wait {
            self.first = None;
            self.last = None;
            let set = std::mem::take(&mut self.pending);
            if !set.is_empty() {
                (self.on_change)(set);
            }
        }
    }
}

/// Not a temp file of ours (`.<name>.<pid>.<n>.tmp`) or of an editor / Python.
fn interesting(name: &str) -> bool {
    !name.starts_with('.') && !name.ends_with(".tmp") && !name.ends_with('~')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const T: Duration = Duration::from_secs(10);

    fn fast() -> WatchOptions {
        WatchOptions {
            debounce: Duration::from_millis(150),
            max_wait: Duration::from_millis(800),
            recheck: Duration::from_millis(200),
        }
    }

    fn start(vault: &Path) -> (StateWatcher, mpsc::Receiver<Changed>) {
        let (tx, rx) = mpsc::channel();
        let w = StateWatcher::start_with(vault, fast(), move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        (w, rx)
    }

    /// Collect callbacks until `want` has been seen (in one or more sets).
    fn wait_for(rx: &mpsc::Receiver<Changed>, want: &[&str]) -> Changed {
        let mut seen = Changed::new();
        let deadline = Instant::now() + T;
        while !want.iter().all(|w| seen.contains(*w)) {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(set) => seen.extend(set),
                Err(_) => panic!("timed out; saw {seen:?}, wanted {want:?}"),
            }
        }
        seen
    }

    #[test]
    fn a_burst_becomes_one_set_without_temp_files() {
        let vault = tempfile::tempdir().unwrap();
        let dir = state_dir(vault.path());
        fs::create_dir_all(&dir).unwrap();
        let (_w, rx) = start(vault.path());
        std::thread::sleep(Duration::from_millis(300)); // armed
        fs::write(dir.join("scale_2026-11-25_eu.json"), b"{}").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        crate::vault_write::atomic_write(&dir, "macro_2026-11-25_eu.json", b"{}").unwrap();
        fs::write(dir.join("junk.tmp"), b"x").unwrap();
        let first = rx.recv_timeout(T).unwrap();
        assert_eq!(
            first,
            ["macro_2026-11-25_eu.json", "scale_2026-11-25_eu.json"]
                .iter()
                .map(|s| s.to_string())
                .collect::<Changed>()
        );
        // Reading a file must not wake the watcher.
        let _ = fs::read(dir.join("scale_2026-11-25_eu.json")).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(600)).is_err());
    }

    #[test]
    fn survives_the_directory_being_deleted_and_recreated() {
        let vault = tempfile::tempdir().unwrap();
        let dir = state_dir(vault.path());
        fs::create_dir_all(&dir).unwrap();
        let (_w, rx) = start(vault.path());
        std::thread::sleep(Duration::from_millis(300));
        fs::write(dir.join("a.json"), b"1").unwrap();
        wait_for(&rx, &["a.json"]);
        fs::remove_dir_all(&dir).unwrap();
        std::thread::sleep(Duration::from_millis(400));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.json"), b"2").unwrap();
        wait_for(&rx, &["b.json"]);
        // And it keeps watching the new directory.
        std::thread::sleep(Duration::from_millis(300));
        fs::write(dir.join("c.json"), b"3").unwrap();
        wait_for(&rx, &["c.json"]);
    }

    #[test]
    fn starts_before_the_directory_exists() {
        let vault = tempfile::tempdir().unwrap();
        let (_w, rx) = start(vault.path());
        std::thread::sleep(Duration::from_millis(300));
        let dir = state_dir(vault.path());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ideas.json"), b"[]").unwrap();
        wait_for(&rx, &["ideas.json"]);
    }

    #[test]
    fn interesting_names() {
        assert!(interesting("scale_2026-11-25_eu.json"));
        assert!(!interesting(".human_read_2026-11-25_eu.json.12.0.tmp"));
        assert!(!interesting("x.json.tmp"));
        assert!(!interesting("x.json~"));
    }
}
