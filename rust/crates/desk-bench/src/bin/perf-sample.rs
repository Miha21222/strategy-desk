//! Everything desk-bench can measure in-process, written as a report.
//!
//!   perf-sample [--out FILE] [--label TEXT] [--commit SHA] [--exe PATH]
//!               [--installer PATH] [--runs N] [--idle-seconds S]
//!               [--representative] [--quick]
//!
//! Measures: day load (demo-size and real-size files), the whole ×10 vault
//! (list + parse), 10 000 stream-json lines parsed and piped through
//! fake-claude, the stress tools' self-checks, and — when `--exe` is given —
//! its size and, on Linux, start / idle RAM / idle CPU through /proc.
//! Windows start/RAM/CPU belong to scripts/rust/bench.ps1; this binary does
//! not try to replace it.
//!
//! A report is `representative: false` unless `--representative` is passed,
//! which only the owner's laptop run should do.

use anyhow::{Context, Result};
use desk_bench::cli::Args;
use desk_bench::day;
use desk_bench::report::{Build, Harness, Metric, Report, Status, SCHEMA};
use desk_bench::stream::{count_frames, turn, StreamSpec};
use desk_bench::vault_gen::{days_for_scale, generate, GenOptions};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn time_ms<T>(f: impl FnOnce() -> T) -> (f64, T) {
    let t = Instant::now();
    let r = f();
    (t.elapsed().as_secs_f64() * 1000.0, r)
}

fn repeat_ms(warmup: usize, runs: usize, mut f: impl FnMut()) -> Vec<f64> {
    for _ in 0..warmup {
        f();
    }
    (0..runs).map(|_| time_ms(&mut f).0).collect()
}

fn sibling(name: &str) -> Result<PathBuf> {
    let me = std::env::current_exe()?;
    let dir = me.parent().context("exe has no folder")?;
    Ok(dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX)))
}

/// One stress self-check: does the tool produce the case it claims to?
fn check(id: &str, ok: bool, detail: String) -> Value {
    json!({"id": id, "status": if ok { "pass" } else { "fail" }, "scope": "harness", "detail": detail})
}

fn pipe_fake_claude(fake: &Path, extra: &[&str]) -> Result<(f64, usize, usize, Option<i32>, bool)> {
    let t = Instant::now();
    let mut child = Command::new(fake)
        .args(["-p", "--output-format", "stream-json", "--verbose"])
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawn {}", fake.display()))?;
    let mut out = BufReader::new(child.stdout.take().context("no stdout")?);
    let (mut ok, mut bad) = (0usize, 0usize);
    let mut buf = Vec::new();
    let mut tail_without_newline = false;
    loop {
        buf.clear();
        let n = out.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if buf.last() != Some(&b'\n') {
            tail_without_newline = true;
        }
        match serde_json::from_slice::<Value>(&buf) {
            Ok(v) if v["type"] != "mystery_frame" => ok += 1,
            _ => bad += 1,
        }
    }
    let status = child.wait()?;
    Ok((
        t.elapsed().as_secs_f64() * 1000.0,
        ok,
        bad,
        status.code(),
        tail_without_newline,
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    //! /proc probes: RSS ≈ Working Set, RssAnon ≈ Private Bytes, utime+stime = CPU.
    use std::fs;

    pub fn tree(root: u32) -> Vec<u32> {
        let mut all: Vec<(u32, u32)> = Vec::new();
        for e in fs::read_dir("/proc").into_iter().flatten().flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
                let after = stat.rsplit_once(')').map(|x| x.1).unwrap_or("");
                let ppid = after
                    .split_whitespace()
                    .nth(1)
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(0);
                all.push((pid, ppid));
            }
        }
        let mut out = vec![root];
        let mut i = 0;
        while i < out.len() {
            let p = out[i];
            out.extend(all.iter().filter(|(_, pp)| *pp == p).map(|(c, _)| *c));
            i += 1;
        }
        out
    }

    fn status_kb(pid: u32, key: &str) -> f64 {
        fs::read_to_string(format!("/proc/{pid}/status"))
            .ok()
            .and_then(|s| {
                s.lines().find_map(|l| {
                    l.strip_prefix(key)
                        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())
                })
            })
            .unwrap_or(0.0)
    }

    /// (rss_mb, anon_mb) summed over the tree.
    pub fn memory(pids: &[u32]) -> (f64, f64) {
        let rss: f64 = pids.iter().map(|p| status_kb(*p, "VmRSS:")).sum();
        let anon: f64 = pids.iter().map(|p| status_kb(*p, "RssAnon:")).sum();
        (rss / 1024.0, anon / 1024.0)
    }

    /// CPU seconds (user + system) summed over the tree.
    pub fn cpu_seconds(pids: &[u32]) -> f64 {
        // USER_HZ is 100 on every Linux this runs on (glibc/musl x86_64, arm64).
        pids.iter()
            .filter_map(|p| fs::read_to_string(format!("/proc/{p}/stat")).ok())
            .map(|s| {
                let after = s
                    .rsplit_once(')')
                    .map(|x| x.1.to_string())
                    .unwrap_or_default();
                let f: Vec<f64> = after
                    .split_whitespace()
                    .map(|x| x.parse().unwrap_or(0.0))
                    .collect();
                (f.get(11).copied().unwrap_or(0.0) + f.get(12).copied().unwrap_or(0.0)) / 100.0
            })
            .sum()
    }
}

/// Start + idle probes of a real binary. Returns metrics (possibly skipped).
fn probe_exe(exe: &Path, runs: usize, idle_s: u64) -> Vec<Metric> {
    let mut out = Vec::new();
    // Start: `--bench-first-frame` prints {"first_frame_ms": ..} and exits.
    let mut samples = Vec::new();
    let mut why = String::new();
    for _ in 0..runs + 1 {
        let t = Instant::now();
        let child = Command::new(exe)
            .arg("--bench-first-frame")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                why = format!("spawn failed: {e}");
                break;
            }
        };
        let mut text = String::new();
        if let Some(mut so) = child.stdout.take() {
            let _ = so.read_to_string(&mut text);
        }
        let wall = t.elapsed().as_secs_f64() * 1000.0;
        let _ = child.wait();
        let found = text
            .lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .any(|v| v["first_frame_ms"].is_number());
        if !found {
            why = "the binary printed no {\"first_frame_ms\":…} line (desk-ui has no window yet)"
                .into();
            break;
        }
        samples.push(wall);
    }
    if samples.len() > 1 {
        samples.remove(0); // the first launch is the warm-up
        out.push(Metric::measured(
            "warm_start_ms",
            samples,
            "Linux: spawn → first_frame_ms line on stdout, wall clock, page cache warm",
        ));
    } else {
        out.push(Metric::skipped("warm_start_ms", &why));
    }
    out.push(Metric::skipped(
        "cold_start_ms",
        "not measured on Linux: dropping the page cache needs root on the host, and only the Windows number counts",
    ));

    #[cfg(target_os = "linux")]
    {
        let child = Command::new(exe)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(mut c) => {
                std::thread::sleep(Duration::from_secs(2));
                if let Ok(Some(st)) = c.try_wait() {
                    let why = format!(
                        "exits on its own within 2 s ({st}); a stub UI has nothing to idle"
                    );
                    for id in ["idle_working_set_mb", "idle_private_mb", "idle_cpu_pct"] {
                        out.push(Metric::skipped(id, &why));
                    }
                } else {
                    let pids = linux::tree(c.id());
                    let cpu0 = linux::cpu_seconds(&pids);
                    let t0 = Instant::now();
                    std::thread::sleep(Duration::from_secs(idle_s));
                    let pids = linux::tree(c.id());
                    let cpu = linux::cpu_seconds(&pids) - cpu0;
                    let wall = t0.elapsed().as_secs_f64();
                    let (rss, anon) = linux::memory(&pids);
                    let ncpu = std::thread::available_parallelism()
                        .map(|n| n.get())
                        .unwrap_or(1) as f64;
                    out.push(Metric::measured(
                        "idle_working_set_mb",
                        vec![rss],
                        "Linux: VmRSS of the process tree",
                    ));
                    out.push(Metric::measured(
                        "idle_private_mb",
                        vec![anon],
                        "Linux: RssAnon of the process tree",
                    ));
                    out.push(Metric::measured(
                        "idle_cpu_pct",
                        vec![cpu / wall / ncpu * 100.0],
                        "Linux: utime+stime of the tree / wall / logical CPUs",
                    ));
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
            Err(e) => {
                for id in ["idle_working_set_mb", "idle_private_mb", "idle_cpu_pct"] {
                    out.push(Metric::skipped(id, &format!("spawn failed: {e}")));
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = idle_s;
        for id in ["idle_working_set_mb", "idle_private_mb", "idle_cpu_pct"] {
            out.push(Metric::skipped(id, "use scripts/rust/bench.ps1 on Windows"));
        }
    }
    out
}

fn main() -> Result<()> {
    let mut a = Args::from_env();
    let quick = a.flag("--quick");
    let representative = a.flag("--representative");
    let runs: usize = a.parsed("--runs")?.unwrap_or(if quick { 5 } else { 30 });
    let idle_s: u64 = a.parsed("--idle-seconds")?.unwrap_or(60);
    let label = a
        .value("--label")?
        .unwrap_or_else(|| "desk-bench perf-sample".into());
    let exe: Option<PathBuf> = a.value("--exe")?.map(PathBuf::from);
    let installer: Option<PathBuf> = a.value("--installer")?.map(PathBuf::from);
    let commit = match a.value("--commit")? {
        Some(c) => c,
        None => Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".into()),
    };
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let out: PathBuf = a
        .value("--out")?
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{date}-{commit}-{}.json", std::env::consts::OS)));
    a.finish()?;

    let work = std::env::temp_dir().join(format!("desk-perf-sample-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let gen = |name: &str, scale: f64, real_size: bool| -> Result<PathBuf> {
        let p = work.join(name);
        generate(
            &p,
            &GenOptions {
                days: days_for_scale(scale),
                real_size,
                png: None,
                ..GenOptions::default()
            },
        )?;
        Ok(p)
    };
    eprintln!("perf-sample: generating vaults in {}", work.display());
    let x1 = gen("x1", 1.0, false)?;
    let fat = gen("x1-real", 1.0, true)?;
    let x10 = gen("x10", if quick { 3.0 } else { 10.0 }, true)?;
    let today = "2026-11-25";

    let mut metrics = Vec::new();
    let hook = if cfg!(feature = "core-day") {
        "desk_core::day"
    } else {
        "serde_json::Value lower bound"
    };
    let files = day::read_day(&x1, today).len();
    let s = repeat_ms(3, runs, || {
        let _ = std::hint::black_box(day::parse_day(&day::read_day(&x1, today)));
    });
    metrics.push(Metric::measured(
        "day_load_ms",
        s,
        &format!("read {files} files of one day from a warm page cache + {hook}; gen-vault ×1, demo-size files"),
    ));
    let s = repeat_ms(3, runs, || {
        let _ = std::hint::black_box(day::parse_day(&day::read_day(&fat, today)));
    });
    metrics.push(Metric::measured(
        "day_load_real_size_ms",
        s,
        &format!("same, macro_*.json padded to ~2000 lines (--real-size); {hook}"),
    ));
    let days = day::list_days(&x10);
    let s = repeat_ms(1, if quick { 2 } else { 5 }, || {
        for d in &days {
            let _ = std::hint::black_box(day::parse_day(&day::read_day(&x10, d)));
        }
    });
    metrics.push(Metric::measured(
        "vault_x10_parse_ms",
        s,
        &format!(
            "every one of {} days of gen-vault ×10 --real-size, read + {hook}",
            days.len()
        ),
    ));
    let s = repeat_ms(2, runs, || {
        let _ = std::hint::black_box(day::list_days(&x10));
    });
    metrics.push(Metric::measured(
        "vault_x10_list_days_ms",
        s,
        "readdir of state/ + date extraction (listDays)",
    ));

    let lines = turn(&StreamSpec::default(), 0, today);
    let s = repeat_ms(2, runs.min(20), || {
        let _ = std::hint::black_box(count_frames(&lines));
    });
    metrics.push(Metric::measured(
        "stream_parse_10k_ms",
        s,
        "10 000 stream-json lines → serde_json::Value each (HOOK(desk-app): the driver's parser)",
    ));

    let mut stress = Vec::new();
    match sibling("fake-claude") {
        Ok(fake) if fake.exists() => {
            let mut s = Vec::new();
            let mut counts_ok = true;
            for _ in 0..if quick { 3 } else { 10 } {
                let (ms, ok, bad, code, _) = pipe_fake_claude(&fake, &["--events", "10000"])?;
                counts_ok &= ok == 10_001 && bad == 0 && code == Some(0);
                s.push(ms);
            }
            metrics.push(Metric::measured(
                "stream_pipe_10k_ms",
                s,
                "spawn fake-claude --events 10000 → read every line through a pipe → serde_json (process + pipe floor)",
            ));
            stress.push(check(
                "fake_claude_10k_complete",
                counts_ok,
                "10 000 events + init arrive, exit 0".into(),
            ));
            let (_, ok, bad, code, _) =
                pipe_fake_claude(&fake, &["--events", "10000", "--garbage-every", "7"])?;
            stress.push(check(
                "fake_claude_garbage",
                ok == 10_001 && bad == 10_001 / 7 && code == Some(0),
                format!("garbage every 7th line: {ok} frames ok, {bad} broken lines skipped"),
            ));
            let (ms, ok, _, code, tail) =
                pipe_fake_claude(&fake, &["--events", "10000", "--die-after", "5000"])?;
            stress.push(check(
                "fake_claude_dies_mid_line",
                ok == 5000 && code == Some(3) && tail && ms < 1000.0,
                format!("{ok} frames, then half a line and exit {code:?} after {ms:.0} ms"),
            ));
            let (ms, ok, _, _, _) =
                pipe_fake_claude(&fake, &["--events", "100", "--stall-ms", "1500"])?;
            stress.push(check(
                "fake_claude_stall",
                ok == 101 && ms >= 1500.0,
                format!("1.5 s stall mid-turn, all {ok} frames after {ms:.0} ms"),
            ));
        }
        _ => {
            metrics.push(Metric::skipped(
                "stream_pipe_10k_ms",
                "fake-claude not built next to perf-sample (cargo build -p desk-bench --release)",
            ));
        }
    }
    match sibling("slow-writer") {
        Ok(sw) if sw.exists() => {
            // Read the target as fast as possible while slow-writer writes it
            // in chunks: how many reads saw broken JSON, and none may crash.
            let target = work.join("watch").join("scale_2026-11-25_eu.json");
            std::fs::create_dir_all(target.parent().context("no parent")?)?;
            let mut child = Command::new(&sw)
                .arg(&target)
                .args([
                    "--mode",
                    "chunks",
                    "--size",
                    "65536",
                    "--chunk",
                    "2048",
                    "--delay-ms",
                    "5",
                ])
                .stdout(Stdio::null())
                .spawn()?;
            let (mut reads, mut broken) = (0usize, 0usize);
            let t = Instant::now();
            while child.try_wait()?.is_none() && t.elapsed() < Duration::from_secs(10) {
                if let Ok(b) = std::fs::read(&target) {
                    reads += 1;
                    if serde_json::from_slice::<Value>(&b).is_err() {
                        broken += 1;
                    }
                }
            }
            let _ = child.wait();
            let final_ok = std::fs::read(&target)
                .ok()
                .map(|b| serde_json::from_slice::<Value>(&b).is_ok())
                .unwrap_or(false);
            stress.push(check(
                "slow_writer_partial_reads",
                broken > 0 && final_ok,
                format!("{reads} reads during a chunked write, {broken} saw broken JSON, final file valid: {final_ok}"),
            ));
        }
        _ => stress.push(check(
            "slow_writer_partial_reads",
            false,
            "slow-writer not built".into(),
        )),
    }

    metrics.push(Metric::skipped(
        "watch_latency_ms",
        "needs desk-app's watcher (HOOK(desk-app)): slow-writer's `done` t_unix_ms → the app's snapshot line, see PERF.md",
    ));
    metrics.push(Metric::skipped(
        "switch_ms",
        "needs a desk-ui bench hook (proposed `--bench-switch`, PERF.md)",
    ));
    for id in ["frame_p50_ms", "frame_p95_ms", "frame_p99_ms"] {
        metrics.push(Metric::skipped(id, "needs the window: DESK_FRAME_STATS=1 while fake-claude streams 10k events (bench.ps1 -StreamFrames)"));
    }

    let mut build = Build {
        kind: "synthetic".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        commit: commit.clone(),
        exe: String::new(),
        profile: if cfg!(debug_assertions) {
            "debug".into()
        } else {
            "release".into()
        },
    };
    if let Some(exe) = &exe {
        build.kind = "rust".into();
        build.exe = exe
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match std::fs::metadata(exe) {
            Ok(m) => metrics.push(Metric::measured(
                "exe_mb",
                vec![m.len() as f64 / 1_048_576.0],
                "file size of the exe",
            )),
            Err(e) => metrics.push(Metric::skipped(
                "exe_mb",
                &format!("{}: {e}", exe.display()),
            )),
        }
        let probed = probe_exe(
            exe,
            if quick { 3 } else { 10 },
            if quick { idle_s.min(5) } else { idle_s },
        );
        let stub = probed
            .iter()
            .any(|m| m.id == "warm_start_ms" && m.status == Status::Skip);
        if stub {
            // A binary with no window yet is trivially small; judging its size
            // against the 15 MB target would be a pass that means nothing.
            for m in metrics.iter_mut().filter(|m| m.id == "exe_mb") {
                *m = m
                    .clone()
                    .info("stub binary without a window: size not comparable with the target yet");
            }
        }
        metrics.extend(probed);
    } else {
        metrics.push(Metric::skipped("exe_mb", "no --exe given"));
    }
    match &installer {
        Some(p) => match std::fs::metadata(p) {
            Ok(m) => metrics.push(Metric::measured(
                "installer_mb",
                vec![m.len() as f64 / 1_048_576.0],
                "file size",
            )),
            Err(e) => metrics.push(Metric::skipped("installer_mb", &format!("{e}"))),
        },
        None => metrics.push(Metric::skipped(
            "installer_mb",
            "no installer yet (stage 7)",
        )),
    }
    metrics.push(Metric::skipped(
        "soak_private_growth_pct",
        "24 h soak runs on Windows only (bench.ps1 -SoakHours)",
    ));

    let mut machine = desk_bench::machine::detect();
    if !representative {
        machine.noise.push(
            "not the owner's laptop: numbers show the harness works, not how fast Desk is".into(),
        );
    }
    let report = Report {
        schema: SCHEMA.into(),
        representative,
        label,
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        date,
        machine,
        build,
        harness: Harness {
            script: format!(
                "desk-bench perf-sample{}",
                if quick { " --quick" } else { "" }
            ),
            runs,
            cold_method: String::new(),
            idle_seconds: idle_s,
            settle_seconds: 0,
        },
        metrics,
        notes: vec![format!("day parser: {hook}")],
    };
    let mut v = serde_json::to_value(&report)?;
    v["stress"] = Value::Array(stress);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&out, serde_json::to_string_pretty(&v)? + "\n")?;
    let _ = std::fs::remove_dir_all(&work);

    for m in &report.metrics {
        let st = match m.status {
            Status::Pass => "pass",
            Status::Fail => "FAIL",
            Status::Skip => "skip",
            Status::Info => "info",
        };
        let val = m
            .value
            .map(|x| format!("{x:.3}"))
            .unwrap_or_else(|| "—".into());
        eprintln!("{:<26} {:>12} {:<5} {st}", m.id, val, m.unit);
    }
    for s in v["stress"].as_array().into_iter().flatten() {
        eprintln!(
            "stress {:<26} {} — {}",
            s["id"].as_str().unwrap_or(""),
            s["status"].as_str().unwrap_or(""),
            s["detail"].as_str().unwrap_or("")
        );
    }
    eprintln!("wrote {}", out.display());
    Ok(())
}
