//! desk-app: everything that waits on something (disk, processes, network).
//!
//! The UI never calls into this crate synchronously on the frame path: it sends
//! a `Command` and renders the latest `Snapshot`. See docs/rust/CONTRACTS.md.

pub mod app;
pub mod chat;
pub mod claude;
pub mod core_wip;
pub mod day;
pub mod macro_env;
pub mod paths;
pub mod run;
pub mod runtime;
pub mod scheduler;
pub mod secrets;
pub mod settings_store;
pub mod shots;
pub mod status;
pub mod tasks;
pub mod tv_launch;
pub mod vault_template;
pub mod vault_write;
pub mod watch;

pub use app::{AppConfig, AppHandle, Command, ShotState, Snapshot, UserError};
pub use chat::{ChatCommand, ChatItem, ChatKind, ChatMeta, ChatsView, Transcript};
pub use day::DayMark;
