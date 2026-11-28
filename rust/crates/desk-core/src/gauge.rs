//! Gauge geometry and the factor split, as `needleAngle` / `polar` /
//! `arcPath` / `splitFactors` in `src/shared/macro.ts`. The UI draws with
//! egui's `Painter`, so `arc_path` (SVG) is kept for parity and the golden
//! screens; `needle_angle` + `polar` are what a painter needs.

use crate::js;
use crate::vault::{ScaleFactor, Side};
use crate::zones::clamp_needle;

/// `needleAngle`: −100 (strong short) sits at the left end of the arc (180°),
/// +100 at the right end (0°). Degrees, mathematical (counter-clockwise from +x).
pub fn needle_angle(needle: f64) -> f64 {
    90.0 - clamp_needle(needle) * 0.9
}

/// `polar`: point on a circle centred at (cx, cy); y grows downwards, as on screen.
pub fn polar(cx: f64, cy: f64, r: f64, angle_deg: f64) -> (f64, f64) {
    let a = angle_deg * std::f64::consts::PI / 180.0;
    (cx + r * a.cos(), cy - r * a.sin())
}

/// `arcPath`: SVG path of an annular arc between two needle values (from < to).
pub fn arc_path(cx: f64, cy: f64, r0: f64, r1: f64, from: f64, to: f64) -> String {
    let a0 = needle_angle(from);
    let a1 = needle_angle(to);
    let (x0, y0) = polar(cx, cy, r1, a0);
    let (x1, y1) = polar(cx, cy, r1, a1);
    let (x2, y2) = polar(cx, cy, r0, a1);
    let (x3, y3) = polar(cx, cy, r0, a0);
    let f = |n: f64| js::to_fixed(n, 2);
    let (r0s, r1s) = (js::number_to_string(r0), js::number_to_string(r1));
    format!(
        "M{} {} A{r1s} {r1s} 0 0 1 {} {} L{} {} A{r0s} {r0s} 0 0 0 {} {} Z",
        f(x0),
        f(y0),
        f(x1),
        f(y1),
        f(x2),
        f(y2),
        f(x3),
        f(y3)
    )
}

/// The factors split the way the report reads them.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FactorSplit<'a> {
    pub long: Vec<&'a ScaleFactor>,
    pub short: Vec<&'a ScaleFactor>,
    pub silent: Vec<&'a ScaleFactor>,
}

/// `splitFactors`: for long, for short, silent — heaviest first; switched-off
/// factors are left out.
pub fn split_factors(factors: &[ScaleFactor]) -> FactorSplit<'_> {
    let by = |a: &&ScaleFactor, b: &&ScaleFactor| {
        js_cmp(b.points(), a.points()).then_with(|| js_cmp(b.share(), a.share()))
    };
    let on: Vec<&ScaleFactor> = factors.iter().filter(|f| f.is_on()).collect();
    let pick = |side: Side| {
        let mut v: Vec<&ScaleFactor> = on
            .iter()
            .copied()
            .filter(|f| f.side == Some(side) && f.points() > 0.0)
            .collect();
        v.sort_by(by);
        v
    };
    let mut silent: Vec<&ScaleFactor> = on
        .iter()
        .copied()
        .filter(|f| f.side.is_none() || f.points() <= 0.0)
        .collect();
    silent.sort_by(|a, b| js_cmp(b.share(), a.share()));
    FactorSplit {
        long: pick(Side::Long),
        short: pick(Side::Short),
        silent,
    }
}

/// `a - b` as a sort comparator (NaN compares equal, as a JS comparator returning NaN does).
fn js_cmp(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strong_short_left_strong_long_right_neutral_on_top() {
        assert_eq!(needle_angle(-100.0), 180.0);
        assert_eq!(needle_angle(0.0), 90.0);
        assert_eq!(needle_angle(100.0), 0.0);
        assert_eq!(needle_angle(250.0), 0.0);
    }

    #[test]
    fn arc_path_matches_the_svg_of_the_ts() {
        let p = arc_path(160.0, 172.0, 104.0, 142.0, -100.0, -40.0);
        assert!(p.starts_with("M18.00 172.00 A142 142 0 0 1 "), "{p}");
        assert!(p.ends_with(" Z"));
        let (x, y) = polar(0.0, 0.0, 1.0, 90.0);
        assert!(x.abs() < 1e-12 && (y + 1.0).abs() < 1e-12);
    }

    fn f(id: &str, side: Option<Side>, share: f64, strength: f64, on: bool) -> ScaleFactor {
        ScaleFactor {
            id: Some(id.into()),
            name: Some(id.into()),
            group: Some("g".into()),
            side,
            share: Some(share),
            strength: Some(strength),
            points: Some(if side.is_some() {
                share * strength
            } else {
                0.0
            }),
            on: Some(on),
            ..Default::default()
        }
    }

    #[test]
    fn splits_by_side_heaviest_first_silent_and_off_apart() {
        let fs = [
            f("a", Some(Side::Long), 10.0, 0.5, true),
            f("b", Some(Side::Long), 6.0, 1.0, true),
            f("c", Some(Side::Short), 12.0, 1.0, true),
            f("d", None, 5.0, 0.0, true),
            f("e", Some(Side::Short), 8.0, 1.0, false),
        ];
        let r = split_factors(&fs);
        let ids = |v: &[&ScaleFactor]| v.iter().map(|x| x.id.clone().unwrap()).collect::<Vec<_>>();
        assert_eq!(ids(&r.long), ["b", "a"]);
        assert_eq!(ids(&r.short), ["c"]);
        assert_eq!(ids(&r.silent), ["d"]);
    }
}
