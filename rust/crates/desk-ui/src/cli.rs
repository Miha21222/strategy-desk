//! Command line of `strategy-desk`. Hand-rolled: three flags and one
//! subcommand do not justify a parser crate in the binary.
//!
//! ```text
//! strategy-desk                         open the window
//! strategy-desk --demo                  … with the debug slider under the gauge
//! strategy-desk --bench-first-frame     open, print {"first_frame_ms":…} after the first frame, exit 0
//! strategy-desk run --window eu|us      headless scheduled review (stage 6; exits 2 for now)
//! ```

use std::ffi::OsString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Eu,
    Us,
}

impl Region {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eu => "eu",
            Self::Us => "us",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowOpts {
    pub demo: bool,
    pub bench_first_frame: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    Window(WindowOpts),
    Run { window: Region },
    Help,
}

pub const USAGE: &str = "\
usage: strategy-desk [--demo] [--bench-first-frame]
       strategy-desk run --window eu|us

  --demo               show a slider that moves the gauge
  --bench-first-frame  print {\"first_frame_ms\": …, \"rss_mb\": …} after the first frame and exit
  run --window eu|us   run the scheduled review without a window (not implemented yet)

env: DESK_FRAME_STATS=1 logs frame time p50/p95/p99 to stderr every 10 s";

/// Parses everything after the program name. `Err` carries the message to
/// print above the usage.
pub fn parse<I>(args: I) -> Result<Cli, String>
where
    I: IntoIterator<Item = OsString>,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|a| a.into_string().map_err(|a| format!("not UTF-8: {a:?}")))
        .collect::<Result<_, _>>()?;
    let mut it = args.iter().map(String::as_str).peekable();

    if it.peek() == Some(&"run") {
        it.next();
        let mut window = None;
        while let Some(a) = it.next() {
            let value = match a {
                "--window" => it.next().ok_or("--window needs eu or us")?,
                _ if a.starts_with("--window=") => &a["--window=".len()..],
                "-h" | "--help" => return Ok(Cli::Help),
                other => return Err(format!("unknown argument for run: {other}")),
            };
            window = Some(match value {
                "eu" => Region::Eu,
                "us" => Region::Us,
                other => return Err(format!("--window must be eu or us, not {other:?}")),
            });
        }
        return window
            .map(|window| Cli::Run { window })
            .ok_or_else(|| "run needs --window eu|us".to_owned());
    }

    let mut opts = WindowOpts::default();
    for a in it {
        match a {
            "--demo" => opts.demo = true,
            "--bench-first-frame" => opts.bench_first_frame = true,
            "-h" | "--help" => return Ok(Cli::Help),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Cli::Window(opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Cli, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_args_is_the_window() {
        assert_eq!(p(&[]), Ok(Cli::Window(WindowOpts::default())));
    }

    #[test]
    fn flags_combine() {
        assert_eq!(
            p(&["--demo", "--bench-first-frame"]),
            Ok(Cli::Window(WindowOpts {
                demo: true,
                bench_first_frame: true
            }))
        );
    }

    #[test]
    fn run_window_both_spellings() {
        assert_eq!(
            p(&["run", "--window", "eu"]),
            Ok(Cli::Run { window: Region::Eu })
        );
        assert_eq!(
            p(&["run", "--window=us"]),
            Ok(Cli::Run { window: Region::Us })
        );
    }

    #[test]
    fn run_rejects_bad_input() {
        assert!(p(&["run"]).is_err());
        assert!(p(&["run", "--window"]).is_err());
        assert!(p(&["run", "--window", "asia"]).is_err());
        assert!(p(&["run", "--window", "eu", "--extra"]).is_err());
    }

    #[test]
    fn unknown_flag_is_an_error() {
        assert!(p(&["--fast"]).is_err());
    }

    #[test]
    fn help() {
        assert_eq!(p(&["--help"]), Ok(Cli::Help));
        assert_eq!(p(&["run", "-h"]), Ok(Cli::Help));
    }
}
