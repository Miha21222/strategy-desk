//! Performance hooks for QA (docs/rust/TEAM.md «Метрики»):
//!
//! * `--bench-first-frame`: one JSON line on stdout once the first frame has
//!   been presented, then exit 0.
//! * `DESK_FRAME_STATS=1`: p50 / p95 / p99 of the frame CPU time on stderr
//!   every 10 s.

use std::io::Write as _;
use std::time::{Duration, Instant};

use eframe::egui;

const REPORT_EVERY: Duration = Duration::from_secs(10);

/// Frame-time percentiles over a window of samples (milliseconds).
#[derive(Debug, Default)]
pub struct FrameStats {
    samples_ms: Vec<f32>,
    window_start: Option<Instant>,
}

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

impl FrameStats {
    /// Adds one frame; returns the window's percentiles when 10 s are up.
    pub fn push(&mut self, now: Instant, frame_ms: f32) -> Option<Percentiles> {
        let start = *self.window_start.get_or_insert(now);
        self.samples_ms.push(frame_ms);
        if now.duration_since(start) < REPORT_EVERY {
            return None;
        }
        let out = percentiles(&mut self.samples_ms);
        self.samples_ms.clear();
        self.window_start = Some(now);
        out
    }
}

/// What the app does with timing, decided once in `main`.
pub struct Perf {
    started: Instant,
    bench_first_frame: bool,
    frames: u64,
    stats: Option<FrameStats>,
}

impl Perf {
    pub fn new(started: Instant, bench_first_frame: bool, frame_stats: bool) -> Self {
        Self {
            started,
            bench_first_frame,
            frames: 0,
            stats: frame_stats.then(FrameStats::default),
        }
    }

    /// Call at the top of every `App::logic`.
    pub fn on_frame(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.frames += 1;
        if self.bench_first_frame {
            if self.frames == 1 {
                // Ask for a second pass: when it starts, the first frame has
                // gone through paint + swap, i.e. it is on screen.
                ctx.request_repaint();
            } else {
                let ms = self.started.elapsed().as_secs_f64() * 1000.0;
                let line = bench_line(ms, rss_mb());
                let mut out = std::io::stdout().lock();
                // Best effort: if stdout is closed nobody is listening anyway.
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
                // Exit right here instead of a polite close: the number is
                // taken, and a driver that is slow to tear the window down
                // must not turn a passing bench into a timeout.
                std::process::exit(0);
            }
        }
        if let Some(stats) = &mut self.stats {
            // `cpu_usage` is the previous frame: ui + tessellation + paint,
            // without the vsync wait. `None` on the very first frame.
            if let Some(cpu) = frame.info().cpu_usage {
                if let Some(p) = stats.push(Instant::now(), cpu * 1000.0) {
                    eprintln!(
                        "frame_stats n={} p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms",
                        p.n, p.p50, p.p95, p.p99, p.max
                    );
                }
            }
            // Keep at least one frame per window so an idle desk still reports.
            ctx.request_repaint_after(REPORT_EVERY);
        }
    }
}

/// The JSON line QA parses. Hand-written: desk-ui has no serde.
pub fn bench_line(first_frame_ms: f64, rss_mb: Option<f64>) -> String {
    let rss = rss_mb.map_or_else(|| "null".to_owned(), |v| format!("{v:.1}"));
    format!("{{\"first_frame_ms\": {first_frame_ms:.1}, \"rss_mb\": {rss}}}")
}

/// Resident set size (Linux) / working set (Windows), MiB.
pub fn rss_mb() -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let kb: f64 = status
            .lines()
            .find_map(|l| l.strip_prefix("VmRSS:"))?
            .trim()
            .trim_end_matches("kB")
            .trim()
            .parse()
            .ok()?;
        Some(kb / 1024.0)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: PROCESS_MEMORY_COUNTERS is plain data (all-zero is valid),
        // the pseudo handle of GetCurrentProcess needs no closing, and `cb`
        // is the size of the struct we pass.
        let counters = unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            c.cb = cb;
            (GetProcessMemoryInfo(GetCurrentProcess(), &mut c, cb) != 0).then_some(c)
        }?;
        Some(counters.WorkingSetSize as f64 / (1024.0 * 1024.0))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
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
    fn stats_report_once_per_window() {
        let t0 = Instant::now();
        let mut s = FrameStats::default();
        assert_eq!(s.push(t0, 1.0), None);
        assert_eq!(s.push(t0 + Duration::from_secs(5), 3.0), None);
        let p = s.push(t0 + Duration::from_secs(10), 2.0).unwrap();
        assert_eq!((p.n, p.p50, p.max), (3, 2.0, 3.0));
        // A fresh window starts after the report.
        assert_eq!(s.push(t0 + Duration::from_secs(11), 1.0), None);
    }

    #[test]
    fn bench_line_is_json() {
        assert_eq!(
            bench_line(123.456, Some(40.04)),
            r#"{"first_frame_ms": 123.5, "rss_mb": 40.0}"#
        );
        assert_eq!(
            bench_line(5.0, None),
            r#"{"first_frame_ms": 5.0, "rss_mb": null}"#
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rss_is_known_on_linux() {
        assert!(rss_mb().is_some_and(|m| m > 0.0));
    }
}
