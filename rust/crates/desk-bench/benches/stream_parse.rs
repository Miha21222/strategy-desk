//! 10 000 stream-json lines parsed with serde_json: the floor for the
//! Claude driver's line parser.
//!
//!   cargo bench -p desk-bench --bench stream_parse
//!
//! HOOK(desk-app): when `desk_app::claude` exposes its line → `DriverEvent`
//! translation, add a second function here that feeds it the same lines.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use desk_bench::stream::{count_frames, turn, StreamSpec};
use std::hint::black_box;

fn benches(c: &mut Criterion) {
    let lines = turn(&StreamSpec::default(), 0, "2026-11-25");
    let bytes: usize = lines.iter().map(|l| l.len() + 1).sum();
    let mut g = c.benchmark_group("stream");
    g.throughput(Throughput::Bytes(bytes as u64));
    g.bench_function("serde_value_10k", |b| {
        b.iter(|| black_box(count_frames(&lines)))
    });
    g.finish();
}

criterion_group!(stream_parse, benches);
criterion_main!(stream_parse);
