//! desk-ui: the egui window of Strategy Desk. It draws the last snapshot and
//! sends commands; it never waits on disk, network or another process.

pub mod app;
pub mod chat;
pub mod cli;
pub mod clock;
pub mod connect;
pub mod fonts;
pub mod gauge;
pub mod i18n;
mod i18n_connect;
mod i18n_ru;
pub mod opts;
pub mod perf;
pub mod screen;
pub mod settings;
pub mod theme;

#[cfg(test)]
mod snapshots;
#[cfg(test)]
mod snapshots_chat;
#[cfg(test)]
mod snapshots_connect;
