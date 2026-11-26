//! The chat as desk-app publishes it (`Snapshot::chats`, CONTRACTS «Чат
//! (этап 5)»), plus the few questions the UI asks of it.

use std::sync::Arc;

use chrono::NaiveDate;
use desk_app::Snapshot;
pub use desk_app::{ChatCommand, ChatItem, ChatKind, ChatMeta, ChatsView, Transcript};
use desk_core::prompts::macro_chat_title;

use crate::i18n::t;

pub type ChatId = String;

/// Questions the UI asks of the published view.
pub trait ViewExt {
    fn meta(&self, id: &str) -> Option<&ChatMeta>;
    /// The day's macro chat, if it exists yet.
    fn macro_for(&self, date: NaiveDate) -> Option<&ChatMeta>;
    fn transcript(&self, id: &str) -> Option<&Arc<Transcript>>;
}

impl ViewExt for ChatsView {
    fn meta(&self, id: &str) -> Option<&ChatMeta> {
        self.list.iter().find(|m| m.id == id)
    }

    fn macro_for(&self, date: NaiveDate) -> Option<&ChatMeta> {
        self.list
            .iter()
            .find(|m| m.kind == ChatKind::Macro && m.date == Some(date))
    }

    fn transcript(&self, id: &str) -> Option<&Arc<Transcript>> {
        self.transcripts.get(id)
    }
}

/// The last thing the agent said (or the app noticed): the agent strip on
/// «Обзор дня» shows it under the button.
pub fn last_word(t: &Transcript) -> Option<&ChatItem> {
    t.items
        .iter()
        .rev()
        .find(|i| matches!(i, ChatItem::Assistant { .. } | ChatItem::Notice { .. }))
}

/// A chat's name on screen. An «Ask» chat has no title until its first
/// message names it; a macro chat is «Обзор дня DD.MM».
pub fn title_of(m: &ChatMeta) -> String {
    if !m.title.trim().is_empty() {
        return m.title.clone();
    }
    match (m.kind, m.date) {
        (ChatKind::Macro, Some(d)) => macro_chat_title(d),
        _ => t("sidebar.newChat").to_owned(),
    }
}

/// The chats of a snapshot.
pub fn chats_of(snap: &Snapshot) -> Arc<ChatsView> {
    snap.chats.clone()
}
