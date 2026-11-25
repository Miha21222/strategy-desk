//! desk-bench: owned by the QA-perf agent. See docs/rust/TEAM.md → «QA · перформанс»
//! and docs/rust/PERF.md for what each piece measures and why.
//!
//! The crate is built so it works before desk-core / desk-app / desk-ui have
//! any code: everything here either generates load (a synthetic vault, a fake
//! `claude` stream, a slow writer) or measures a lower bound with plain
//! `serde_json`. The places that switch to the real implementation once it
//! lands are marked `HOOK(desk-core)` / `HOOK(desk-app)`.

pub mod cli;
pub mod day;
pub mod machine;
pub mod png;
pub mod report;
pub mod rng;
pub mod stats;
pub mod stream;
pub mod vault_gen;
