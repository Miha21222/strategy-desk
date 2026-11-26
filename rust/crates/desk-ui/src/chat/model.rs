//! The chat as the UI reads it: CONTRACTS «Чат (этап 5)», field for field.
//!
//! desk-app's `ChatsView` is still empty (its backend is being built in
//! parallel), so these are the contract's types spelled out here. When the
//! backend lands they become `pub use desk_app::{ChatsView, ChatMeta, ...}`
//! and [`chats_of`] reads `snap.chats`; nothing else in the UI changes.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
pub use desk_app::app::ChatKind;
use desk_app::Snapshot;

pub type ChatId = String;

/// Everything the UI knows about the chats.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatsView {
    /// Newest first.
    pub list: Vec<ChatMeta>,
    /// The chat desk-app opened last (`New`, `OpenMacro`, `Run`).
    pub open: Option<ChatId>,
    /// Only the open and the recent chats, not all of them.
    pub transcripts: HashMap<ChatId, Arc<Transcript>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatMeta {
    pub id: ChatId,
    pub kind: ChatKind,
    pub title: String,
    /// The day of a macro chat (one per day); `None` for «+ Новый чат».
    pub date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub session_id: Option<String>,
    /// A turn is running: the chat takes no `Send` (the UI blocks it).
    pub busy: bool,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    pub items: Vec<ChatItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatItem {
    User {
        text: String,
        at: DateTime<Utc>,
    },
    /// `live` while the turn streams into it.
    Assistant {
        text: String,
        at: DateTime<Utc>,
        live: bool,
    },
    /// A tool call: `ok` is `None` while it runs.
    Tool {
        id: String,
        name: String,
        summary: String,
        ok: Option<bool>,
    },
    /// Something the app says itself: an i18n key plus variables.
    Notice {
        key: String,
        vars: Vec<(String, String)>,
    },
}

impl ChatsView {
    pub fn meta(&self, id: &str) -> Option<&ChatMeta> {
        self.list.iter().find(|m| m.id == id)
    }

    /// The day's macro chat, if it exists yet.
    pub fn macro_for(&self, date: NaiveDate) -> Option<&ChatMeta> {
        self.list
            .iter()
            .find(|m| m.kind == ChatKind::Macro && m.date == Some(date))
    }

    pub fn transcript(&self, id: &str) -> Option<&Arc<Transcript>> {
        self.transcripts.get(id)
    }
}

impl Transcript {
    /// The last thing the agent said (or the app noticed): the agent strip
    /// on «Обзор дня» shows it under the button.
    pub fn last_word(&self) -> Option<&ChatItem> {
        self.items
            .iter()
            .rev()
            .find(|i| matches!(i, ChatItem::Assistant { .. } | ChatItem::Notice { .. }))
    }
}

/// The chats of a snapshot. desk-app's `ChatsView` has no fields yet, so this
/// is empty until the backend lands (then: `snap.chats.clone()`).
pub fn chats_of(_snap: &Snapshot) -> ChatsView {
    ChatsView::default()
}
