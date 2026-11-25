//! Checks the golden test harness itself and the fixtures it reads, without
//! desk-core's API — so it runs (and keeps the fixtures honest) before
//! desk-core lands. The golden comparisons are in golden_day.rs /
//! golden_helpers.rs behind `--features golden`.

mod common;

use common::*;
use serde_json::json;

#[test]
fn diff_reads_like_the_object() {
    let e = json!({"a": 1, "b": {"c": [1, 2]}, "n": null});
    let a = json!({"a": 1.0, "b": {"c": [1, 3]}, "x": true});
    let d = json_diff(&e, &a, Cmp::default());
    assert_eq!(
        d,
        vec![
            "$.b.c[1]: expected 2, got 3",
            "$.n: missing in Rust (TS has null)",
            "$.x: extra in Rust (true) — TS omits it",
        ]
    );
    let lax = Cmp {
        absent_equals_null: true,
        ..Cmp::default()
    };
    assert!(json_diff(&json!({"n": null}), &json!({}), lax).is_empty());
    assert!(json_diff(&json!([0.1 + 0.2]), &json!([0.3]), Cmp::default()).is_empty());
    assert_eq!(
        json_diff(&json!([1]), &json!([1, 2]), Cmp::default()),
        vec!["$: expected 1 items, got 2"]
    );
}

#[test]
fn kebab_matches_the_ts_spelling() {
    #[derive(Debug)]
    enum Z {
        StrongShort,
        Neutral,
    }
    assert_eq!(kebab(&Z::StrongShort), "strong-short");
    assert_eq!(kebab(&Z::Neutral), "neutral");
}

#[test]
fn golden_fixtures_are_present_and_consistent() {
    let g = golden();
    let index = read_json(&g.join("expected/index.json"));
    let days: Vec<String> = index["days"]
        .as_array()
        .expect("days")
        .iter()
        .map(|d| str_of(d).to_owned())
        .collect();
    assert!(
        days.len() >= 9,
        "the demo vault has at least nine days, got {days:?}"
    );
    let files: Vec<String> = expected_days(&g.join("expected"))
        .into_iter()
        .map(|(d, _)| d)
        .collect();
    let mut sorted = days.clone();
    sorted.sort();
    assert_eq!(files, sorted, "one day_<D>.json per listed day");
    let vault = read_vault(&g.join("vault"));
    assert!(
        vault
            .keys()
            .any(|k| k.starts_with("indices-kb/state/scale_")),
        "the vault has scales"
    );
    for (k, v) in &vault {
        if k.ends_with(".png") {
            assert!(v.len() <= 200 * 1024, "{k} is over 200 KB");
            assert!(v.starts_with(b"\x89PNG\r\n\x1a\n"), "{k} is a real PNG");
        }
    }
}

#[test]
fn golden_json_carries_no_base64_and_no_machine_paths() {
    fn walk(v: &serde_json::Value, at: &str, bad: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => {
                if s.starts_with("data:image") || s.contains("/home/") || s.contains(":\\Users") {
                    bad.push(format!("{at}: {}", &s[..s.len().min(60)]));
                }
            }
            serde_json::Value::Array(a) => a
                .iter()
                .enumerate()
                .for_each(|(i, x)| walk(x, &format!("{at}[{i}]"), bad)),
            serde_json::Value::Object(o) => o
                .iter()
                .for_each(|(k, x)| walk(x, &format!("{at}.{k}"), bad)),
            _ => {}
        }
    }
    let mut bad = Vec::new();
    let mut n = 0;
    for dir in [golden().join("expected"), golden().join("expected/helpers")] {
        for e in std::fs::read_dir(&dir).expect("expected dir").flatten() {
            if e.path().extension().is_some_and(|x| x == "json") {
                n += 1;
                walk(
                    &read_json(&e.path()),
                    &e.file_name().to_string_lossy(),
                    &mut bad,
                );
            }
        }
    }
    assert!(n > 15, "golden json present");
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn every_edge_case_has_its_expected_outcome() {
    let root = fixtures().join("edge");
    let mut n = 0;
    for e in std::fs::read_dir(&root).expect("edge dir").flatten() {
        let case = e.path();
        if !case.join("case.json").exists() {
            continue;
        }
        n += 1;
        let meta = read_json(&case.join("case.json"));
        let compare = str_of(&meta["compare"]);
        assert!(
            ["strict", "no-crash"].contains(&compare),
            "{}: compare = {compare}",
            case.display()
        );
        for d in meta["dates"].as_array().expect("dates") {
            let f = case.join(format!("expected/day_{}.json", str_of(d)));
            assert!(f.exists(), "{} missing", f.display());
        }
        assert!(case.join("expected/index.json").exists());
    }
    assert!(n >= 8, "edge cases present: {n}");
}
