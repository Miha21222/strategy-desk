//! desk-core: the pure half of Strategy Desk.
//!
//! Ported from `src/shared/macro.ts` and friends. Rule: nothing in this crate
//! touches the disk, the network, a process or the clock. Callers hand in bytes
//! and a `now`; functions hand back values. That is what makes it testable
//! against the golden files in `rust/fixtures/golden/`.
//!
//! | Module | From TS |
//! | --- | --- |
//! | [`vault`] | the types of `src/shared/macro.ts` |
//! | [`normalize`] | `normalize*`, `trimCollect`, `lessonsTail`, `visibleIdeas`, `humanHit` |
//! | [`day`] | `loadDay` / `listDays` of `src/main/macro-state.ts`, without the disk |
//! | [`zones`], [`gauge`] | `zoneOf`, `zoneSegments`, `needleAngle`, `arcPath`, `splitFactors` |
//! | [`windows`] | `windowState`, `defaultWindow`, `kyivNow` + TZ item 21 (exchange clocks) |
//! | [`stats`] | `parseCsv`, `factorStats`, `fmt` |
//! | [`names`] | file names, `dateOfStateFile`, `isFriday`, `isShotPath`, `resolveNote` (pure half) |
//! | [`shots`] | `legendColor`, `mainShot`, `shotTabLabel`, `shotsErrorText`, `readShot` (pure half) |
//! | [`prompts`] | `runCommand`, `discussPrompt`, `macroChatTitle` |
//! | [`settings`] | `DEFAULT_SETTINGS`, `macroMcpFor`, `legacyConfigPatch` of `src/shared/defaults.ts` |
//! | [`preflight`] | `buildChecks` of `src/shared/preflight.ts` (no gt-rag, official MCP, Codex, legacy) |
//! | [`schedule`] | the Task Scheduler jobs and which window a firing is for (TZ items 18, 21) |
//! | [`wizard`] | `src/renderer/setup.ts`: pages, skips, limitation notes, auto-started tasks |
//! | [`owner`] | `writeHumanRead` / `writeDayAnswers` (pure half: the record and its text) |
//! | [`js`] | the JavaScript semantics the TS relied on (`String()`, `trim`, `Math.round`, …) |

pub mod day;
pub mod gauge;
pub mod js;
pub mod names;
pub mod normalize;
pub mod owner;
pub mod preflight;
pub mod prompts;
pub mod schedule;
pub mod settings;
pub mod shots;
pub mod stats;
pub mod vault;
pub mod windows;
pub mod wizard;
pub mod zones;

pub use day::{list_days, load_day, Day, DayInput, MacroIndex, RegionSnapshot};
pub use vault::{HumanSide, Raw, Region, Side};
pub use windows::{window_state, WindowId, WindowState};

/// The errors desk-core can report. Reading a day never fails (a broken
/// file is a missing file, as in the TS); these are for the callers that
/// need a checked value.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// Not an ISO calendar date (`YYYY-MM-DD` that exists).
    #[error("bad date: {0:?}")]
    BadDate(String),
    /// A record could not be written as JSON.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
