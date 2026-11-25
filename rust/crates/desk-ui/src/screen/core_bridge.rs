//! Where the screen meets desk-core's «how often was this argument right»:
//! one function per question, so a change of source is a one-line swap.

use desk_core::stats::HitRate;
use desk_core::vault::{IbReach, MoveSide, SimilarDay};
use desk_core::{Day, Region};

/// How often `factor` was right in `region` before `day.date` (the badge
/// under each argument).
///
/// desk-core's `stats::factor_hit_rate(rows, id, region, before,
/// HIT_RATE_DAYS)` wants the rows of `scale_factor_scores.csv`, and neither
/// `Day` nor the `Snapshot` carries them yet (asked of the lead). Until they
/// do, this reads the day's own factor stats — the 30-day window `load_day`
/// already computes from the same CSV — so the badge shows real numbers.
pub fn factor_hit_rate(day: &Day, region: Region, factor: &str) -> Option<HitRate> {
    day.stats
        .iter()
        .find(|s| s.region == region && s.factor.as_deref() == Some(factor))
        .filter(|s| s.total > 0)
        .map(|s| HitRate {
            hits: s.hits,
            total: s.total,
        })
}

/// How today's arguments did on a similar day: of the factors that voted
/// today (`today`), how many the skill scored right on that day
/// (`SimilarDay::factors_hit`, the per-day form of `factor_hit_rate_on`).
pub fn args_on(similar: &SimilarDay, today: &[&str]) -> Option<HitRate> {
    let scored: Vec<bool> = today
        .iter()
        .filter_map(|id| similar.factors_hit.get(*id).copied())
        .collect();
    (!scored.is_empty()).then(|| HitRate {
        hits: scored.iter().filter(|h| **h).count() as u64,
        total: scored.len() as u64,
    })
}

/// The i18n key of a move's side (`macro.dir.*`).
pub fn side_key(s: MoveSide) -> &'static str {
    match s {
        MoveSide::Long => "macro.dir.LONG",
        MoveSide::Short => "macro.dir.SHORT",
        MoveSide::Flat => "macro.dir.FLAT",
    }
}

/// The i18n key of how far the move off the IB went.
pub fn reach_key(r: IbReach) -> &'static str {
    match r {
        IbReach::Ib1 => "desk.ib.IB1",
        IbReach::Ib2 => "desk.ib.IB2",
        IbReach::Ib => "desk.ib.IB",
        IbReach::NoReach => "desk.ib.none",
    }
}

/// A hand-written `similar_<D>_eu.json` for the screen tests and snapshots:
/// the demo vault has no such file.
#[cfg(test)]
pub fn sample_similar(date: &str) -> desk_core::vault::Similar {
    let v = serde_json::json!({
        "date": date, "region": "eu", "asof": format!("{date} 09:31"),
        "days": [
            {"date": "2026-09-08", "score": 0.84, "match": ["пила", "ночь ≥ 0.8 ADR", "стрелка −41"],
             "day_move": {"side": "SHORT", "pct_adr": -0.62}, "ib_move": {"side": "SHORT", "reach": "IB1"},
             "verdict": "план дошёл до IB1",
             "factors_hit": {"night_sweep": true, "room": true, "bias_a": true, "m_verdict": true,
                             "m15_flow": false, "opens": true, "dr_zone": true, "prev_close": true, "d1_flow": false}},
            {"date": "2026-08-27", "score": 0.71, "match": ["сняли максимум ночи", "новости за шорт"],
             "day_move": {"side": "FLAT", "pct_adr": 0.18}, "ib_move": {"side": "LONG", "reach": "IB"},
             "factors_hit": {"night_sweep": false, "room": true, "bias_a": false, "m_verdict": true,
                             "m15_flow": false, "opens": true, "dr_zone": false, "prev_close": true, "d1_flow": false}},
            {"date": "2026-08-13", "score": 0.66, "match": ["понедельник после тренда", "VIX растёт"],
             "day_move": {"side": "SHORT", "pct_adr": -0.91}, "ib_move": {"side": "SHORT", "reach": "IB2"},
             "factors_hit": {"night_sweep": true, "room": true, "bias_a": true, "m_verdict": true,
                             "m15_flow": true, "opens": true, "dr_zone": true, "prev_close": true, "d1_flow": false}}
        ]
    });
    desk_core::normalize::normalize_similar(&v, date, Region::Eu)
        .expect("the sample is a similar file")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todays_arguments_on_a_similar_day() {
        let s = sample_similar("2026-09-14");
        let d = &s.days[0];
        // Only today's voting factors count; unknown ones are skipped.
        assert_eq!(
            args_on(d, &["night_sweep", "d1_flow", "not_scored"]),
            Some(HitRate { hits: 1, total: 2 })
        );
        assert_eq!(args_on(d, &["not_scored"]), None);
    }
}
