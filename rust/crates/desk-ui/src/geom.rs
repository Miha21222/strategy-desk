//! Gauge geometry and zones — a straight port of the pure helpers in
//! `src/shared/macro.ts` (`zoneOf`, `clampNeedle`, `needleAngle`, `polar`,
//! `zoneSegments`).
//!
//! TEMPORARY HOME: CONTRACTS.md gives `zones` / `gauge` to desk-core. Until
//! desk-core lands them this module keeps the UI self-contained; the lead
//! swaps the call sites to `desk_core::gauge` and deletes this file.

use eframe::egui::{pos2, Pos2};

/// Zone thresholds in needle percent, from `config/scale.json`
/// (`|needle| < neutral` → neutral, `>= strong` → strong).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zones {
    pub neutral: f32,
    pub strong: f32,
}

impl Default for Zones {
    /// `DEFAULT_ZONES` of the TS desk; real values come from the scale config.
    fn default() -> Self {
        Self {
            neutral: 15.0,
            strong: 40.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZoneKey {
    StrongShort,
    Short,
    Neutral,
    Long,
    StrongLong,
}

impl ZoneKey {
    /// The key fragment the i18n strings use (`macro.zone.<slug>`).
    pub fn slug(self) -> &'static str {
        match self {
            Self::StrongShort => "strong-short",
            Self::Short => "short",
            Self::Neutral => "neutral",
            Self::Long => "long",
            Self::StrongLong => "strong-long",
        }
    }
}

/// Same thresholds as scale.py.
pub fn zone_of(needle: f32, zones: Zones) -> ZoneKey {
    let a = needle.abs();
    if a < zones.neutral {
        ZoneKey::Neutral
    } else if needle > 0.0 {
        if a >= zones.strong {
            ZoneKey::StrongLong
        } else {
            ZoneKey::Long
        }
    } else if a >= zones.strong {
        ZoneKey::StrongShort
    } else {
        ZoneKey::Short
    }
}

/// −100..100; NaN and infinities become 0 like `Number.isFinite` in TS.
pub fn clamp_needle(n: f32) -> f32 {
    if n.is_finite() {
        n.clamp(-100.0, 100.0)
    } else {
        0.0
    }
}

/// −100 sits at the left end of the arc (180°), +100 at the right end (0°).
/// Degrees, mathematical (counter-clockwise from +x).
pub fn needle_angle(needle: f32) -> f32 {
    90.0 - clamp_needle(needle) * 0.9
}

/// Point on a circle centred at (cx, cy); y grows downwards as on screen.
pub fn polar(cx: f32, cy: f32, r: f32, angle_deg: f32) -> Pos2 {
    let a = angle_deg.to_radians();
    pos2(cx + r * a.cos(), cy - r * a.sin())
}

/// The five segments of the gauge as needle ranges, left to right.
pub fn zone_segments(zones: Zones) -> [(ZoneKey, f32, f32); 5] {
    [
        (ZoneKey::StrongShort, -100.0, -zones.strong),
        (ZoneKey::Short, -zones.strong, -zones.neutral),
        (ZoneKey::Neutral, -zones.neutral, zones.neutral),
        (ZoneKey::Long, zones.neutral, zones.strong),
        (ZoneKey::StrongLong, zones.strong, 100.0),
    ]
}

/// `signed()` of Gauge.tsx: rounded, real minus sign, `%` glued on.
pub fn signed(n: f32) -> String {
    let r = n.round();
    let sign = if r > 0.0 {
        "+"
    } else if r < 0.0 {
        "\u{2212}"
    } else {
        ""
    };
    format!("{sign}{}%", r.abs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const Z: Zones = Zones {
        neutral: 15.0,
        strong: 40.0,
    };

    #[test]
    fn zones_follow_scale_py() {
        assert_eq!(zone_of(0.0, Z), ZoneKey::Neutral);
        assert_eq!(zone_of(14.9, Z), ZoneKey::Neutral);
        assert_eq!(zone_of(15.0, Z), ZoneKey::Long);
        assert_eq!(zone_of(-15.0, Z), ZoneKey::Short);
        assert_eq!(zone_of(40.0, Z), ZoneKey::StrongLong);
        assert_eq!(zone_of(-40.0, Z), ZoneKey::StrongShort);
        assert_eq!(zone_of(-39.9, Z), ZoneKey::Short);
    }

    #[test]
    fn zones_are_inputs_not_constants() {
        let wide = Zones {
            neutral: 25.0,
            strong: 60.0,
        };
        assert_eq!(zone_of(20.0, wide), ZoneKey::Neutral);
        assert_eq!(zone_of(50.0, wide), ZoneKey::Long);
        assert_eq!(zone_segments(wide)[1], (ZoneKey::Short, -60.0, -25.0));
    }

    #[test]
    fn needle_is_clamped() {
        assert_eq!(clamp_needle(250.0), 100.0);
        assert_eq!(clamp_needle(-250.0), -100.0);
        assert_eq!(clamp_needle(f32::NAN), 0.0);
        assert_eq!(clamp_needle(f32::INFINITY), 0.0);
    }

    #[test]
    fn angles_span_the_half_dial() {
        assert_eq!(needle_angle(-100.0), 180.0);
        assert_eq!(needle_angle(0.0), 90.0);
        assert_eq!(needle_angle(100.0), 0.0);
        let top = polar(160.0, 172.0, 10.0, 90.0);
        assert!((top.x - 160.0).abs() < 1e-4 && (top.y - 162.0).abs() < 1e-4);
    }

    #[test]
    fn segments_cover_the_dial() {
        let s = zone_segments(Z);
        assert_eq!(s[0].1, -100.0);
        assert_eq!(s[4].2, 100.0);
        for w in s.windows(2) {
            assert_eq!(w[0].2, w[1].1, "segments must touch");
        }
    }

    #[test]
    fn signed_matches_ts() {
        assert_eq!(signed(30.4), "+30%");
        assert_eq!(signed(-80.0), "\u{2212}80%");
        assert_eq!(signed(0.2), "0%");
        assert_eq!(signed(-0.2), "0%");
    }
}
