//! The Claude Code driver: one `claude -p` process per chat session, spoken
//! to in stream-json over stdin/stdout.
//!
//! Replaces the Agent SDK (`src/main/drivers/claude.ts` + `session.ts`). The
//! SDK is a thin wrapper over exactly this protocol; talking to the CLI
//! directly drops the 226 MB bundled `claude.exe` and the Node runtime.
//!
//! * [`parse`] turns stdout lines into [`DriverEvent`]s (pure, no I/O).
//! * [`process`] owns the child: spawn, user turns on stdin, stop, death.
//! * [`config`] is what a session is started with, plus the macro profile.

pub mod config;
pub mod parse;
pub mod process;

pub use config::{ClaudeConfig, MACRO_ALLOWED, MACRO_DISALLOWED};
pub use parse::Translator;
pub use process::{ClaudeSession, SessionGone};

/// What a driver reports while a session runs (docs/rust/CONTRACTS.md).
#[derive(Debug, Clone, PartialEq)]
pub enum DriverEvent {
    /// The CLI told us which conversation it is in. Emitted once per id: the
    /// CLI repeats `system/init` on every turn with the same id.
    SessionStarted { session_id: String },
    /// A piece of the visible answer.
    TextDelta { text: String },
    /// The model called a tool. `summary` is one short line for the UI.
    ToolCall {
        id: String,
        name: String,
        summary: String,
    },
    /// A tool call finished; `id` matches a previous `ToolCall`.
    ToolResult { id: String, ok: bool },
    /// Accounting of the turn that just ended. `input`/`output` are the main
    /// loop's tokens of this turn; `cost_usd` is the CLI's running total for
    /// the process (cumulative across turns, see `SDKResultSuccess`).
    Usage {
        input: u64,
        output: u64,
        cost_usd: Option<f64>,
    },
    /// The turn is over. Always the last event of a turn.
    Done { stop: StopReason },
    /// Something the user should see. `key` is an i18n key, `detail` is raw
    /// diagnostic text (never a sentence meant for translation).
    Error { key: String, detail: String },
}

/// Why a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The model finished its answer.
    EndTurn,
    /// The user pressed «Стоп» (or the session was shut down mid-turn).
    Interrupted,
    /// `--max-turns` was hit.
    MaxTurns,
    /// The CLI reported an error result (auth, API, execution).
    Error,
    /// The process went away before the turn's result.
    ProcessDied,
}

/// i18n keys the driver puts into [`DriverEvent::Error`].
pub mod keys {
    /// The process exited (or was killed) before the turn's `result`.
    pub const PROCESS_DIED: &str = "chat.error.process_died";
    /// The process exited with a failure status while no turn was running.
    pub const PROCESS_EXITED: &str = "chat.error.process_exited";
    /// A `result` frame with an error subtype or `is_error: true`.
    pub const RESULT: &str = "chat.error.result";
    /// `--resume` named a conversation the CLI does not have (recorded from
    /// 2.1.332: a startup `result` with `errors: ["No conversation found …"]`).
    /// The caller may retry once without `--resume`.
    pub const RESUME_MISSING: &str = "chat.error.resume_missing";
    /// `error_max_turns`.
    pub const MAX_TURNS: &str = "chat.error.max_turns";
    /// An assistant frame carrying `error` (authentication_failed, rate_limit, …);
    /// the code is the detail.
    pub const API: &str = "chat.error.api";
}
