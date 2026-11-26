//! The chats, as the app's background loop owns them: the list, the loaded
//! transcripts, one `claude` process per chat that has talked since start,
//! and an ordered writer so the files on disk follow in the same order.
//!
//! Single-threaded by design (it lives inside the app's actor): events of
//! the driver come back through `forward` as [`ChatMsg`] and are folded here.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use super::launch::{find_claude, session_config, ClaudeLaunch};
use super::model::{
    apply_event, auto_title, clean_title, keys, ChatCommand, ChatId, ChatKind, ChatMeta, ChatsView,
    Transcript,
};
use super::store::{new_id, ChatStore};
use crate::app::UserError;
use crate::claude::{keys as driver_keys, ClaudeSession, DriverEvent};

/// What comes back from a session's forwarder or the writer.
#[derive(Debug)]
pub enum ChatMsg {
    Event {
        chat: ChatId,
        gen: u64,
        ev: DriverEvent,
    },
    /// The session's event stream ended: its process is gone.
    Closed { chat: ChatId, gen: u64 },
    /// A chat file could not be written (detail is the OS error).
    StoreFailed { detail: String },
}

/// How a message gets back into the app's loop.
pub type Forward = Arc<dyn Fn(ChatMsg) + Send + Sync>;

enum WriteOp {
    Save(ChatId, Vec<u8>),
    Delete(ChatId),
}

struct Live {
    session: ClaudeSession,
    gen: u64,
    /// Started with `--resume`.
    resumed: bool,
    /// The turn in flight, for the one retry without `--resume` (C3).
    last_text: String,
    /// The CLI said the resumed conversation is gone: retry after its Done.
    retry: bool,
}

pub struct Chats {
    store: ChatStore,
    writes: Option<mpsc::UnboundedSender<WriteOp>>,
    writer: Option<JoinHandle<()>>,
    metas: Vec<ChatMeta>,
    transcripts: HashMap<ChatId, Arc<Transcript>>,
    sessions: HashMap<ChatId, Live>,
    open: Option<ChatId>,
    gen: u64,
    seq: u64,
    launch: ClaudeLaunch,
    forward: Forward,
    /// The view changed since the last [`Chats::take_view`].
    dirty: bool,
}

/// Writes in the order they were asked for: a late save must never bring a
/// deleted chat back, nor an older transcript win over a newer one.
async fn writer(store: ChatStore, mut rx: mpsc::UnboundedReceiver<WriteOp>, forward: Forward) {
    while let Some(op) = rx.recv().await {
        let s = store.clone();
        let res = tokio::task::spawn_blocking(move || match op {
            WriteOp::Save(id, bytes) => s.save(&id, &bytes),
            WriteOp::Delete(id) => s.delete(&id),
        })
        .await;
        let err = match res {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(e) => Some(e.to_string()),
        };
        if let Some(detail) = err {
            tracing::warn!(%detail, "chats: write failed");
            forward(ChatMsg::StoreFailed { detail });
        }
    }
}

impl Chats {
    /// Must be called inside the tokio runtime (it starts the writer).
    pub fn new(store: ChatStore, launch: ClaudeLaunch, forward: Forward) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let writer = tokio::spawn(writer(store.clone(), rx, Arc::clone(&forward)));
        Self {
            store,
            writes: Some(tx),
            writer: Some(writer),
            metas: Vec::new(),
            transcripts: HashMap::new(),
            sessions: HashMap::new(),
            open: None,
            gen: 0,
            seq: 0,
            launch,
            forward,
            dirty: true,
        }
    }

    /// Read the list (metas only) off the UI path.
    pub async fn load_list(&mut self) -> Result<(), UserError> {
        let store = self.store.clone();
        let res = tokio::task::spawn_blocking(move || store.list()).await;
        match res {
            Ok(Ok(list)) => {
                self.metas = list;
                self.dirty = true;
                Ok(())
            }
            Ok(Err(e)) => Err(UserError::new(keys::STORE).var("detail", e.to_string())),
            Err(e) => Err(UserError::new(keys::STORE).var("detail", e.to_string())),
        }
    }

    /// Stop every process and wait (bounded) for the queued writes.
    pub async fn close(mut self) {
        for (_, live) in self.sessions.drain() {
            live.session.shutdown();
        }
        self.writes = None;
        if let Some(w) = self.writer.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), w).await;
        }
    }

    /// The vault changed: idle sessions run in the old cwd, end them.
    pub fn vault_changed(&mut self) {
        let busy: Vec<ChatId> = self
            .metas
            .iter()
            .filter(|m| m.busy)
            .map(|m| m.id.clone())
            .collect();
        self.sessions.retain(|id, live| {
            let keep = busy.contains(id);
            if !keep {
                live.session.shutdown();
            }
            keep
        });
    }

    /// The view, if it changed since last time. Drops transcripts nobody
    /// shows (they are on disk).
    pub fn take_view(&mut self) -> Option<Arc<ChatsView>> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        let keep: Vec<ChatId> = self
            .metas
            .iter()
            .filter(|m| m.busy || self.open.as_deref() == Some(m.id.as_str()))
            .map(|m| m.id.clone())
            .collect();
        self.transcripts.retain(|id, _| keep.contains(id));
        let mut list = self.metas.clone();
        list.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        Some(Arc::new(ChatsView {
            list,
            open: self.open.clone(),
            transcripts: self.transcripts.clone(),
        }))
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn meta(&self, id: &str) -> Option<&ChatMeta> {
        self.metas.iter().find(|m| m.id == id)
    }

    fn meta_mut(&mut self, id: &str) -> Option<&mut ChatMeta> {
        self.metas.iter_mut().find(|m| m.id == id)
    }

    fn persist(&mut self, id: &str) {
        let (Some(meta), Some(t)) = (self.meta(id), self.transcripts.get(id)) else {
            return;
        };
        match ChatStore::encode(meta, t) {
            Ok(bytes) => {
                if let Some(w) = &self.writes {
                    let _ = w.send(WriteOp::Save(id.to_string(), bytes));
                }
            }
            Err(e) => tracing::warn!(error = %e, "chats: cannot encode a chat"),
        }
    }

    /// The transcript of a known chat, from disk if it is not in memory.
    async fn ensure_loaded(&mut self, id: &str) -> Result<(), UserError> {
        if self.transcripts.contains_key(id) {
            return Ok(());
        }
        let (store, key) = (self.store.clone(), id.to_string());
        let t = match tokio::task::spawn_blocking(move || store.load(&key)).await {
            Ok(Ok(Some((_, t)))) => t,
            // Listed but gone or never saved: start it empty.
            Ok(Ok(None)) => Transcript::default(),
            Ok(Err(e)) => return Err(UserError::new(keys::STORE).var("detail", e.to_string())),
            Err(e) => return Err(UserError::new(keys::STORE).var("detail", e.to_string())),
        };
        self.transcripts.insert(id.to_string(), Arc::new(t));
        Ok(())
    }

    fn create(
        &mut self,
        kind: ChatKind,
        title: String,
        date: Option<NaiveDate>,
        now: DateTime<Utc>,
    ) -> ChatId {
        self.seq += 1;
        let id = new_id(now, self.seq);
        self.metas.push(ChatMeta {
            id: id.clone(),
            kind,
            title,
            date,
            created_at: now,
            updated_at: now,
            session_id: None,
            busy: false,
            cost_usd: 0.0,
        });
        self.transcripts.insert(id.clone(), Arc::default());
        self.persist(&id);
        id
    }

    async fn open_macro(
        &mut self,
        date: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<ChatId, UserError> {
        let found = self
            .metas
            .iter()
            .find(|m| m.kind == ChatKind::Macro && m.date == Some(date))
            .map(|m| m.id.clone());
        let id = match found {
            Some(id) => {
                self.ensure_loaded(&id).await?;
                id
            }
            None => self.create(
                ChatKind::Macro,
                desk_core::prompts::macro_chat_title(date),
                Some(date),
                now,
            ),
        };
        self.open = Some(id.clone());
        self.dirty = true;
        Ok(id)
    }

    /// Handle a chat command. `Err` goes to the app's error list; problems a
    /// chat should show itself become `Notice` items instead.
    pub async fn command(
        &mut self,
        cmd: ChatCommand,
        vault: Option<&Path>,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<(), UserError> {
        match cmd {
            ChatCommand::New {
                kind: ChatKind::Ask,
            } => {
                let id = self.create(ChatKind::Ask, String::new(), None, now);
                self.open = Some(id);
                self.dirty = true;
            }
            ChatCommand::New {
                kind: ChatKind::Macro,
            } => {
                self.open_macro(today, now).await?;
            }
            ChatCommand::OpenMacro { date } => {
                self.open_macro(date, now).await?;
            }
            ChatCommand::Open { chat_id } => {
                if self.meta(&chat_id).is_none() {
                    return Err(UserError::new(keys::UNKNOWN_CHAT));
                }
                self.ensure_loaded(&chat_id).await?;
                self.open = Some(chat_id);
                self.dirty = true;
            }
            ChatCommand::Send { chat_id, text } => self.send(&chat_id, &text, vault, now).await?,
            ChatCommand::SendMacro { date, text } => {
                let id = self.open_macro(date, now).await?;
                self.send(&id, &text, vault, now).await?;
            }
            ChatCommand::Run { window, date } => {
                let id = self.open_macro(date, now).await?;
                self.send(
                    &id,
                    &desk_core::prompts::run_command(window, date),
                    vault,
                    now,
                )
                .await?;
            }
            ChatCommand::Stop { chat_id } => {
                if self.meta(&chat_id).is_some_and(|m| m.busy) {
                    if let Some(live) = self.sessions.get(&chat_id) {
                        let _ = live.session.stop();
                    }
                }
            }
            ChatCommand::Rename { chat_id, title } => {
                self.ensure_loaded(&chat_id).await?;
                let Some(m) = self.meta_mut(&chat_id) else {
                    return Err(UserError::new(keys::UNKNOWN_CHAT));
                };
                m.title = clean_title(&title);
                self.persist(&chat_id);
                self.dirty = true;
            }
            ChatCommand::Delete { chat_id } => {
                if self.meta(&chat_id).is_none() {
                    return Err(UserError::new(keys::UNKNOWN_CHAT));
                }
                if let Some(live) = self.sessions.remove(&chat_id) {
                    live.session.shutdown();
                }
                self.metas.retain(|m| m.id != chat_id);
                self.transcripts.remove(&chat_id);
                if self.open.as_deref() == Some(chat_id.as_str()) {
                    self.open = None;
                }
                if let Some(w) = &self.writes {
                    let _ = w.send(WriteOp::Delete(chat_id));
                }
                self.dirty = true;
            }
        }
        Ok(())
    }

    fn notice(&mut self, id: &str, key: &str, vars: Vec<(String, String)>) {
        if let Some(t) = self.transcripts.get_mut(id) {
            Arc::make_mut(t).notice(key, vars);
        }
        if let Some(m) = self.meta_mut(id) {
            m.busy = false;
        }
        self.persist(id);
        self.dirty = true;
    }

    async fn send(
        &mut self,
        id: &str,
        text: &str,
        vault: Option<&Path>,
        now: DateTime<Utc>,
    ) -> Result<(), UserError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let Some(meta) = self.meta(id) else {
            return Err(UserError::new(keys::UNKNOWN_CHAT));
        };
        if meta.busy {
            return Err(UserError::new(keys::BUSY));
        }
        self.ensure_loaded(id).await?;
        if let Some(t) = self.transcripts.get_mut(id) {
            Arc::make_mut(t).push_user(text, now);
        }
        if let Some(m) = self.meta_mut(id) {
            if m.title.is_empty() {
                m.title = auto_title(text);
            }
            m.updated_at = now;
            m.busy = true;
        }
        self.dirty = true;
        let Some(vault) = vault else {
            self.notice(id, keys::NO_VAULT, Vec::new());
            return Ok(());
        };
        // One retry: a process that died idle may not have reported yet.
        for _ in 0..2 {
            if !self.sessions.contains_key(id) {
                if let Err((key, vars)) = self.spawn(id, vault) {
                    self.notice(id, key, vars);
                    return Ok(());
                }
            }
            if let Some(l) = self.sessions.get_mut(id) {
                l.last_text = text.to_string();
            }
            let sent = self.sessions.get(id).map(|l| l.session.send(text));
            match sent {
                Some(Ok(())) => {
                    self.persist(id);
                    return Ok(());
                }
                _ => {
                    self.sessions.remove(id);
                }
            }
        }
        self.notice(id, driver_keys::PROCESS_DIED, Vec::new());
        Ok(())
    }

    /// Start the chat's process and the task that brings its events back.
    #[allow(clippy::type_complexity)]
    fn spawn(
        &mut self,
        id: &str,
        vault: &Path,
    ) -> Result<(), (&'static str, Vec<(String, String)>)> {
        let Some(meta) = self.meta(id) else {
            return Err((keys::UNKNOWN_CHAT, Vec::new()));
        };
        let Some(claude) = find_claude(&self.launch) else {
            return Err((keys::NO_CLAUDE, Vec::new()));
        };
        let resumed = meta.session_id.is_some();
        let cfg = session_config(
            meta.kind,
            vault,
            &claude,
            meta.session_id.as_deref(),
            &self.launch,
        );
        let (session, mut rx) = ClaudeSession::spawn(&cfg).map_err(|e| {
            let key = if e.kind() == std::io::ErrorKind::NotFound {
                keys::NO_CLAUDE
            } else {
                keys::SPAWN
            };
            (key, vec![("detail".to_string(), e.to_string())])
        })?;
        self.gen += 1;
        let (gen, chat, forward) = (self.gen, id.to_string(), Arc::clone(&self.forward));
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                forward(ChatMsg::Event {
                    chat: chat.clone(),
                    gen,
                    ev,
                });
            }
            forward(ChatMsg::Closed { chat, gen });
        });
        self.sessions.insert(
            id.to_string(),
            Live {
                session,
                gen,
                resumed,
                last_text: String::new(),
                retry: false,
            },
        );
        Ok(())
    }

    /// Fold what a session or the writer reported. `Err` = for the error list.
    pub fn on_msg(
        &mut self,
        msg: ChatMsg,
        vault: Option<&Path>,
        now: DateTime<Utc>,
    ) -> Result<(), UserError> {
        match msg {
            ChatMsg::StoreFailed { detail } => {
                return Err(UserError::new(keys::STORE).var("detail", detail))
            }
            ChatMsg::Event { chat, gen, ev } => {
                let Some(live) = self.sessions.get_mut(&chat).filter(|l| l.gen == gen) else {
                    return Ok(()); // a stopped session's leftovers, or a deleted chat
                };
                // C3 (TEST_PLAN): the resumed conversation is gone on the CLI's
                // side. Forget the id and run the same turn once more without
                // --resume, instead of showing the error and failing again.
                let resume_gone = matches!(&ev, DriverEvent::Error { key, .. } if key == driver_keys::RESUME_MISSING);
                if resume_gone && live.resumed && !live.retry {
                    live.retry = true;
                    if let Some(m) = self.metas.iter_mut().find(|m| m.id == chat) {
                        m.session_id = None;
                    }
                    self.persist(&chat);
                    return Ok(());
                }
                let retry = live.retry && matches!(ev, DriverEvent::Done { .. });
                let (Some(meta), Some(t)) = (
                    self.metas.iter_mut().find(|m| m.id == chat),
                    self.transcripts.get_mut(&chat),
                ) else {
                    return Ok(());
                };
                let applied = apply_event(meta, Arc::make_mut(t), ev, now);
                if applied.resume_missing {
                    // Not resumable and already retried: next turn starts fresh.
                    meta.session_id = None;
                }
                if applied.persist || applied.resume_missing {
                    self.persist(&chat);
                }
                self.dirty = true;
                if retry {
                    self.retry_fresh(&chat, vault);
                }
            }
            ChatMsg::Closed { chat, gen } => {
                if self.sessions.get(&chat).map(|l| l.gen) == Some(gen) {
                    self.sessions.remove(&chat);
                }
                // The driver always ends a turn with Done; this is the guard
                // for a stream that ended without one.
                if self.meta(&chat).is_some_and(|m| m.busy) && !self.sessions.contains_key(&chat) {
                    if let Some(t) = self.transcripts.get_mut(&chat) {
                        Arc::make_mut(t).close_live();
                    }
                    self.notice(&chat, driver_keys::PROCESS_DIED, Vec::new());
                }
            }
        }
        Ok(())
    }

    /// The retry of C3: a new process without `--resume`, the same text.
    fn retry_fresh(&mut self, id: &str, vault: Option<&Path>) {
        let Some(old) = self.sessions.remove(id) else {
            return;
        };
        old.session.shutdown();
        let Some(vault) = vault else {
            self.notice(id, keys::NO_VAULT, Vec::new());
            return;
        };
        if let Err((key, vars)) = self.spawn(id, vault) {
            self.notice(id, key, vars);
            return;
        }
        let sent = self.sessions.get_mut(id).map(|l| {
            l.last_text = old.last_text.clone();
            l.session.send(old.last_text.clone())
        });
        if matches!(sent, Some(Ok(()))) {
            if let Some(m) = self.meta_mut(id) {
                m.busy = true;
            }
            self.dirty = true;
        } else {
            self.notice(id, driver_keys::PROCESS_DIED, Vec::new());
        }
    }
}
