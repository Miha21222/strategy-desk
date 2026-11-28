//! Synthetic vault for benches and stress tests.
//!
//!   gen-vault <out_dir> [--scale N | --days D] [--seed S] [--end YYYY-MM-DD]
//!             [--real-size] [--png WxH|none] [--force]
//!
//! ×1 ≈ the demo vault (10 days), ×10 ≈ two years of weekdays (501 days).
//! Same seed + options = byte-identical output. Prints a JSON summary.
//! Refuses to write into a non-empty folder that it did not generate, so a
//! slip of the path can never touch a real vault.

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use desk_bench::cli::Args;
use desk_bench::vault_gen::{days_for_scale, generate, GenOptions};
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut a = Args::from_env();
    if a.flag("--help") || a.flag("-h") {
        println!(
            "{}",
            include_str!("gen-vault.rs")
                .lines()
                .skip(2)
                .take(8)
                .map(|l| l.trim_start_matches("//!"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        return Ok(());
    }
    let mut o = GenOptions::default();
    let scale: Option<f64> = a.parsed("--scale")?;
    let days: Option<usize> = a.parsed("--days")?;
    o.days = match (scale, days) {
        (Some(_), Some(_)) => bail!("--scale and --days are exclusive"),
        (Some(s), None) => days_for_scale(s),
        (None, Some(d)) => d,
        (None, None) => days_for_scale(1.0),
    };
    if let Some(s) = a.parsed("--seed")? {
        o.seed = s;
    }
    if let Some(e) = a.value("--end")? {
        o.end = NaiveDate::parse_from_str(&e, "%Y-%m-%d").with_context(|| format!("--end {e}"))?;
    }
    o.real_size = a.flag("--real-size");
    if let Some(p) = a.value("--png")? {
        o.png = if p == "none" {
            None
        } else {
            let (w, h) = p.split_once('x').context("--png WxH")?;
            Some((w.parse()?, h.parse()?))
        };
    }
    let force = a.flag("--force");
    let out = PathBuf::from(
        a.positional()
            .context("usage: gen-vault <out_dir> [options]; --help")?,
    );
    a.finish()?;

    if out.exists() {
        let ours = out.join(".desk-bench.json").exists();
        let empty = std::fs::read_dir(&out)?.next().is_none();
        if !empty && !ours {
            bail!(
                "{} is not empty and was not made by gen-vault; refusing",
                out.display()
            );
        }
        if !empty && !force {
            bail!(
                "{} already holds a generated vault; pass --force to replace it",
                out.display()
            );
        }
        if !empty {
            std::fs::remove_dir_all(&out)?;
        }
    }
    let t = std::time::Instant::now();
    let s = generate(&out, &o)?;
    let mut v = serde_json::to_value(&s)?;
    v["ms"] = serde_json::json!(t.elapsed().as_millis() as u64);
    v["out"] = serde_json::json!(out.display().to_string());
    println!("{v}");
    Ok(())
}
