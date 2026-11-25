//! desk-app: everything that waits on something (disk, processes, network).
//!
//! The UI never calls into this crate synchronously on the frame path: it sends
//! a `Command` and renders the latest `Snapshot`. See docs/rust/CONTRACTS.md.
