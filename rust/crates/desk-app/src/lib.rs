//! desk-app: everything that waits on something (disk, processes, network).
//!
//! The UI never calls into this crate synchronously on the frame path: it sends
//! a `Command` and renders the latest `Snapshot`. See docs/rust/CONTRACTS.md.

pub mod app;
pub mod claude;
pub mod day;
pub mod macro_env;
pub mod shots;
pub mod vault_write;
pub mod watch;

pub use app::{AppConfig, AppHandle, ChatCommand, Command, ShotEntry, Snapshot, UserError};
pub use day::DayMark;
