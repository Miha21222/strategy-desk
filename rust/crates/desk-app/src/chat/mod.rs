//! The chat (stage 5): the day's review chat and «+ Новый чат», both the
//! owner's own Claude Code in the vault (CONTRACTS «Чат (этап 5)»).
//!
//! * [`model`] — `ChatMeta`, `Transcript`, `ChatItem`, `ChatsView`, the fold.
//! * [`store`] — one JSON per chat in the app data dir, atomic.
//! * [`launch`] — which binary, prompt and environment a session gets.
//! * [`manager`] — the chats as the app's loop owns them.

pub mod launch;
pub mod manager;
pub mod model;
pub mod store;

pub use launch::ClaudeLaunch;
pub use model::{ChatCommand, ChatId, ChatItem, ChatKind, ChatMeta, ChatsView, Transcript};
