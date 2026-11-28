//! CSV statistics: `parseCsv` and `factorStats` of `src/shared/macro.ts`, over
//! `journal/scale_factor_scores.csv`, plus `fmt` for numbers on screen.

use crate::js;
use crate::names::parse_date;
use crate::vault::{FactorStat, Region};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One CSV row: header → cell (a missing trailing cell is `""`).
pub type CsvRow = BTreeMap<String, String>;

/// The hit-rate window of [`factor_stats`], in calendar days ending at the date.
pub const STATS_DAYS: i64 = 30;

/// `parseCsv`: minimal RFC-4180 reader (quotes, commas and newlines inside
/// quotes; a leading BOM dropped; blank lines skipped). The first row is the
/// header; with a repeated header name the last column wins, as in
/// `Object.fromEntries`.
pub fn parse_csv(text: &str) -> Vec<CsvRow> {
    let src = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut it = src.chars().peekable();
    while let Some(c) = it.next() {
        if quoted {
            if c == '"' && it.peek() == Some(&'"') {
                cell.push('"');
                it.next();
            } else if c == '"' {
                quoted = false;
            } else {
                cell.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == ',' {
            row.push(std::mem::take(&mut cell));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            row.push(std::mem::take(&mut cell));
            rows.push(std::mem::take(&mut row));
        } else {
            cell.push(c);
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    let mut rows = rows
        .into_iter()
        .filter(|r| r.len() > 1 || r.first().is_some_and(|c| !c.is_empty()));
    let Some(head) = rows.next() else {
        return Vec::new();
    };
    rows.map(|r| {
        head.iter()
            .enumerate()
            .map(|(i, h)| (h.clone(), r.get(i).cloned().unwrap_or_default()))
            .collect()
    })
    .collect()
}

/// `factorStats`: hit rate per factor over `days` calendar days ending at
/// `date`. Rows without a verdict (`hit` empty: the factor was silent, or the
/// window stayed flat) do not count — the same rule as `scale.py stats`.
/// Most rows first, then by factor id (`localeCompare`).
///
/// A row whose `date` is not an ISO date counts, whatever it says: in the TS
/// the day difference is `NaN` there, and `NaN < 0 || NaN >= days` is false.
pub fn factor_stats_rows(rows: &[CsvRow], date: NaiveDate, days: i64) -> Vec<FactorStat> {
    let today = date.signed_duration_since(epoch()).num_days();
    let mut acc: Vec<FactorStat> = Vec::new();
    let mut index: BTreeMap<(Region, Option<String>), usize> = BTreeMap::new();
    for r in rows {
        let hit = r.get("hit").map(String::as_str);
        if hit != Some("1") && hit != Some("0") {
            continue;
        }
        let Some(region) = r.get("region").and_then(|s| Region::parse(s)) else {
            continue;
        };
        if let Some(day) = r.get("date").and_then(|d| js::iso_day_number(d)) {
            let d = today - day;
            if d < 0 || d >= days {
                continue;
            }
        }
        let factor = r.get("factor").cloned();
        let key = (region, factor.clone());
        let i = *index.entry(key).or_insert_with(|| {
            acc.push(FactorStat {
                region,
                factor,
                hits: 0,
                total: 0,
            });
            acc.len() - 1
        });
        acc[i].total += 1;
        if hit == Some("1") {
            acc[i].hits += 1;
        }
    }
    // Stable, like Array.prototype.sort: insertion order breaks full ties.
    acc.sort_by(|a, b| {
        b.total.cmp(&a.total).then_with(|| {
            js::locale_compare(
                a.factor.as_deref().unwrap_or(""),
                b.factor.as_deref().unwrap_or(""),
            )
        })
    });
    acc
}

/// [`factor_stats_rows`] straight from the CSV text, over [`STATS_DAYS`] — what `loadDay` puts in `stats`.
pub fn factor_stats(csv: &str, date: NaiveDate) -> Vec<FactorStat> {
    factor_stats_rows(&parse_csv(csv), date, STATS_DAYS)
}

fn epoch() -> NaiveDate {
    NaiveDate::default() // 1970-01-01
}

/// The default look-back of [`factor_hit_rate`], in trading days.
pub const HIT_RATE_DAYS: usize = 20;

/// How often one argument (factor) called the window right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HitRate {
    pub hits: u64,
    pub total: u64,
}

impl HitRate {
    /// hits / total (never NaN: a `HitRate` always has `total > 0`).
    pub fn rate(&self) -> f64 {
        self.hits as f64 / self.total as f64
    }
}

/// The calendar date of a row, when it is a real `YYYY-MM-DD`.
fn row_date(r: &CsvRow) -> Option<NaiveDate> {
    r.get("date").and_then(|d| parse_date(d).ok())
}

fn row_is(r: &CsvRow, id: &str, region: Region) -> bool {
    r.get("factor").map(String::as_str) == Some(id)
        && r.get("region").map(String::as_str) == Some(region.as_str())
}

/// Count verdict rows the way `factorStats` does: `hit` is `1` or `0`; an
/// empty `hit` (the factor was silent, or the window stayed flat) is no verdict.
fn count<'a>(rows: impl Iterator<Item = &'a CsvRow>) -> Option<HitRate> {
    let mut h = HitRate { hits: 0, total: 0 };
    for r in rows {
        match r.get("hit").map(String::as_str) {
            Some("1") => {
                h.hits += 1;
                h.total += 1;
            }
            Some("0") => h.total += 1,
            _ => {}
        }
    }
    (h.total > 0).then_some(h)
}

/// The hit rate of factor `id` in `region` over the last `n` trading days
/// strictly before `before` — for the morning screen, where today has no
/// verdict yet. A trading day is a date the CSV has any row for in that
/// region (a day the factor was silent still counts as a day), so «20 days»
/// means the last 20 reviewed days, not 20 verdicts. Rows whose date is not a
/// real ISO date are skipped: they cannot be placed in time (`factorStats`,
/// which only asks «within 30 days», counts them). `None` when the factor
/// gave no verdict in that span.
pub fn factor_hit_rate(
    rows: &[CsvRow],
    id: &str,
    region: Region,
    before: NaiveDate,
    n: usize,
) -> Option<HitRate> {
    let days: BTreeSet<NaiveDate> = rows
        .iter()
        .filter(|r| r.get("region").map(String::as_str) == Some(region.as_str()))
        .filter_map(row_date)
        .filter(|d| *d < before)
        .collect();
    let span: BTreeSet<NaiveDate> = days.into_iter().rev().take(n).collect();
    count(
        rows.iter()
            .filter(|r| row_is(r, id, region) && row_date(r).is_some_and(|d| span.contains(&d))),
    )
}

/// The hit rate of factor `id` in `region` on the given dates only (the
/// similar days of «Похожие дни»). Same counting as [`factor_hit_rate`];
/// `None` when the factor gave no verdict on any of them.
pub fn factor_hit_rate_on(
    rows: &[CsvRow],
    id: &str,
    region: Region,
    dates: &[NaiveDate],
) -> Option<HitRate> {
    let set: BTreeSet<NaiveDate> = dates.iter().copied().collect();
    count(
        rows.iter()
            .filter(|r| row_is(r, id, region) && row_date(r).is_some_and(|d| set.contains(&d))),
    )
}

/// `fmt`: a number for the screen, `—` for a missing one; never panics.
pub fn fmt(x: Option<f64>, digits: usize) -> String {
    match x {
        Some(x) if x.is_finite() => js::to_fixed(x, digits),
        _ => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn row(pairs: &[(&str, &str)]) -> CsvRow {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn parses_quoted_cells() {
        let rows = parse_csv("\u{FEFF}a,b,c\r\n1,\"x, y\",3\n4,\"he said \"\"hi\"\"\",\n");
        assert_eq!(
            rows,
            vec![
                row(&[("a", "1"), ("b", "x, y"), ("c", "3")]),
                row(&[("a", "4"), ("b", "he said \"hi\""), ("c", "")])
            ]
        );
    }

    #[test]
    fn csv_edge_cases() {
        assert!(parse_csv("").is_empty());
        assert!(parse_csv("a,b").is_empty());
        // Blank lines are skipped, a missing cell is empty, a newline inside quotes stays.
        let rows = parse_csv("a,b\n\n1\n\"x\ny\",2");
        assert_eq!(
            rows,
            vec![
                row(&[("a", "1"), ("b", "")]),
                row(&[("a", "x\ny"), ("b", "2")])
            ]
        );
        // An unterminated quote swallows the rest, like the TS.
        assert_eq!(parse_csv("a\n\"open,\nx"), vec![row(&[("a", "open,\nx")])]);
        // A repeated header: the last column wins.
        assert_eq!(parse_csv("a,a\n1,2"), vec![row(&[("a", "2")])]);
    }

    #[test]
    fn counts_the_last_30_days_only_ignoring_silent_rows() {
        let rows = parse_csv(
            &[
                "date,region,factor,group,share,side,strength,dir,hit",
                "2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1",
                "2026-10-08,eu,bias_a,g,12,LONG,1,SHORT,0",
                "2026-10-08,eu,smt,g,2,,0,SHORT,",
                "2026-08-01,eu,bias_a,g,12,SHORT,1,SHORT,1",
                "2026-10-13,eu,bias_a,g,12,SHORT,1,SHORT,1",
                "2026-10-08,us,room,g,12,LONG,1,SHORT,0",
            ]
            .join("\n"),
        );
        let s = factor_stats_rows(&rows, d("2026-10-12"), 30);
        let bias = s
            .iter()
            .find(|x| x.region == Region::Eu && x.factor.as_deref() == Some("bias_a"))
            .unwrap();
        assert_eq!((bias.hits, bias.total), (1, 2));
        assert!(!s.iter().any(|x| x.factor.as_deref() == Some("smt")));
        assert_eq!(s.iter().find(|x| x.region == Region::Us).unwrap().total, 1);
    }

    #[test]
    fn window_edges_and_ordering() {
        let csv = "date,region,factor,hit\n2026-09-13,eu,old,1\n2026-09-14,eu,edge,1\n2026-10-12,us,b,0\n2026-10-12,eu,b,1\n2026-10-12,eu,a,1\nnot-a-date,eu,a,1\n2026-10-12,xx,a,1";
        let s = factor_stats(csv, d("2026-10-12"));
        // 2026-10-12 − 2026-09-13 = 29 days → in; 09-12 would be 30 → out.
        // The two «b» tie on total and id: the one seen first (us) stays first.
        let names: Vec<_> = s
            .iter()
            .map(|x| (x.region.as_str(), x.factor.clone().unwrap(), x.total))
            .collect();
        assert_eq!(
            names,
            [
                ("eu", "a".to_string(), 2),
                ("us", "b".into(), 1),
                ("eu", "b".into(), 1),
                ("eu", "edge".into(), 1),
                ("eu", "old".into(), 1)
            ]
        );
        let out = factor_stats("date,region,factor,hit\n2026-09-12,eu,x,1", d("2026-10-12"));
        assert!(out.is_empty());
    }

    #[test]
    fn a_csv_without_a_factor_column_still_counts() {
        let s = factor_stats("date,region,hit\n2026-10-12,eu,1", d("2026-10-12"));
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::json!([{"region": "eu", "hits": 1, "total": 1}])
        );
    }

    #[test]
    fn fmt_never_throws() {
        assert_eq!(fmt(None, 1), "—");
        assert_eq!(fmt(Some(1.234), 2), "1.23");
        assert_eq!(fmt(Some(f64::NAN), 1), "—");
        assert_eq!(fmt(Some(-0.05), 1), "-0.1");
    }

    const SCORES: &str = "date,region,factor,group,share,side,strength,dir,hit
2026-10-01,eu,bias_a,g,12,SHORT,1,SHORT,1
2026-10-02,eu,bias_a,g,12,LONG,1,SHORT,0
2026-10-02,eu,room,g,12,LONG,1,SHORT,1
2026-10-05,eu,room,g,12,,0,SHORT,
2026-10-06,eu,bias_a,g,12,,0,FLAT,
2026-10-07,eu,bias_a,g,12,SHORT,1,SHORT,1
2026-10-08,eu,bias_a,g,12,SHORT,1,SHORT,1
2026-10-08,us,bias_a,g,12,SHORT,1,LONG,0
nope,eu,bias_a,g,12,SHORT,1,SHORT,0
2026-10-12,eu,bias_a,g,12,SHORT,1,SHORT,0
";

    #[test]
    fn hit_rate_looks_back_n_trading_days_before_the_date() {
        let rows = parse_csv(SCORES);
        let h = |n| factor_hit_rate(&rows, "bias_a", Region::Eu, d("2026-10-12"), n);
        // Before 12.10 (the 12th itself excluded): 01 ✓, 02 ✗, 06 silent, 07 ✓, 08 ✓.
        assert_eq!(h(20), Some(HitRate { hits: 3, total: 4 }));
        // The last 3 trading days of Europe are 06, 07, 08 (05 has only a silent room row, still a day).
        assert_eq!(h(3), Some(HitRate { hits: 2, total: 2 }));
        // 4 days: 05, 06, 07, 08.
        assert_eq!(h(4), Some(HitRate { hits: 2, total: 2 }));
        assert_eq!(h(5), Some(HitRate { hits: 2, total: 3 }));
        assert_eq!(h(0), None);
        assert!((h(20).unwrap().rate() - 0.75).abs() < 1e-12);
        // The US has its own days and verdicts.
        assert_eq!(
            factor_hit_rate(&rows, "bias_a", Region::Us, d("2026-10-12"), 20),
            Some(HitRate { hits: 0, total: 1 })
        );
        // Silent only → no verdict at all.
        assert_eq!(
            factor_hit_rate(&rows, "room", Region::Eu, d("2026-10-03"), 20),
            Some(HitRate { hits: 1, total: 1 })
        );
        assert_eq!(
            factor_hit_rate(&rows, "room", Region::Eu, d("2026-10-06"), 1),
            None
        );
        assert_eq!(
            factor_hit_rate(&rows, "nope", Region::Eu, d("2026-10-12"), 20),
            None
        );
        assert_eq!(
            factor_hit_rate(&rows, "bias_a", Region::Eu, d("2026-10-01"), 20),
            None
        );
        assert_eq!(
            factor_hit_rate(&[], "bias_a", Region::Eu, d("2026-10-12"), HIT_RATE_DAYS),
            None
        );
    }

    #[test]
    fn hit_rate_on_the_similar_days_only() {
        let rows = parse_csv(SCORES);
        let on = |ds: &[&str]| {
            let dates: Vec<NaiveDate> = ds.iter().map(|x| d(x)).collect();
            factor_hit_rate_on(&rows, "bias_a", Region::Eu, &dates)
        };
        assert_eq!(
            on(&["2026-10-01", "2026-10-02", "2026-10-08"]),
            Some(HitRate { hits: 2, total: 3 })
        );
        assert_eq!(on(&["2026-10-06"]), None, "silent that day");
        assert_eq!(
            on(&["2026-10-12", "2026-10-12"]),
            Some(HitRate { hits: 0, total: 1 }),
            "a date listed twice counts once"
        );
        assert_eq!(on(&[]), None);
        assert_eq!(
            serde_json::to_value(on(&["2026-10-01"])).unwrap(),
            serde_json::json!({"hits": 1, "total": 1})
        );
    }
}
