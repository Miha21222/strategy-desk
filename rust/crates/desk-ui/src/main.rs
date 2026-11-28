//! strategy-desk.exe entry point. `strategy-desk run --window eu|us|review` is the
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
        Ok(Cli::Run { window, date }) => {
            let secrets = desk_app::secrets::KeyringStore::new();
            let code = desk_app::run::run(desk_app::run::RunArgs {
                window,
                date,
                now: chrono::Utc::now(),
                data_dir: None,
                secrets: &secrets,
                claude: Default::default(),
            });
            ExitCode::from(u8::try_from(code).unwrap_or(1))
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
