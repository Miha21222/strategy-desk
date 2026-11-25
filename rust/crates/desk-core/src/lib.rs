//! desk-core: the pure half of Strategy Desk.
//!
//! Ported from `src/shared/macro.ts` and friends. Rule: nothing in this crate
//! touches the disk, the network, a process or the clock. Callers hand in bytes
//! and a `now`; functions hand back values. That is what makes it testable
//! against the golden files in `rust/fixtures/golden/`.
