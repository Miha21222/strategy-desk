//! Day load: ~22 files of one day, and the whole ×10 vault.
//!
//!   cargo bench -p desk-bench --bench day_load
//!
//! Until desk-core lands this times `desk_bench::day::parse_day`, i.e. the
//! serde_json lower bound (see src/day.rs). With `--features core-day` the
//! same groups time `desk_core::day` — HOOK(desk-core). Read times include
//! the OS file cache (warm): that is the case on screen switches, and the
//! cold case is covered by bench.ps1's cold start.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use desk_bench::day::{self, DayInput};
use desk_bench::vault_gen::{days_for_scale, generate, GenOptions};
use std::hint::black_box;
use std::path::PathBuf;

fn vault(name: &str, scale: f64, real_size: bool) -> PathBuf {
    // Regenerated every run: a cached vault from an older generator would
    // silently measure other input.
    let p = std::env::temp_dir().join(format!(
        "desk-bench-criterion-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&p);
    let o = GenOptions {
        days: days_for_scale(scale),
        real_size,
        png: None,
        ..GenOptions::default()
    };
    if let Err(e) = generate(&p, &o) {
        panic!("gen-vault into {}: {e:#}", p.display()); // bench setup: nothing to measure without it
    }
    p
}

fn benches(c: &mut Criterion) {
    let x1 = vault("x1", 1.0, false);
    let fat = vault("x1-real-size", 1.0, true);
    let x10 = vault("x10", 10.0, true);
    let today = "2026-11-25";

    let mut g = c.benchmark_group("day");
    for (name, v) in [("demo_size", &x1), ("real_size", &fat)] {
        let input = day::read_day(v, today);
        g.bench_function(format!("parse_only/{name}"), |b| {
            b.iter_batched(
                || input.clone(),
                |i: DayInput| black_box(day::parse_day(&i)),
                BatchSize::SmallInput,
            )
        });
        g.bench_function(format!("read_and_parse/{name}"), |b| {
            b.iter(|| black_box(day::parse_day(&day::read_day(v, today))))
        });
    }
    g.finish();

    let mut g = c.benchmark_group("vault_x10");
    g.sample_size(10);
    g.bench_function("list_days", |b| b.iter(|| black_box(day::list_days(&x10))));
    let days = day::list_days(&x10);
    g.bench_function("read_and_parse_all_days", |b| {
        b.iter(|| {
            let mut n = 0usize;
            for d in &days {
                n += day::parse_day(&day::read_day(&x10, d)).unwrap_or(0);
            }
            black_box(n)
        })
    });
    g.finish();
    for p in [x1, fat, x10] {
        let _ = std::fs::remove_dir_all(p);
    }
}

criterion_group!(day_load, benches);
criterion_main!(day_load);
