//! Writes a state file the slow and awkward ways the skill (Python) and
//! PowerShell really do, so the watcher can be tested against them.
//!
//!   slow-writer <target.json> [--mode chunks|truncate|rename]
//!               [--from FILE | --size BYTES] [--chunk BYTES] [--delay-ms MS]
//!               [--cut FRACTION] [--then-complete-ms MS]
//!               [--repeat K] [--interval-ms MS] [--bom] [--verbose]
//!
//! Modes:
//! - `chunks` (default): truncate the target and write it in `--chunk`-byte
//!   pieces with `--delay-ms` between them — a reader in between sees a
//!   prefix of valid JSON.
//! - `truncate`: write only the first `--cut` (0.5) of it and stop — a file
//!   that stays broken. `--then-complete-ms MS` rewrites it whole after MS.
//! - `rename`: write `<target>.tmp-<pid>` in chunks, then rename over the
//!   target — the atomic way (what the desk itself does). On Windows the
//!   rename fails while a reader holds the target without FILE_SHARE_DELETE;
//!   it is retried 50 × 10 ms and every failed try is reported.
//!
//! `--repeat K --interval-ms MS` does it K times (a burst, for the 300 ms
//! debounce). Content: `--from` a file, else valid JSON of `--size` bytes
//! (64 KiB). `--bom` prefixes a UTF-8 BOM, as PowerShell 5.1 writes.
//!
//! Stdout: one JSON line per event, `t_unix_ms` with sub-ms precision.
//! `{"event":"done",…}` is the moment the watch-latency clock starts.

use anyhow::{bail, Context, Result};
use desk_bench::cli::Args;
use serde_json::json;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn emit(v: serde_json::Value) {
    println!("{v}");
}

fn synthetic(size: usize, seq: usize) -> Vec<u8> {
    let head = format!("{{\n \"desk_bench\": \"slow-writer\",\n \"seq\": {seq},\n \"pad\": [\n");
    let tail = "  0\n ]\n}\n";
    let mut s = head;
    let mut i = 0usize;
    while s.len() + tail.len() + 16 < size {
        s.push_str(&format!("  {i},\n"));
        i += 1;
    }
    s.push_str(tail);
    s.into_bytes()
}

struct Job {
    target: PathBuf,
    mode: String,
    chunk: usize,
    delay: Duration,
    cut: f64,
    then_complete: Option<Duration>,
    verbose: bool,
}

impl Job {
    fn write_chunks(&self, f: &mut File, data: &[u8], seq: usize) -> Result<()> {
        let n = data.len().div_ceil(self.chunk.max(1));
        for (i, c) in data.chunks(self.chunk.max(1)).enumerate() {
            f.write_all(c)?;
            f.flush()?;
            if self.verbose {
                emit(
                    json!({"event": "chunk", "seq": seq, "i": i, "bytes": c.len(), "t_unix_ms": now_ms()}),
                );
            }
            if i + 1 < n && !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
        }
        Ok(())
    }

    fn open_truncate(p: &Path) -> Result<File> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(p)
            .with_context(|| format!("open {}", p.display()))
    }

    fn run(&self, data: &[u8], seq: usize) -> Result<()> {
        emit(
            json!({"event": "start", "seq": seq, "mode": self.mode, "bytes": data.len(), "t_unix_ms": now_ms()}),
        );
        match self.mode.as_str() {
            "chunks" => {
                let mut f = Self::open_truncate(&self.target)?;
                self.write_chunks(&mut f, data, seq)?;
                f.sync_all()?;
            }
            "truncate" => {
                let cut = ((data.len() as f64) * self.cut.clamp(0.0, 1.0)) as usize;
                let mut f = Self::open_truncate(&self.target)?;
                self.write_chunks(&mut f, &data[..cut], seq)?;
                f.sync_all()?;
                drop(f);
                emit(
                    json!({"event": "truncated", "seq": seq, "bytes": cut, "t_unix_ms": now_ms()}),
                );
                match self.then_complete {
                    Some(d) => {
                        std::thread::sleep(d);
                        let mut f = Self::open_truncate(&self.target)?;
                        f.write_all(data)?;
                        f.sync_all()?;
                    }
                    None => return Ok(()),
                }
            }
            "rename" => {
                let name = self
                    .target
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let tmp = self
                    .target
                    .with_file_name(format!("{name}.tmp-{}", std::process::id()));
                let mut f = Self::open_truncate(&tmp)?;
                self.write_chunks(&mut f, data, seq)?;
                f.sync_all()?;
                drop(f);
                let mut tries = 0;
                loop {
                    match fs::rename(&tmp, &self.target) {
                        Ok(()) => break,
                        Err(e) if tries < 50 => {
                            tries += 1;
                            emit(
                                json!({"event": "rename_retry", "seq": seq, "try": tries, "error": e.to_string(), "t_unix_ms": now_ms()}),
                            );
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(e) => {
                            let _ = fs::remove_file(&tmp);
                            emit(
                                json!({"event": "error", "seq": seq, "error": e.to_string(), "t_unix_ms": now_ms()}),
                            );
                            bail!("rename over {} failed: {e}", self.target.display());
                        }
                    }
                }
            }
            other => bail!("unknown --mode {other} (chunks|truncate|rename)"),
        }
        emit(
            json!({"event": "done", "seq": seq, "mode": self.mode, "bytes": data.len(), "t_unix_ms": now_ms(),
            "path": self.target.display().to_string()}),
        );
        Ok(())
    }
}

fn main() -> Result<()> {
    let mut a = Args::from_env();
    let mode = a.value("--mode")?.unwrap_or_else(|| "chunks".into());
    let from: Option<PathBuf> = a.value("--from")?.map(PathBuf::from);
    let size: usize = a.parsed("--size")?.unwrap_or(64 * 1024);
    let chunk: usize = a.parsed("--chunk")?.unwrap_or(4096);
    let delay: u64 = a.parsed("--delay-ms")?.unwrap_or(20);
    let cut: f64 = a.parsed("--cut")?.unwrap_or(0.5);
    let then_complete: Option<u64> = a.parsed("--then-complete-ms")?;
    let repeat: usize = a.parsed("--repeat")?.unwrap_or(1);
    let interval: u64 = a.parsed("--interval-ms")?.unwrap_or(0);
    let bom = a.flag("--bom");
    let verbose = a.flag("--verbose");
    let target = PathBuf::from(
        a.positional()
            .context("usage: slow-writer <target.json> [options]")?,
    );
    a.finish()?;

    let job = Job {
        target,
        mode,
        chunk,
        delay: Duration::from_millis(delay),
        cut,
        then_complete: then_complete.map(Duration::from_millis),
        verbose,
    };
    for seq in 0..repeat.max(1) {
        let mut data = match &from {
            Some(p) => fs::read(p).with_context(|| format!("read {}", p.display()))?,
            None => synthetic(size, seq),
        };
        if bom {
            data.splice(0..0, [0xEF, 0xBB, 0xBF]);
        }
        job.run(&data, seq)?;
        if seq + 1 < repeat && interval > 0 {
            std::thread::sleep(Duration::from_millis(interval));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_is_valid_json_of_about_the_size() {
        for size in [10, 1000, 65_536] {
            let d = synthetic(size, 3);
            let v: serde_json::Value = serde_json::from_slice(&d).unwrap_or_default();
            assert_eq!(v["seq"], 3);
            assert!(d.len() <= size.max(80), "{} > {size}", d.len());
        }
    }
}
