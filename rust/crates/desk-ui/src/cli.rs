//! Command line of `strategy-desk`. Hand-rolled: three flags and one
//! subcommand do not justify a parser crate in the binary.
//!
//! ```text
//! strategy-desk                         open the window (vault from --vault or DESK_VAULT)
//! strategy-desk --demo                  … on the demo vault (rust/fixtures/golden/vault), clock at its «today»
//! strategy-desk --now 2026-10-26T10:15  … with the clock fixed (Kyiv wall time; also DESK_NOW)
//! strategy-desk --bench-first-frame     open, print {"first_frame_ms":…} after the first frame, exit 0
//! strategy-desk --bench-data-ready      open, wait for the day, print {"first_frame_ms","data_ready_ms",…}, exit 0
//! strategy-desk --bench-switch N        N day switches, print {"switch_ms":[…]}, exit 0
//! strategy-desk run --window eu|us|review [--date YYYY-MM-DD]
//!                                       headless scheduled review (desk_app::run)
//! ```

use std::ffi::OsString;

use chrono::NaiveDate;
use desk_core::windows::WindowId;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowOpts {
    pub demo: bool,
    pub bench_first_frame: bool,
    /// `--bench-data-ready`: like `--bench-first-frame`, waiting for the day.
    pub bench_data_ready: bool,
    /// `--bench-switch N`: N day switches, then exit.
    pub bench_switch: Option<u32>,
    pub vault: Option<PathBuf>,
    /// `--now`, unparsed (the app parses and reports it).
    pub now: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cli {
    Window(WindowOpts),
    /// `--date` set = a run by hand for that day (skips the «is it my minute» check).
    Run {
        window: WindowId,
        date: Option<NaiveDate>,
    },
    Help,
}

pub const USAGE: &str = "\
usage: strategy-desk [--vault DIR] [--demo] [--now TIME] [--bench-first-frame | --bench-data-ready]
                     [--bench-switch N]
       strategy-desk run --window eu|us|review [--date YYYY-MM-DD]

  --vault DIR          the Trading Strategy Assistant folder (else env DESK_VAULT)
  --demo               open the demo vault with the clock at its «today» (2026-10-26 10:15)
  --now TIME           fix the clock: 2026-10-26T10:15 (Kyiv) or RFC 3339 (else env DESK_NOW)
  --bench-first-frame  print {\"first_frame_ms\": …, \"rss_mb\": …} after the first frame and exit
  --bench-data-ready   wait until the day is drawn, print {\"first_frame_ms\", \"data_ready_ms\",
                       \"private_mb\", \"ws_mb\"} and exit
  --bench-switch N     switch the day N times, print {\"switch_ms\": […]} and exit
  run --window W       run the scheduled review without a window (W = eu | us | review);
                       --date D runs it by hand for that day; log in <data>/logs/run-D-W.log

env: DESK_FRAME_STATS=1 logs frame time p50/p95/p99 to stderr every 10 s;
     DESK_STARTUP_TRACE=1 prints one JSON line per start-up phase to stderr;
     DESK_OPT=vsync0,app_sync,app_late,shots_full,no_inter flips start-up choices;
     DESK_BENCH_AUTOCHAT=1 opens «+ Новый чат» after the first frame and sends one message
     (DESK_BENCH_AUTOCHAT_TEXT, else «Что из уроков подходит к сегодняшнему дню?»)";

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
        let rest: Vec<String> = it.map(str::to_owned).collect();
        if rest.iter().any(|a| a == "-h" || a == "--help") {
            return Ok(Cli::Help);
        }
        // `--window=us` → `--window us`: the scheduler writes the spaced form.
        let rest = rest.into_iter().flat_map(|a| match a.split_once('=') {
            Some((k, v)) if k.starts_with("--") => vec![k.to_owned(), v.to_owned()],
            _ => vec![a],
        });
        return desk_app::run::parse_args(rest).map(|(window, date)| Cli::Run { window, date });
    }

    let mut opts = WindowOpts::default();
    while let Some(a) = it.next() {
        // `--flag value` or `--flag=value`.
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (a, None),
        };
        let mut value = |name: &str| -> Result<String, String> {
            inline
                .map(str::to_owned)
                .or_else(|| it.next().map(str::to_owned))
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "--demo" => opts.demo = true,
            "--bench-first-frame" => opts.bench_first_frame = true,
            "--bench-data-ready" => opts.bench_data_ready = true,
            "--bench-switch" => {
                let v = value("--bench-switch")?;
                let n =
                    v.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(|| {
                        format!("--bench-switch needs a positive count, not {v:?}")
                    })?;
                opts.bench_switch = Some(n);
            }
            "--vault" => opts.vault = Some(PathBuf::from(value("--vault")?)),
            "--now" => opts.now = Some(value("--now")?),
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
                bench_first_frame: true,
                ..WindowOpts::default()
            }))
        );
    }

    #[test]
    fn values_both_spellings() {
        let Ok(Cli::Window(o)) = p(&[
            "--vault",
            "/v",
            "--now=2026-10-26T10:15",
            "--bench-switch",
            "20",
        ]) else {
            panic!("parse failed")
        };
        assert_eq!(o.vault, Some(PathBuf::from("/v")));
        assert_eq!(o.now.as_deref(), Some("2026-10-26T10:15"));
        assert_eq!(o.bench_switch, Some(20));
        assert!(p(&["--bench-switch", "0"]).is_err());
        assert!(p(&["--bench-switch", "x"]).is_err());
        assert!(p(&["--vault"]).is_err());
        let Ok(Cli::Window(o)) = p(&["--bench-data-ready"]) else {
            panic!("parse failed")
        };
        assert!(o.bench_data_ready && !o.bench_first_frame);
    }

    #[test]
    fn run_window_both_spellings() {
        assert_eq!(
            p(&["run", "--window", "eu"]),
            Ok(Cli::Run {
                window: WindowId::Eu,
                date: None
            })
        );
        assert_eq!(
            p(&["run", "--window=us"]),
            Ok(Cli::Run {
                window: WindowId::Us,
                date: None
            })
        );
        // What the Task Scheduler registers for the evening, and a run by hand.
        assert_eq!(
            p(&["run", "--window", "review", "--date", "2026-11-27"]),
            Ok(Cli::Run {
                window: WindowId::Review,
                date: NaiveDate::from_ymd_opt(2026, 11, 27)
            })
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
