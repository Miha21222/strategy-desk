//! Shared helpers of the golden tests: where the fixtures live, how a vault
//! folder becomes the `DayInput` map, and a structural JSON comparison that
//! prints a readable diff (`$.eu.scale.result.needle: expected -58, got -57`).
//!
//! Lives in `tests/common/mod.rs` (not `tests/common.rs`) so cargo does not
//! build it as a test binary of its own.
#![allow(dead_code)] // each golden file uses a different subset

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// `rust/fixtures`.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// `rust/fixtures/golden`.
pub fn golden() -> PathBuf {
    fixtures().join("golden")
}

pub fn read_json(path: &Path) -> Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Every file under `vault`, keyed by its vault-relative path with `/`
/// separators (`indices-kb/state/scale_2026-09-14_eu.json`,
/// `Макро-обзоры/2026-09-14 Европа.md`). That is the `DayInput` of
/// CONTRACTS.md: desk-core sees names and bytes, never the disk.
pub fn read_vault(vault: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).expect("under root");
                let key = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(key, std::fs::read(&p).expect("readable fixture"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(vault, vault, &mut out);
    out
}

/// The `day_<D>.json` files of an `expected/` folder, by date, sorted.
pub fn expected_days(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let date = name.strip_prefix("day_")?.strip_suffix(".json")?.to_owned();
            Some((date, e.path()))
        })
        .collect();
    out.sort();
    out
}

/// How strict the comparison is. The default is the TEAM rule 4 reading:
/// the Rust JSON must be the TS JSON.
#[derive(Clone, Copy, Debug)]
pub struct Cmp {
    /// TS drops `undefined` fields while Rust may write `null`. Off by
    /// default: the dump JSON is the contract. Flip to see only real diffs.
    pub absent_equals_null: bool,
    /// Relative tolerance for numbers (TS has only f64; 12 and 12.0 are equal).
    pub eps: f64,
}

impl Default for Cmp {
    fn default() -> Self {
        Cmp {
            absent_equals_null: false,
            eps: 1e-9,
        }
    }
}

fn num_eq(a: f64, b: f64, eps: f64) -> bool {
    a == b || (a - b).abs() <= eps * a.abs().max(b.abs()).max(1.0)
}

fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.chars().count() > 120 {
        let cut: String = s.chars().take(117).collect();
        format!("{cut}…")
    } else {
        s
    }
}

/// Every difference between `expected` and `actual`, one line each, with a
/// JSONPath-like location. Object key order is ignored; array order is not.
pub fn json_diff(expected: &Value, actual: &Value, cmp: Cmp) -> Vec<String> {
    let mut out = Vec::new();
    diff_at("$", expected, actual, cmp, &mut out);
    out
}

fn diff_at(path: &str, e: &Value, a: &Value, cmp: Cmp, out: &mut Vec<String>) {
    match (e, a) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (
                x.as_f64().unwrap_or(f64::NAN),
                y.as_f64().unwrap_or(f64::NAN),
            );
            if !num_eq(x, y, cmp.eps) {
                out.push(format!("{path}: expected {x}, got {y}"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, ev) in x {
                let p = format!("{path}.{k}");
                match y.get(k) {
                    Some(av) => diff_at(&p, ev, av, cmp, out),
                    None if cmp.absent_equals_null && ev.is_null() => {}
                    None => out.push(format!("{p}: missing in Rust (TS has {})", short(ev))),
                }
            }
            for (k, av) in y {
                if !x.contains_key(k) && !(cmp.absent_equals_null && av.is_null()) {
                    out.push(format!(
                        "{path}.{k}: extra in Rust ({}) — TS omits it",
                        short(av)
                    ));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!(
                    "{path}: expected {} items, got {}",
                    x.len(),
                    y.len()
                ));
            }
            for (i, (ev, av)) in x.iter().zip(y).enumerate() {
                diff_at(&format!("{path}[{i}]"), ev, av, cmp, out);
            }
        }
        _ if e == a => {}
        _ => out.push(format!("{path}: expected {}, got {}", short(e), short(a))),
    }
}

/// Collects failures across many cases and panics once, readably.
#[derive(Default)]
pub struct Report {
    cases: usize,
    failed: Vec<String>,
}

impl Report {
    pub fn case(&mut self) {
        self.cases += 1;
    }

    pub fn fail(&mut self, label: impl Into<String>, lines: Vec<String>) {
        let mut s = label.into();
        for l in lines.iter().take(25) {
            let _ = write!(s, "\n    {l}");
        }
        if lines.len() > 25 {
            let _ = write!(s, "\n    … and {} more", lines.len() - 25);
        }
        self.failed.push(s);
    }

    /// Compare two JSON values as one case.
    pub fn json(&mut self, label: impl Into<String>, expected: &Value, actual: &Value, cmp: Cmp) {
        self.case();
        let d = json_diff(expected, actual, cmp);
        if !d.is_empty() {
            self.fail(label, d);
        }
    }

    /// Compare two displayable values as one case.
    pub fn eq<T: PartialEq + std::fmt::Debug>(
        &mut self,
        label: impl Into<String>,
        expected: T,
        actual: T,
    ) {
        self.case();
        if expected != actual {
            self.fail(
                label,
                vec![format!("expected {expected:?}, got {actual:?}")],
            );
        }
    }

    pub fn finish(self, what: &str) {
        assert!(self.cases > 0, "{what}: no cases ran — are the fixtures there? (bash rust/fixtures/tools/capture-golden.sh)");
        if !self.failed.is_empty() {
            let shown: Vec<&str> = self.failed.iter().take(40).map(String::as_str).collect();
            panic!(
                "{what}: {} of {} cases differ from the old app (golden):\n  {}{}",
                self.failed.len(),
                self.cases,
                shown.join("\n  "),
                if self.failed.len() > 40 {
                    format!("\n  … and {} more", self.failed.len() - 40)
                } else {
                    String::new()
                }
            );
        }
    }
}

/// `StrongShort` → `strong-short`: enum variants compared with the TS strings
/// through `Debug`, so the tests need no particular serde attributes.
pub fn kebab<T: std::fmt::Debug>(v: &T) -> String {
    let s = format!("{v:?}");
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.extend(ch.to_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A golden number that JSON cannot spell (`"NaN"`, `"Infinity"`).
pub fn f64_of(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) if s == "NaN" => f64::NAN,
        Value::String(s) if s == "Infinity" => f64::INFINITY,
        Value::String(s) if s == "-Infinity" => f64::NEG_INFINITY,
        other => panic!("not a number in the golden file: {other}"),
    }
}

pub fn str_of(v: &Value) -> &str {
    v.as_str()
        .unwrap_or_else(|| panic!("not a string in the golden file: {v}"))
}
