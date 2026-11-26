//! Performance hooks for QA (docs/rust/TEAM.md «Метрики», CONTRACTS «Хуки»):
//!
//! * `--bench-first-frame`: one JSON line on stdout once the first frame has
//!   been presented, then exit 0.
//! * `--bench-data-ready`: the same, but waits until the day is drawn —
//!   `{"first_frame_ms", "data_ready_ms", "private_mb", "ws_mb"}`.
//! * `DESK_FRAME_STATS=1`: p50 / p95 / p99 of the frame CPU time on stderr
//!   every 10 s, from a reporter thread (it never asks for a frame itself).
//! * `DESK_STARTUP_TRACE=1`: one JSON line per start-up phase on stderr,
//!   with the time since `main` and the process memory (see [`trace`]).

use std::io::Write as _;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui;

const REPORT_EVERY: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------- memory

/// Process memory, MiB: Private Bytes / Working Set on Windows; on Linux
/// `RssAnon` (anonymous resident memory, the nearest thing to private
/// bytes) and `VmRSS`. Elsewhere `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mem {
    pub private_mb: Option<f64>,
    pub ws_mb: Option<f64>,
}

pub fn mem() -> Mem {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let kb = |key: &str| -> Option<f64> {
            status
                .lines()
                .find_map(|l| l.strip_prefix(key))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse::<f64>()
                .ok()
                .map(|k| k / 1024.0)
        };
        Mem {
            private_mb: kb("RssAnon:"),
            ws_mb: kb("VmRSS:"),
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        // SAFETY: PROCESS_MEMORY_COUNTERS_EX is plain data (all-zero is
        // valid) and starts with PROCESS_MEMORY_COUNTERS, which is what the
        // call takes; `cb` tells it the larger struct is there. The pseudo
        // handle of GetCurrentProcess needs no closing.
        let c = unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            c.cb = cb;
            let p = (&mut c as *mut PROCESS_MEMORY_COUNTERS_EX).cast::<PROCESS_MEMORY_COUNTERS>();
            (GetProcessMemoryInfo(GetCurrentProcess(), p, cb) != 0).then_some(c)
        };
        let mb = |b: usize| b as f64 / (1024.0 * 1024.0);
        Mem {
            private_mb: c.map(|c| mb(c.PrivateUsage)),
            ws_mb: c.map(|c| mb(c.WorkingSetSize)),
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Mem::default()
    }
}

/// Working set / RSS, MiB (the `rss_mb` of `--bench-first-frame`).
pub fn rss_mb() -> Option<f64> {
    mem().ws_mb
}

fn json_mb(v: Option<f64>) -> String {
    v.map_or_else(|| "null".to_owned(), |v| format!("{v:.1}"))
}

fn print_line(line: &str) {
    let mut out = std::io::stdout().lock();
    // Best effort: if stdout is closed nobody is listening anyway.
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// The JSON line of `--bench-first-frame`. Hand-written: desk-ui has no serde.
pub fn bench_line(first_frame_ms: f64, rss_mb: Option<f64>) -> String {
    format!(
        "{{\"first_frame_ms\": {first_frame_ms:.1}, \"rss_mb\": {}}}",
        json_mb(rss_mb)
    )
}

/// The JSON line of `--bench-data-ready`.
pub fn data_ready_line(first_frame_ms: f64, data_ready_ms: Option<f64>, m: Mem) -> String {
    let ready = data_ready_ms.map_or_else(|| "null".to_owned(), |v| format!("{v:.1}"));
    format!(
        "{{\"first_frame_ms\": {first_frame_ms:.1}, \"data_ready_ms\": {ready}, \"private_mb\": {}, \"ws_mb\": {}}}",
        json_mb(m.private_mb),
        json_mb(m.ws_mb)
    )
}

// ---------------------------------------------------------------- start-up trace

/// `DESK_STARTUP_TRACE=1`: phases of the start-up on stderr, one JSON line
/// each, `{"trace": "<phase>", "ms": <since main>, "private_mb", "ws_mb"}`.
/// Every phase is printed once; marks are cheap no-ops when tracing is off.
pub mod trace {
    use super::*;

    struct Trace {
        started: Instant,
        seen: Mutex<Vec<&'static str>>,
    }

    static TRACE: OnceLock<Option<Trace>> = OnceLock::new();

    /// Call first thing in `main`, with the clock `main` started.
    pub fn init(started: Instant) {
        let on = std::env::var_os("DESK_STARTUP_TRACE").is_some_and(|v| v == "1");
        let _ = TRACE.set(on.then(|| Trace {
            started,
            seen: Mutex::new(Vec::new()),
        }));
    }

    pub fn enabled() -> bool {
        matches!(TRACE.get(), Some(Some(_)))
    }

    /// The JSON of one phase.
    pub fn line(phase: &str, ms: f64, m: Mem, extra: &str) -> String {
        format!(
            "{{\"trace\": \"{phase}\", \"ms\": {ms:.1}, \"private_mb\": {}, \"ws_mb\": {}{extra}}}",
            json_mb(m.private_mb),
            json_mb(m.ws_mb)
        )
    }

    /// Print `phase` once (later calls with the same phase are ignored).
    pub fn mark(phase: &'static str) {
        mark_with(phase, "");
    }

    /// [`mark`] with extra JSON fields (`, "k": v`).
    pub fn mark_with(phase: &'static str, extra: &str) {
        let Some(Some(t)) = TRACE.get() else {
            return;
        };
        {
            let mut seen = t.seen.lock().unwrap_or_else(|e| e.into_inner());
            if seen.contains(&phase) {
                return;
            }
            seen.push(phase);
        }
        let ms = t.started.elapsed().as_secs_f64() * 1000.0;
        eprintln!("{}", line(phase, ms, mem(), extra));
    }

    /// `idle_10s`: memory once the window has sat still for 10 s.
    pub fn idle_after_first_frame() {
        if !enabled() {
            return;
        }
        let spawned = std::thread::Builder::new()
            .name("desk-trace".into())
            .spawn(|| {
                std::thread::sleep(Duration::from_secs(10));
                mark("idle_10s");
            });
        if spawned.is_err() {
            mark("idle_10s_unavailable");
        }
    }
}

// ---------------------------------------------------------------- frame stats

/// Frame-time percentiles over a window of samples (milliseconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Percentiles {
    pub n: usize,
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
}

/// Nearest-rank percentiles; `None` for no samples.
pub fn percentiles(samples: &mut [f32]) -> Option<Percentiles> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_by(f32::total_cmp);
    let n = samples.len();
    let rank = |q: f32| {
        let i = ((q * n as f32).ceil() as usize).clamp(1, n) - 1;
        samples[i]
    };
    Some(Percentiles {
        n,
        p50: rank(0.50),
        p95: rank(0.95),
        p99: rank(0.99),
        max: samples[n - 1],
    })
}

/// The stderr line for one 10-s window; zero frames is a line too (QA counts
/// idle frames per minute from these).
pub fn stats_line(p: Option<Percentiles>) -> String {
    let p = p.unwrap_or(Percentiles {
        n: 0,
        p50: 0.0,
        p95: 0.0,
        p99: 0.0,
        max: 0.0,
    });
    format!(
        "frame_stats n={} p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
        p.n, p.p50, p.p95, p.p99, p.max
    )
}

/// Samples shared with the reporter thread.
type Samples = Arc<Mutex<Vec<f32>>>;

fn start_reporter(samples: Samples) {
    let spawned = std::thread::Builder::new()
        .name("desk-frame-stats".into())
        .spawn(move || loop {
            std::thread::sleep(REPORT_EVERY);
            let mut window =
                std::mem::take(&mut *samples.lock().unwrap_or_else(|e| e.into_inner()));
            eprintln!("{}", stats_line(percentiles(&mut window)));
        });
    if let Err(e) = spawned {
        eprintln!("strategy-desk: DESK_FRAME_STATS: no reporter thread ({e})");
    }
}

// ---------------------------------------------------------------- per frame

/// What the app does with timing, decided once in `run`.
pub struct Perf {
    started: Instant,
    bench_first_frame: bool,
    bench_data_ready: bool,
    frames: u64,
    first_frame_ms: Option<f64>,
    /// `Some(NaN)` = the day was drawn, waiting for that frame to be presented.
    data_ready_ms: Option<f64>,
    samples: Option<Samples>,
}

/// How long `--bench-data-ready` waits for a day before it gives up.
const DATA_READY_TIMEOUT: Duration = Duration::from_secs(20);

impl Perf {
    pub fn new(
        started: Instant,
        bench_first_frame: bool,
        bench_data_ready: bool,
        frame_stats: bool,
    ) -> Self {
        let samples = frame_stats.then(Samples::default);
        if let Some(s) = &samples {
            start_reporter(Arc::clone(s));
        }
        Self {
            started,
            bench_first_frame,
            bench_data_ready,
            frames: 0,
            first_frame_ms: None,
            data_ready_ms: None,
            samples,
        }
    }

    fn since_start(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }

    /// The day is on screen (called after the frame that drew it).
    pub fn day_drawn(&mut self, ctx: &egui::Context) {
        if self.data_ready_ms.is_none() {
            // Measured like the first frame: at the start of the next pass.
            self.data_ready_ms = Some(f64::NAN);
            ctx.request_repaint();
        }
    }

    /// Call at the top of every `App::logic`.
    pub fn on_frame(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.frames += 1;
        if self.frames == 1 {
            // Ask for a second pass: when it starts, the first frame has
            // gone through paint + swap, i.e. it is on screen.
            ctx.request_repaint();
        } else if self.frames == 2 {
            let ms = self.since_start();
            self.first_frame_ms = Some(ms);
            trace::mark("first_frame_presented");
            trace::idle_after_first_frame();
            if self.bench_first_frame && !self.bench_data_ready {
                print_line(&bench_line(ms, rss_mb()));
                // Exit right here instead of a polite close: the number is
                // taken, and a driver that is slow to tear the window down
                // must not turn a passing bench into a timeout.
                std::process::exit(0);
            }
        }
        if self.data_ready_ms.is_some_and(f64::is_nan) {
            let ms = self.since_start();
            self.data_ready_ms = Some(ms);
            trace::mark("day_presented");
        }
        if self.bench_data_ready {
            let first = self.first_frame_ms.unwrap_or(f64::NAN);
            if let Some(ready) = self.data_ready_ms.filter(|v| !v.is_nan()) {
                print_line(&data_ready_line(first, Some(ready), mem()));
                std::process::exit(0);
            }
            if self.started.elapsed() > DATA_READY_TIMEOUT {
                print_line(&data_ready_line(first, None, mem()));
                std::process::exit(1);
            }
            // Keep polling while desk-app loads (it also wakes us on `rev`).
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        if let Some(s) = &self.samples {
            // `cpu_usage` is the previous frame: ui + tessellation + paint,
            // without the vsync wait. `None` on the very first frame.
            if let Some(cpu) = frame.info().cpu_usage {
                s.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(cpu * 1000.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_nearest_rank() {
        let mut v: Vec<f32> = (1..=100).map(|i| i as f32).collect();
        let p = percentiles(&mut v).unwrap();
        assert_eq!(
            (p.n, p.p50, p.p95, p.p99, p.max),
            (100, 50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(percentiles(&mut []), None);
        let one = percentiles(&mut [7.0]).unwrap();
        assert_eq!((one.p50, one.p99), (7.0, 7.0));
    }

    #[test]
    fn idle_window_still_reports() {
        assert_eq!(
            stats_line(None),
            "frame_stats n=0 p50=0.00ms p95=0.00ms p99=0.00ms max=0.00ms"
        );
    }

    #[test]
    fn lines_are_json() {
        assert_eq!(
            bench_line(123.456, Some(40.04)),
            r#"{"first_frame_ms": 123.5, "rss_mb": 40.0}"#
        );
        assert_eq!(
            bench_line(5.0, None),
            r#"{"first_frame_ms": 5.0, "rss_mb": null}"#
        );
        let m = Mem {
            private_mb: Some(50.0),
            ws_mb: None,
        };
        assert_eq!(
            data_ready_line(100.0, Some(180.26), m),
            r#"{"first_frame_ms": 100.0, "data_ready_ms": 180.3, "private_mb": 50.0, "ws_mb": null}"#
        );
        assert_eq!(
            trace::line("fonts", 12.0, m, r#", "opts": "a""#),
            r#"{"trace": "fonts", "ms": 12.0, "private_mb": 50.0, "ws_mb": null, "opts": "a"}"#
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn memory_is_known_on_linux() {
        let m = mem();
        assert!(m.ws_mb.is_some_and(|v| v > 0.0) && m.private_mb.is_some());
    }
}
