//! desk-ui: the egui window of Strategy Desk. It draws the last snapshot and
//! sends commands; it never waits on disk, network or another process.

pub mod app;
pub mod cli;
pub mod gauge;
pub mod geom;
pub mod i18n;
pub mod mock;
pub mod perf;
pub mod theme;

#[cfg(test)]
mod snapshots;
