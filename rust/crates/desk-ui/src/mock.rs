//! Stand-ins for the desk-core types the screen will draw (`ScaleResult`,
//! `GroupResult`, `ScaleLight` of `src/shared/macro.ts`), with one demo day.
//!
//! TEMPORARY: desk-core and desk-app are written in parallel. The lead swaps
//! these for `desk_core::vault::*` and a `Snapshot` from desk-app; the field
//! names follow the TS types so the swap is mechanical.

use desk_core::zones::Zones;

/// One group of factors (`GroupResult`), with its name from the map key.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupResult {
    pub name: String,
    /// The group's share of the whole scale, percent.
    pub share: f32,
    pub long: f32,
    pub short: f32,
    pub needle: f32,
}

/// `ScaleResult`: the split and the needle, `groups` in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleResult {
    pub long: f32,
    pub short: f32,
    pub undecided: f32,
    pub needle: f32,
    pub groups: Vec<GroupResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightColor {
    Green,
    Yellow,
    Red,
}

/// `ScaleLight` without the reason lists (not drawn yet).
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleLight {
    pub color: LightColor,
    pub risk_pct: f32,
}

/// What the hero card needs from `ScaleState`.
#[derive(Clone, Debug, PartialEq)]
pub struct ScaleState {
    pub result: ScaleResult,
    /// Needle after the discussion; `None` = no discussion.
    pub after: Option<ScaleResult>,
    pub light: ScaleLight,
    pub zones: Zones,
}

fn group(name: &str, share: f32, long: f32, short: f32) -> GroupResult {
    GroupResult {
        name: name.to_owned(),
        share,
        long,
        short,
        needle: (long - short).round(),
    }
}

/// A Europe window leaning long, shaped like the demo vault's
/// (`scripts/make-demo-vault.py`: groups «контекст дня», «окно открытия», «макро»).
pub fn demo_scale() -> ScaleState {
    let result = ScaleResult {
        long: 55.2,
        short: 24.8,
        undecided: 20.0,
        needle: 30.0,
        groups: vec![
            group("контекст дня", 33.0, 21.5, 6.0),
            group("окно открытия", 47.0, 27.2, 9.6),
            group("макро", 20.0, 6.5, 9.2),
        ],
    };
    ScaleState {
        after: None,
        light: ScaleLight {
            color: LightColor::Green,
            risk_pct: 0.5,
        },
        zones: Zones::default(),
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_split_adds_up() {
        let s = demo_scale();
        let r = &s.result;
        assert!((r.long + r.short + r.undecided - 100.0).abs() < 0.01);
        let shares: f32 = r.groups.iter().map(|g| g.share).sum();
        assert!((shares - 100.0).abs() < 0.01);
    }
}
