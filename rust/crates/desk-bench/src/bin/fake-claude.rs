//! A stand-in for `claude -p --output-format stream-json --verbose`.
//!
//!   fake-claude [claude's own flags…] [--events N] [--rate EPS] [--seed S]
//!               [--tool-every K] [--die-after K] [--garbage-every M]
//!               [--stall-ms MS] [--stall-at K]
//!
//! Every option also reads `FAKE_CLAUDE_<NAME>` (EVENTS, RATE, SEED,
//! TOOL_EVERY, DIE_AFTER, GARBAGE_EVERY, STALL_MS, STALL_AT), because the
//! desk builds claude's argv itself and only the environment reaches us.
//! Flags it does not know (claude's) are ignored.
//!
//! - With `--input-format stream-json` it reads user frames from stdin, one
//!   turn per `{"type":"user",…}` line, and exits on EOF — like the real CLI
//!   under the desk. Otherwise it answers one turn and exits.
//! - A turn is exactly N lines ending in `result`; `system/init` comes once
//!   before the first turn.
//! - `--rate` = lines per second (0 = as fast as the pipe takes them).
//! - `--garbage-every M` INSERTS a broken line after every M-th real line
//!   (five kinds, rotating), so a consumer still has to see all N.
//! - `--die-after K` writes K real lines, then half of the next one with no
//!   newline, and exits with code 3 — the «обрыв посреди ответа» case.
//! - `--stall-ms MS` sleeps once before line `--stall-at` (default N/2) of
//!   the first turn — the «hangs, then carries on» case.
//! - `--version` and `auth status` answer like a logged-in CLI.

use anyhow::Result;
use desk_bench::cli::Args;
use desk_bench::stream::{garbage, init_line, turn, StreamSpec};
use std::io::{BufRead, Write};
use std::time::{Duration, Instant};

struct Opts {
    spec: StreamSpec,
    rate: f64,
    die_after: usize,
    garbage_every: usize,
    stall_ms: u64,
    stall_at: usize,
}

struct Out<W: Write> {
    w: W,
    real: usize,
    garbage: usize,
    /// Pacing restarts every turn: a user typing for a minute must not make
    /// the next answer arrive in one burst.
    start: Instant,
    turn_base: usize,
    o: Opts,
    stalled: bool,
}

impl<W: Write> Out<W> {
    /// Pace to `rate`: line i is due at i / rate seconds after the start.
    /// Deadline-based, so sleep overshoot (15 ms on Windows) does not drift.
    fn pace(&mut self) {
        if self.o.rate <= 0.0 {
            return;
        }
        let due = Duration::from_secs_f64((self.real - self.turn_base) as f64 / self.o.rate);
        let now = self.start.elapsed();
        if due > now {
            let _ = self.w.flush();
            std::thread::sleep(due - now);
        }
    }

    fn line(&mut self, l: &str, turn_pos: usize, first_turn: bool) -> Result<()> {
        if first_turn && !self.stalled && self.o.stall_ms > 0 && turn_pos == self.o.stall_at {
            self.stalled = true;
            self.w.flush()?;
            std::thread::sleep(Duration::from_millis(self.o.stall_ms));
            // The stall is not part of the paced schedule.
            self.start += Duration::from_millis(self.o.stall_ms);
        }
        if self.o.die_after > 0 && self.real >= self.o.die_after {
            let half = &l.as_bytes()[..l.len() / 2];
            self.w.write_all(half)?;
            self.w.flush()?;
            eprintln!("fake-claude: dying after {} lines (--die-after)", self.real);
            std::process::exit(3);
        }
        self.pace();
        self.w.write_all(l.as_bytes())?;
        self.w.write_all(b"\n")?;
        self.real += 1;
        if self.o.garbage_every > 0 && self.real.is_multiple_of(self.o.garbage_every) {
            self.w.write_all(&garbage(self.garbage))?;
            self.w.write_all(b"\n")?;
            self.garbage += 1;
        }
        Ok(())
    }

    fn turn(&mut self, n: usize, cwd: &str) -> Result<()> {
        self.start = Instant::now();
        self.turn_base = self.real;
        if n == 0 {
            let init = init_line(self.o.spec.seed, cwd);
            self.line(&init, usize::MAX, false)?;
        }
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        for (i, l) in turn(&self.o.spec, n, &date).iter().enumerate() {
            self.line(l, i, n == 0)?;
        }
        self.w.flush()?;
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut a = Args::from_env();
    if a.flag("--version") || a.flag("-v") {
        println!("0.0.0-fake (fake-claude, desk-bench)");
        return Ok(());
    }
    if a.rest().first().map(String::as_str) == Some("auth") {
        println!(r#"{{"loggedIn":true,"authMethod":"claude.ai","fake":true}}"#);
        return Ok(());
    }
    let events: usize = a.parsed_or_env("--events", "FAKE_CLAUDE_EVENTS", 200)?;
    let o = Opts {
        spec: StreamSpec {
            events,
            seed: a.parsed_or_env("--seed", "FAKE_CLAUDE_SEED", 23)?,
            tool_every: a.parsed_or_env("--tool-every", "FAKE_CLAUDE_TOOL_EVERY", 4)?,
        },
        rate: a.parsed_or_env("--rate", "FAKE_CLAUDE_RATE", 0.0)?,
        die_after: a.parsed_or_env("--die-after", "FAKE_CLAUDE_DIE_AFTER", 0)?,
        garbage_every: a.parsed_or_env("--garbage-every", "FAKE_CLAUDE_GARBAGE_EVERY", 0)?,
        stall_ms: a.parsed_or_env("--stall-ms", "FAKE_CLAUDE_STALL_MS", 0)?,
        stall_at: a.parsed_or_env("--stall-at", "FAKE_CLAUDE_STALL_AT", events / 2)?,
    };
    let stdin_turns = a.value("--input-format")?.as_deref() == Some("stream-json");
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let stdout = std::io::stdout();
    let mut out = Out {
        w: std::io::BufWriter::with_capacity(64 * 1024, stdout.lock()),
        real: 0,
        garbage: 0,
        start: Instant::now(),
        turn_base: 0,
        o,
        stalled: false,
    };
    if !stdin_turns {
        return out.turn(0, &cwd);
    }
    let mut n = 0;
    for l in std::io::stdin().lock().lines() {
        let l = l?;
        let is_user = serde_json::from_str::<serde_json::Value>(&l)
            .map(|v| v["type"] == "user")
            .unwrap_or(false);
        if is_user {
            out.turn(n, &cwd)?;
            n += 1;
        }
    }
    Ok(())
}
