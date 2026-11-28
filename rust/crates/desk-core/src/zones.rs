//! The needle's zones: ±15 % neutral, ±40 % strong (from `config/scale.json`),
//! as `zoneOf` / `zoneSegments` / `clampNeedle` in `src/shared/macro.ts`.

use crate::js;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// `DEFAULT_ZONES`.
pub const DEFAULT_NEUTRAL: f64 = 15.0;
pub const DEFAULT_STRONG: f64 = 40.0;

/// The zone thresholds of a day (`MacroDay.zones`).
///
/// `loadDay` takes `cfg.zones.neutral ?? 15` without checking the type, so a
/// config holding `"neutral": "20"` puts the string into the day. `Zones`
/// keeps what the config said for serialization (the golden files) and
/// gives the drawing code numbers: a value that is not a number reads as the
/// default.
#[derive(Debug, Clone, PartialEq)]
pub struct Zones {
    pub neutral: f64,
    pub strong: f64,
    raw: Option<(Value, Value)>,
}

impl Default for Zones {
    fn default() -> Self {
        Zones {
            neutral: DEFAULT_NEUTRAL,
            strong: DEFAULT_STRONG,
            raw: None,
        }
    }
}

impl Zones {
    pub fn new(neutral: f64, strong: f64) -> Self {
        Zones {
            neutral,
            strong,
            raw: None,
        }
    }

    /// The zones of a parsed `config/scale.json` (any JSON, or none).
    pub fn from_config(cfg: Option<&Value>) -> Self {
        let z = js::prop(cfg, "zones");
        let pick = |k: &str, d: f64| -> (f64, Value) {
            match js::nullish(js::prop(z, k)) {
                Some(v) => (v.as_f64().unwrap_or(d), v.clone()),
                None => (d, js::number(d)),
            }
        };
        let (neutral, rn) = pick("neutral", DEFAULT_NEUTRAL);
        let (strong, rs) = pick("strong", DEFAULT_STRONG);
        Zones {
            neutral,
            strong,
            raw: Some((rn, rs)),
        }
    }
}

impl Serialize for Zones {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (n, st) = match &self.raw {
            Some((n, st)) => (n.clone(), st.clone()),
            None => (js::number(self.neutral), js::number(self.strong)),
        };
        let mut m = s.serialize_struct("Zones", 2)?;
        m.serialize_field("neutral", &n)?;
        m.serialize_field("strong", &st)?;
        m.end()
    }
}

impl<'de> Deserialize<'de> for Zones {
    /// Reads `{"neutral": …, "strong": …}` the way `loadDay` reads the config's `zones`.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let mut cfg = serde_json::Map::new();
        cfg.insert("zones".into(), v);
        Ok(Zones::from_config(Some(&Value::Object(cfg))))
    }
}

/// `ZoneKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ZoneKey {
    StrongShort,
    Short,
    Neutral,
    Long,
    StrongLong,
}

impl ZoneKey {
    /// `ZONE_KEYS`, left to right on the dial.
    pub const ALL: [ZoneKey; 5] = [
        ZoneKey::StrongShort,
        ZoneKey::Short,
        ZoneKey::Neutral,
        ZoneKey::Long,
        ZoneKey::StrongLong,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ZoneKey::StrongShort => "strong-short",
            ZoneKey::Short => "short",
            ZoneKey::Neutral => "neutral",
            ZoneKey::Long => "long",
            ZoneKey::StrongLong => "strong-long",
        }
    }
}

/// `zoneOf`: same thresholds as scale.py — |needle| < neutral → neutral, ≥ strong → strong.
pub fn zone_of(needle: f64, zones: &Zones) -> ZoneKey {
    let a = needle.abs();
    if a < zones.neutral {
        return ZoneKey::Neutral;
    }
    if needle > 0.0 {
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

/// `clampNeedle`: into [−100, 100]; not a number → 0.
pub fn clamp_needle(n: f64) -> f64 {
    if !n.is_finite() {
        return 0.0;
    }
    n.clamp(-100.0, 100.0)
}

/// One segment of the dial as a needle range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ZoneSegment {
    pub key: ZoneKey,
    pub from: f64,
    pub to: f64,
}

/// `zoneSegments`: the five segments of the gauge, left to right.
pub fn zone_segments(zones: &Zones) -> [ZoneSegment; 5] {
    let (n, s) = (zones.neutral, zones.strong);
    [
        ZoneSegment {
            key: ZoneKey::StrongShort,
            from: -100.0,
            to: -s,
        },
        ZoneSegment {
            key: ZoneKey::Short,
            from: -s,
            to: -n,
        },
        ZoneSegment {
            key: ZoneKey::Neutral,
            from: -n,
            to: n,
        },
        ZoneSegment {
            key: ZoneKey::Long,
            from: n,
            to: s,
        },
        ZoneSegment {
            key: ZoneKey::StrongLong,
            from: s,
            to: 100.0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uses_the_same_thresholds_as_scale_py() {
        let z = Zones::default();
        assert_eq!(zone_of(0.0, &z), ZoneKey::Neutral);
        assert_eq!(zone_of(14.0, &z), ZoneKey::Neutral);
        assert_eq!(zone_of(-14.9, &z), ZoneKey::Neutral);
        assert_eq!(zone_of(15.0, &z), ZoneKey::Long);
        assert_eq!(zone_of(-15.0, &z), ZoneKey::Short);
        assert_eq!(zone_of(39.0, &z), ZoneKey::Long);
        assert_eq!(zone_of(40.0, &z), ZoneKey::StrongLong);
        assert_eq!(zone_of(-58.0, &z), ZoneKey::StrongShort);
        assert_eq!(zone_of(20.0, &Zones::new(25.0, 50.0)), ZoneKey::Neutral);
    }

    #[test]
    fn covers_the_whole_dial_with_five_contiguous_segments() {
        let segs = zone_segments(&Zones::default());
        assert_eq!(segs.map(|s| s.key), ZoneKey::ALL);
        assert_eq!(segs[0].from, -100.0);
        assert_eq!(segs[4].to, 100.0);
        for i in 1..segs.len() {
            assert_eq!(segs[i].from, segs[i - 1].to);
        }
    }

    #[test]
    fn clamps_the_needle() {
        assert_eq!(clamp_needle(250.0), 100.0);
        assert_eq!(clamp_needle(-250.0), -100.0);
        assert_eq!(clamp_needle(f64::NAN), 0.0);
        assert_eq!(clamp_needle(-57.0), -57.0);
    }

    #[test]
    fn zones_from_config_keep_what_the_file_said() {
        let z = Zones::from_config(Some(&json!({"zones": {"neutral": 20}})));
        assert_eq!((z.neutral, z.strong), (20.0, 40.0));
        assert_eq!(
            serde_json::to_value(&z).unwrap(),
            json!({"neutral": 20, "strong": 40})
        );
        let z = Zones::from_config(Some(&json!({"zones": {"neutral": "x", "strong": null}})));
        assert_eq!((z.neutral, z.strong), (15.0, 40.0));
        assert_eq!(
            serde_json::to_value(&z).unwrap(),
            json!({"neutral": "x", "strong": 40})
        );
        let z = Zones::from_config(None);
        assert_eq!(
            serde_json::to_value(&z).unwrap(),
            json!({"neutral": 15, "strong": 40})
        );
        assert_eq!(
            serde_json::to_value(ZoneKey::StrongShort).unwrap(),
            json!("strong-short")
        );
    }
}
