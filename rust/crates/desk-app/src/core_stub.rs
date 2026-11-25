//! Stand-ins for desk-core types that do not exist yet.
//!
//! TODO(core): replace every item here with its `desk_core` twin
//! (`desk_core::day::Day`, `desk_core::vault::{Region, HumanRead, DayAnswers}`)
//! and delete this module. The shapes follow `src/shared/macro.ts`, so the
//! files written today are the files the skill reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// TODO(core): `desk_core::day::Day`. Until then: `{"date": "...", "files":
/// {name: parsed JSON | text}}`, built by `day::build` from the raw files.
pub type DayData = serde_json::Value;

/// TODO(core): `desk_core::vault::Region`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    Eu,
    Us,
}

impl Region {
    pub const ALL: [Region; 2] = [Region::Eu, Region::Us];

    pub fn as_str(self) -> &'static str {
        match self {
            Region::Eu => "eu",
            Region::Us => "us",
        }
    }
}

/// TODO(core): `desk_core::vault::HumanSide`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HumanSide {
    Long,
    Short,
    Neutral,
    NoTrade,
}

/// `state/human_read_<D>_<region>.json`. TODO(core): `desk_core::vault::HumanRead`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanRead {
    pub date: String,
    pub region: Region,
    pub side: HumanSide,
    /// Kyiv wall clock, «YYYY-MM-DD HH:MM».
    pub at: String,
    /// True when the scale was not on screen yet (the honest case).
    pub before_scale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeTaken {
    None,
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByRule {
    Yes,
    No,
    Unsure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayAnswerRegion {
    pub trade: TradeTaken,
    /// Only meaningful when a trade was taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_rule: Option<ByRule>,
}

/// `state/day_answers_<D>.json`. TODO(core): `desk_core::vault::DayAnswers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayAnswers {
    pub date: String,
    pub at: String,
    pub regions: BTreeMap<Region, DayAnswerRegion>,
    /// «Что помогло или помешало» — one line, free text.
    pub note: String,
}
