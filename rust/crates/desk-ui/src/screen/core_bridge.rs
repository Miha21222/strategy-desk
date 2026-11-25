//! The two desk-core calls the screen is waiting for, each behind one
//! function so the swap is a one-line change when they land:
//!
//! * `desk_core::stats::factor_hit_rate` — how often a factor's vote was
//!   right over the last 20 trading days before the date;
//! * `Day.<region>.similar` (`similar_<D>_<region>.json`) with
//!   `desk_core::stats::factor_hit_rate_on` — similar days and how today's
//!   arguments did on them.

use desk_core::{Day, Region};

/// «7 из 10».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HitRate {
    pub hits: u64,
    pub total: u64,
}

impl HitRate {
    pub fn rate(self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.hits as f64 / self.total as f64
        }
    }
}

/// How often `factor` was right in `region` before `day.date`.
///
/// TEMPORARY: until `desk_core::stats::factor_hit_rate` lands this reads the
/// day's own factor stats (`day.stats`, the 30-day CSV window `load_day`
/// already computes), so the badge has real numbers in the meantime.
pub fn factor_hit_rate(day: &Day, region: Region, factor: &str) -> Option<HitRate> {
    day.stats
        .iter()
        .find(|s| s.region == region && s.factor.as_deref() == Some(factor))
        .map(|s| HitRate {
            hits: s.hits,
            total: s.total,
        })
}

/// One similar day, as the card draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct SimilarDay {
    /// `YYYY-MM-DD`.
    pub date: String,
    /// What matched, short («пила, ночь ≥ 0.8 ADR, CPI в окне»).
    pub matched: String,
    /// How the day went: `LONG` / `SHORT` / `FLAT`.
    pub day_dir: Option<String>,
    /// The day's move in ADR.
    pub day_adr: Option<f64>,
    /// The move off the IB, in words («вниз, IB1 за 40 минут»).
    pub ib: Option<String>,
    /// Today's arguments on that day (`factor_hit_rate_on`).
    pub args: Option<HitRate>,
    /// The board's verdict for that day, when there is one.
    pub verdict: Option<String>,
}

/// Up to three similar days for the window.
///
/// TEMPORARY: desk-core has no `similar` yet, so this is empty and the card
/// stays silent; the screen tests feed a sample through `ScreenState`.
pub fn similar_days(_day: &Day, _region: Region) -> Vec<SimilarDay> {
    Vec::new()
}

/// A sample for the screen tests and the snapshots until desk-core lands.
#[cfg(test)]
pub fn sample_similar() -> Vec<SimilarDay> {
    vec![
        SimilarDay {
            date: "2026-09-08".into(),
            matched: "пила, ночь ≥ 0.8 ADR, стрелка −41".into(),
            day_dir: Some("SHORT".into()),
            day_adr: Some(0.6),
            ib: Some("вниз, IB1 за 40 минут".into()),
            args: Some(HitRate { hits: 7, total: 9 }),
            verdict: Some("план дошёл до IB1".into()),
        },
        SimilarDay {
            date: "2026-08-27".into(),
            matched: "сняли максимум ночи, новости за шорт".into(),
            day_dir: Some("FLAT".into()),
            day_adr: Some(0.2),
            ib: Some("в обе стороны, без цели".into()),
            args: Some(HitRate { hits: 4, total: 9 }),
            verdict: None,
        },
        SimilarDay {
            date: "2026-08-13".into(),
            matched: "понедельник после тренда, VIX растёт".into(),
            day_dir: Some("SHORT".into()),
            day_adr: Some(0.9),
            ib: Some("вниз, IB2".into()),
            args: Some(HitRate { hits: 8, total: 9 }),
            verdict: None,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate() {
        assert_eq!(HitRate { hits: 7, total: 10 }.rate(), 0.7);
        assert_eq!(HitRate { hits: 0, total: 0 }.rate(), 0.0);
    }
}
