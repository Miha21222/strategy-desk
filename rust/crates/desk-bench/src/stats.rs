//! Sample statistics with the exact definitions docs/rust/PERF.md promises.
//!
//! Percentiles are nearest-rank (no interpolation): with 10 samples p95 is the
//! largest one. That is deliberate — an interpolated p95 of 10 launches would
//! report a number no launch ever took. bench.ps1 and report.py use the same
//! rule, so the three never disagree on a report.

/// Nearest-rank percentile, `p` in (0, 100]. `None` for no samples.
pub fn percentile(samples: &[f64], p: f64) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut v: Vec<f64> = samples.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * v.len() as f64).ceil() as usize;
    let idx = rank.clamp(1, v.len()) - 1;
    Some(v[idx])
}

/// Median: mean of the two middle samples for an even count.
pub fn median(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    let mut v: Vec<f64> = samples.to_vec();
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

pub fn mean(samples: &[f64]) -> Option<f64> {
    if samples.is_empty() {
        None
    } else {
        Some(samples.iter().sum::<f64>() / samples.len() as f64)
    }
}

/// Round for the report: three significant decimals are more than any of
/// these clocks can honestly resolve.
pub fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank() {
        let s: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(percentile(&s, 95.0), Some(10.0));
        assert_eq!(percentile(&s, 50.0), Some(5.0));
        assert_eq!(percentile(&s, 10.0), Some(1.0));
        assert_eq!(percentile(&[], 50.0), None);
        let s100: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&s100, 99.0), Some(99.0));
    }

    #[test]
    fn median_even_odd() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        assert_eq!(mean(&[1.0, 2.0]), Some(1.5));
    }
}
