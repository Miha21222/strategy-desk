//! strategy-desk.exe entry point. `strategy-desk run --window eu|us` is the
//! headless path the Task Scheduler uses; no arguments opens the window.

use std::process::ExitCode;
use std::time::Instant;

use desk_ui::cli::{self, Cli, USAGE};

fn main() -> ExitCode {
    // First thing: the clock `--bench-first-frame` measures from.
    let started = Instant::now();
    desk_ui::perf::trace::init(started);
    desk_ui::perf::trace::mark("main");

    match cli::parse(std::env::args_os().skip(1)) {
        Ok(Cli::Window(opts)) => match desk_ui::app::run(started, opts) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("strategy-desk: {e:#}");
                ExitCode::FAILURE
            }
        },
        Ok(Cli::Run { window }) => {
            eprintln!(
                "strategy-desk run --window {}: not implemented yet",
                window.as_str()
            );
            ExitCode::from(2)
        }
        Ok(Cli::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("strategy-desk: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
