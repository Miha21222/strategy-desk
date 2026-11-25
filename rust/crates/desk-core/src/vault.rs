//! The vault contract: the files the `macro-review` skill writes into
//! `indices-kb/state/` (and the owner's two), as `src/shared/macro.ts` types them.
//!
//! Two kinds of types live here:
//!
//! - **Normalized files** (`HumanRead`, `DayAnswers`, `Remember`, `Drift`,
//!   `Week`, `Ideas`, `Shots`): the TS builds these field by field in a
//!   `normalize*` function, so they are plain structs whose `Serialize` writes
//!   exactly the fields, order and `null`s the TS object has.
//! - **Passed-through files** (`ScaleFile`, `MacroVerdict`, `MacroFile`,
//!   `ReviewFile`, `ReviewAgent`): the TS only casts the parsed JSON
//!   (`as ScaleState`), so the desk shows — and `loadDay` returns — whatever
//!   the skill wrote, unknown fields and wrong types included. These come
//!   wrapped in [`Raw`]: it serializes the JSON as read (that is what the
//!   golden files hold) and derefs to a typed, lenient view for drawing,
//!   where a missing or wrong-typed field is simply `None` / empty and never
//!   fails the parse.

use crate::js;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::ops::Deref;

// ---------------------------------------------------------------- basic enums

/// `Region` of macro.ts: Europe (FDAX/FESX) or the US (ES/NQ).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
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

    /// `r === 'eu' || r === 'us'`.
    pub fn parse(s: &str) -> Option<Region> {
        match s {
            "eu" => Some(Region::Eu),
            "us" => Some(Region::Us),
            _ => None,
        }
    }

    /// `REGION_NOTE` of macro-state.ts: the region's word in a note's file name.
    pub fn note_word(self) -> &'static str {
        match self {
            Region::Eu => "Европа",
            Region::Us => "США",
        }
    }
}

impl std::fmt::Display for Region {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `Side` of macro.ts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Long,
    Short,
}

impl Side {
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Long => "LONG",
            Side::Short => "SHORT",
        }
    }
}

/// `HumanSide`: what the owner expects of the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HumanSide {
    Long,
    Short,
    Neutral,
    NoTrade,
}

impl HumanSide {
    /// `HUMAN_SIDES`.
    pub const ALL: [HumanSide; 4] = [
        HumanSide::Long,
        HumanSide::Short,
        HumanSide::Neutral,
        HumanSide::NoTrade,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            HumanSide::Long => "LONG",
            HumanSide::Short => "SHORT",
            HumanSide::Neutral => "NEUTRAL",
            HumanSide::NoTrade => "NO_TRADE",
        }
    }

    pub fn parse(s: &str) -> Option<HumanSide> {
        HumanSide::ALL.into_iter().find(|h| h.as_str() == s)
    }
}

/// `TradeTaken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeTaken {
    None,
    Long,
    Short,
}

impl TradeTaken {
    pub fn parse(s: &str) -> Option<TradeTaken> {
        match s {
            "none" => Some(TradeTaken::None),
            "long" => Some(TradeTaken::Long),
            "short" => Some(TradeTaken::Short),
            _ => None,
        }
    }
}

/// `ByRule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByRule {
    Yes,
    No,
    Unsure,
}

impl ByRule {
    pub fn parse(s: &str) -> Option<ByRule> {
        match s {
            "yes" => Some(ByRule::Yes),
            "no" => Some(ByRule::No),
            "unsure" => Some(ByRule::Unsure),
            _ => None,
        }
    }
}

/// `VAULT_PATHS`: folders inside the vault, as path segments.
pub mod paths {
    pub const STATE: [&str; 2] = ["indices-kb", "state"];
    pub const JOURNAL: [&str; 2] = ["indices-kb", "journal"];
    pub const CONFIG: [&str; 3] = ["indices-kb", "config", "scale.json"];
    pub const NOTES: [&str; 1] = ["Макро-обзоры"];
    /// Chart screenshots the skill takes of the review's levels (PNG only).
    pub const SHOTS: [&str; 2] = ["indices-kb", "shots"];
}

// ---------------------------------------------------------------- Raw<T>

/// A file the TS passes through as parsed: serializes the JSON exactly as
/// read (after the JS number/key normalization of [`crate::js::jsify`]),
/// derefs to a lenient typed view `T`.
#[derive(Debug, Clone, PartialEq)]
pub struct Raw<T> {
    value: Value,
    view: T,
}

impl<T: DeserializeOwned + Default> Raw<T> {
    /// Wrap a parsed value. Never fails: a value that does not look like a
    /// `T` at all gets the default view and keeps its raw JSON.
    pub fn new(value: Value) -> Self {
        let view = T::deserialize(&value).unwrap_or_default();
        Raw { value, view }
    }
}

impl<T> Raw<T> {
    /// The JSON as the skill wrote it.
    pub fn raw(&self) -> &Value {
        &self.value
    }

    /// The typed view.
    pub fn get(&self) -> &T {
        &self.view
    }

    /// JS truthiness of the raw value (`if (r.verdict)`): a verdict file that
    /// holds `false` or `0` is there but does not count.
    pub fn is_truthy(&self) -> bool {
        js::truthy(&self.value)
    }
}

impl<T> Deref for Raw<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.view
    }
}

impl<T> Serialize for Raw<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(s)
    }
}

impl<'de, T: DeserializeOwned + Default> Deserialize<'de> for Raw<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Raw::new(Value::deserialize(d)?))
    }
}

/// Lenient field deserializers for the typed views: a wrong type becomes the
/// field's default instead of failing the whole file.
pub(crate) mod lenient {
    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    pub fn de<'de, D, T>(d: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned + Default,
    {
        let v = Value::deserialize(d)?;
        Ok(T::deserialize(v).unwrap_or_default())
    }

    /// A list where each element that is not a `T` is dropped, not the list.
    pub fn vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        Ok(match Value::deserialize(d)? {
            Value::Array(a) => a
                .into_iter()
                .filter_map(|x| T::deserialize(x).ok())
                .collect(),
            _ => Vec::new(),
        })
    }

    /// A string field; numbers and booleans are read as their JS `String()`.
    pub fn string<'de, D>(d: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match Value::deserialize(d)? {
            Value::String(s) => Some(s),
            v @ (Value::Number(_) | Value::Bool(_)) => Some(crate::js::to_string(&v)),
            _ => None,
        })
    }

    /// A `Side | '' | null` field: anything but LONG / SHORT is no side.
    pub fn side<'de, D>(d: D) -> Result<Option<super::Side>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(match Value::deserialize(d)? {
            Value::String(s) if s == "LONG" => Some(super::Side::Long),
            Value::String(s) if s == "SHORT" => Some(super::Side::Short),
            _ => None,
        })
    }
}

/// `number | string` in a file (`share: 12` or `share: "12"`).
#[derive(Debug, Clone, PartialEq)]
pub enum NumOrStr {
    Num(f64),
    Str(String),
}

impl NumOrStr {
    /// The value read as a number (a numeric string parses), for drawing.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            NumOrStr::Num(n) => Some(*n),
            NumOrStr::Str(s) => js::trim(s).parse().ok(),
        }
    }

    pub(crate) fn from_value(v: Option<&Value>) -> Option<NumOrStr> {
        match v {
            Some(Value::Number(n)) => n.as_f64().map(NumOrStr::Num),
            Some(Value::String(s)) => Some(NumOrStr::Str(s.clone())),
            _ => None,
        }
    }
}

impl Serialize for NumOrStr {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            NumOrStr::Num(n) => js::ser_f64(n, s),
            NumOrStr::Str(x) => s.serialize_str(x),
        }
    }
}

impl<'de> Deserialize<'de> for NumOrStr {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        NumOrStr::from_value(Some(&v))
            .ok_or_else(|| serde::de::Error::custom("not a number or string"))
    }
}

// ---------------------------------------------------------------- scale_<D>_<region>.json

/// `ScaleFactor`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleFactor {
    #[serde(deserialize_with = "lenient::string")]
    pub id: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub group: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub share_cfg: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub on: Option<bool>,
    #[serde(deserialize_with = "lenient::side")]
    pub side: Option<Side>,
    #[serde(deserialize_with = "lenient::de")]
    pub strength: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub why: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub share: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub points: Option<f64>,
}

impl ScaleFactor {
    /// `f.on !== false`.
    pub fn is_on(&self) -> bool {
        self.on != Some(false)
    }
    pub fn share(&self) -> f64 {
        self.share.unwrap_or(0.0)
    }
    pub fn points(&self) -> f64 {
        self.points.unwrap_or(0.0)
    }
}

/// `GroupResult`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct GroupResult {
    #[serde(deserialize_with = "lenient::de")]
    pub share: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub long: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub short: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub needle: Option<f64>,
}

/// `ScaleResult`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleResult {
    #[serde(deserialize_with = "lenient::de")]
    pub long: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub short: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub undecided: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub max: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub needle: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub zone: Option<String>,
    #[serde(deserialize_with = "lenient::side")]
    pub side: Option<Side>,
    /// Group name → result, in file order.
    #[serde(deserialize_with = "lenient::de")]
    pub groups: Option<indexed::Groups>,
}

impl ScaleResult {
    pub fn needle(&self) -> f64 {
        self.needle.unwrap_or(0.0)
    }
}

/// Ordered maps for the typed views (file order matters for drawing).
pub mod indexed {
    use super::GroupResult;
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    /// `Record<string, GroupResult>` in file order; non-object groups are dropped.
    #[derive(Debug, Clone, PartialEq, Default)]
    pub struct Groups(pub Vec<(String, GroupResult)>);

    impl Groups {
        pub fn get(&self, name: &str) -> Option<&GroupResult> {
            self.0.iter().find(|(k, _)| k == name).map(|(_, g)| g)
        }
        pub fn iter(&self) -> impl Iterator<Item = (&str, &GroupResult)> {
            self.0.iter().map(|(k, g)| (k.as_str(), g))
        }
    }

    impl<'de> Deserialize<'de> for Groups {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let v = Value::deserialize(d)?;
            let m = v
                .as_object()
                .ok_or_else(|| serde::de::Error::custom("groups: not an object"))?;
            Ok(Groups(
                m.iter()
                    .filter_map(|(k, g)| GroupResult::deserialize(g).ok().map(|g| (k.clone(), g)))
                    .collect(),
            ))
        }
    }
}

/// `ScaleLight`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleLight {
    /// 'green' | 'yellow' | 'red' | anything the skill invents.
    #[serde(deserialize_with = "lenient::string")]
    pub color: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub label: Option<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub red: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub yellow: Vec<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub risk_pct: Option<f64>,
}

/// `PlanLevel`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct PlanLevel {
    #[serde(deserialize_with = "lenient::de")]
    pub price: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub adr: Option<f64>,
}

/// `PlanSetup`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct PlanSetup {
    #[serde(deserialize_with = "lenient::string")]
    pub setup: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub why: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub entry: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub stop: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub target: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub until: Option<String>,
}

/// `ScalePlan`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScalePlan {
    #[serde(deserialize_with = "lenient::side")]
    pub side: Option<Side>,
    #[serde(deserialize_with = "lenient::de")]
    pub price: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub adr: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub ib_typical_adr: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub ib_typical_class: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub risk_pct: Option<f64>,
    /// `Record<string, string>`; non-string values are dropped.
    #[serde(deserialize_with = "string_map")]
    pub session: Vec<(String, String)>,
    #[serde(deserialize_with = "lenient::vec")]
    pub manipulation_levels: Vec<PlanLevel>,
    #[serde(deserialize_with = "lenient::vec")]
    pub targets: Vec<PlanLevel>,
    #[serde(deserialize_with = "lenient::vec")]
    pub setups: Vec<PlanSetup>,
    #[serde(deserialize_with = "lenient::string")]
    pub cancel: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub summary: Option<String>,
}

fn string_map<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(String, String)>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => m
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned())))
            .collect(),
        _ => Vec::new(),
    })
}

/// `DiscussionItem`: an argument about the scale (the owner's or the agent's).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct DiscussionItem {
    #[serde(deserialize_with = "lenient::string")]
    pub at: Option<String>,
    /// 'owner' | 'agent' | …
    #[serde(deserialize_with = "lenient::string")]
    pub by: Option<String>,
    #[serde(deserialize_with = "lenient::side")]
    pub side: Option<Side>,
    #[serde(deserialize_with = "lenient::de")]
    pub share: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub text: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub reply: Option<String>,
    /// 'agree' | 'partial' | 'disagree' | '' | …
    #[serde(deserialize_with = "lenient::string")]
    pub agent_view: Option<String>,
}

/// One entry of `ScaleState.history`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleHistory {
    #[serde(deserialize_with = "lenient::string")]
    pub asof: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub needle: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub after: Option<f64>,
}

/// `state/scale_<D>_<region>.json` — `ScaleState` of macro.ts, after
/// [`crate::normalize::normalize_scale`] (v1 files get their percentages).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleFile {
    #[serde(deserialize_with = "lenient::de")]
    pub region: Option<Region>,
    #[serde(deserialize_with = "lenient::string")]
    pub date: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub asof: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub sym: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub pair: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub version: Option<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub factors: Vec<ScaleFactor>,
    #[serde(deserialize_with = "lenient::de")]
    pub result: ScaleResult,
    /// Needle after the discussion: base factors + every argument. `None` = no discussion.
    #[serde(deserialize_with = "lenient::de")]
    pub after: Option<ScaleResult>,
    #[serde(deserialize_with = "lenient::de")]
    pub light: ScaleLight,
    /// `Record<string, number | null>`, in file order.
    #[serde(deserialize_with = "levels_map")]
    pub levels: Vec<(String, Option<f64>)>,
    #[serde(deserialize_with = "lenient::de")]
    pub plan: Option<ScalePlan>,
    #[serde(deserialize_with = "lenient::vec")]
    pub discussion: Vec<DiscussionItem>,
    #[serde(deserialize_with = "lenient::vec")]
    pub history: Vec<ScaleHistory>,
}

fn levels_map<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(String, Option<f64>)>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => m.into_iter().map(|(k, v)| (k, v.as_f64())).collect(),
        _ => Vec::new(),
    })
}

/// `ScaleState`, by its TS name.
pub type ScaleState = ScaleFile;

// ---------------------------------------------------------------- macro_<D>_<region>[_verdict].json

/// `DayShape`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct DayShape {
    #[serde(deserialize_with = "lenient::string")]
    pub forecast: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub prob_two_sided: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub summary: Option<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub windows: Vec<DayShapeWindow>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct DayShapeWindow {
    #[serde(deserialize_with = "lenient::string")]
    pub window: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub expect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct VerdictSource {
    #[serde(deserialize_with = "lenient::string")]
    pub title: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub url: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ScaleJudgment {
    #[serde(deserialize_with = "lenient::side")]
    pub side: Option<Side>,
    #[serde(deserialize_with = "lenient::de")]
    pub strength: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub text: Option<String>,
}

/// `MacroVerdict`: `state/macro_<D>_<region>_verdict.json`, the agent's part.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct MacroVerdict {
    /// 'LONG' | 'SHORT' | 'NEUTRAL' | 'NO_TRADE' | …
    #[serde(deserialize_with = "lenient::string")]
    pub verdict: Option<String>,
    /// 'low' | 'medium' | 'high' | …
    #[serde(deserialize_with = "lenient::string")]
    pub confidence: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub summary: Option<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub reasons: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub risks: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub watch: Vec<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub day_shape: Option<DayShape>,
    #[serde(deserialize_with = "lenient::vec")]
    pub sources: Vec<VerdictSource>,
    #[serde(deserialize_with = "lenient::vec")]
    pub plan: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub scale_judgment: Vec<ScaleJudgment>,
}

/// `CalendarEvent`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct CalendarEvent {
    #[serde(deserialize_with = "lenient::string")]
    pub title: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub impact: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub time_eet: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub currency: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub forecast: Option<NumOrStr>,
    #[serde(deserialize_with = "lenient::de")]
    pub previous: Option<NumOrStr>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Calendar {
    #[serde(deserialize_with = "lenient::string")]
    pub status: Option<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub reasons: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub flags: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Market {
    #[serde(deserialize_with = "lenient::string")]
    pub label: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub chg_pct: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub main: bool,
    #[serde(deserialize_with = "lenient::string")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Headlines {
    #[serde(deserialize_with = "lenient::de")]
    pub count: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub relevant: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub tone_sum: Option<f64>,
    /// Tag → count, in file order; non-numbers dropped.
    #[serde(deserialize_with = "num_map")]
    pub tags: Vec<(String, f64)>,
}

fn num_map<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<(String, f64)>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => m
            .into_iter()
            .filter_map(|(k, v)| v.as_f64().map(|f| (k, f)))
            .collect(),
        _ => Vec::new(),
    })
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Tilt {
    #[serde(deserialize_with = "lenient::de")]
    pub score: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub hint: Option<String>,
}

/// `MacroCollect`: the part of `state/macro_<D>_<region>.json` the desk shows
/// (built by [`crate::normalize::trim_collect`]; headlines cut to their summary).
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct MacroFile {
    #[serde(deserialize_with = "lenient::string")]
    pub asof_eet: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub open_eet: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub calendar: Option<Calendar>,
    #[serde(deserialize_with = "lenient::vec")]
    pub markets: Vec<Market>,
    #[serde(deserialize_with = "lenient::de")]
    pub headlines: Option<Headlines>,
    #[serde(deserialize_with = "lenient::de")]
    pub tilt: Option<Tilt>,
    /// Shape not fixed by the contract (`unknown` in TS).
    pub regime: Value,
    pub prev_day_check: Vec<Value>,
    #[serde(deserialize_with = "lenient::de")]
    pub previous_verdict: Option<MacroVerdict>,
}

/// `MacroCollect`, by its TS name.
pub type MacroCollect = MacroFile;

// ---------------------------------------------------------------- review_<D>.json, review_<D>_agent.json

/// `ReviewFactorRow`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFactorRow {
    #[serde(deserialize_with = "lenient::string")]
    pub factor: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub group: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub share: Option<NumOrStr>,
    #[serde(deserialize_with = "lenient::string")]
    pub side: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub strength: Option<NumOrStr>,
    #[serde(deserialize_with = "lenient::string")]
    pub dir: Option<String>,
    /// '1' | '0' | '' | …
    #[serde(deserialize_with = "lenient::string")]
    pub hit: Option<String>,
}

/// `ReviewRegion.facts`: what the window actually did. Unknown keys stay in the raw file.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFacts {
    #[serde(deserialize_with = "lenient::string")]
    pub dir: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub chg_adr: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub first_break: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub ib_adr: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub partial: Option<bool>,
}

/// `ReviewRegion.score`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewScore {
    #[serde(deserialize_with = "lenient::string")]
    pub needle_hit: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub after_hit: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub plan_hit: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub manip_swept: Option<bool>,
}

/// `ReviewRegion.human`: `scale.py review` copies the owner's read here and scores it.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewHuman {
    #[serde(deserialize_with = "lenient::string")]
    pub side: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub at: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub hit: Option<String>,
}

/// `ReviewRegion`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewRegion {
    #[serde(deserialize_with = "lenient::string")]
    pub missing: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub facts: Option<ReviewFacts>,
    #[serde(deserialize_with = "lenient::de")]
    pub score: Option<ReviewScore>,
    #[serde(deserialize_with = "lenient::vec")]
    pub factors: Vec<ReviewFactorRow>,
    #[serde(deserialize_with = "lenient::vec")]
    pub discussion: Vec<DiscussionItem>,
    #[serde(deserialize_with = "lenient::de")]
    pub needle: Option<f64>,
    #[serde(deserialize_with = "lenient::string")]
    pub zone: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub after: Option<f64>,
    #[serde(deserialize_with = "lenient::de")]
    pub light: Option<ScaleLight>,
    #[serde(deserialize_with = "lenient::de")]
    pub macro_verdict: Option<MacroVerdict>,
    #[serde(deserialize_with = "lenient::de")]
    pub human: Option<ReviewHuman>,
}

/// `Partial<Record<Region, T>>`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, bound(deserialize = "T: DeserializeOwned"))]
pub struct PerRegion<T> {
    #[serde(deserialize_with = "lenient::de")]
    pub eu: Option<T>,
    #[serde(deserialize_with = "lenient::de")]
    pub us: Option<T>,
}

impl<T> Default for PerRegion<T> {
    fn default() -> Self {
        PerRegion { eu: None, us: None }
    }
}

impl<T> PerRegion<T> {
    pub fn get(&self, r: Region) -> Option<&T> {
        match r {
            Region::Eu => self.eu.as_ref(),
            Region::Us => self.us.as_ref(),
        }
    }
}

/// `ReviewMech`: `state/review_<D>.json`, the mechanical evening review.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewFile {
    #[serde(deserialize_with = "lenient::string")]
    pub date: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub asof: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub regions: PerRegion<ReviewRegion>,
    /// Observations about the trader himself, when the skill has weeks enough to make them.
    #[serde(deserialize_with = "lenient::vec")]
    pub trader: Vec<String>,
}

/// `ReviewMech`, by its TS name.
pub type ReviewMech = ReviewFile;

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewAgentRegion {
    #[serde(deserialize_with = "lenient::string")]
    pub what_worked: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub what_failed: Option<String>,
    #[serde(deserialize_with = "lenient::string")]
    pub discussion: Option<String>,
}

/// `ReviewAgent`: `state/review_<D>_agent.json`, the agent's evening words.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct ReviewAgent {
    #[serde(deserialize_with = "lenient::string")]
    pub summary: Option<String>,
    #[serde(deserialize_with = "lenient::de")]
    pub regions: PerRegion<ReviewAgentRegion>,
    #[serde(deserialize_with = "lenient::vec")]
    pub lessons: Vec<String>,
    #[serde(deserialize_with = "lenient::vec")]
    pub weight_ideas: Vec<String>,
}

// ---------------------------------------------------------------- shots_<D>_<region>[_review].json

/// `ShotLegend`: one line drawn on a screenshot, as the legend under the picture names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShotLegend {
    pub key: String,
    /// Plain words, written by the skill («манипуляция: открытие дня»).
    pub label: String,
    /// `ShotKind` (manip / target / level / open / zone / fact) or anything else.
    pub kind: String,
    pub color: String,
    /// 'solid' | 'dashed'; absent in older files (then «open» is dashed, the rest solid).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub style: Option<String>,
}

/// `MacroShot`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroShot {
    pub name: String,
    /// Timeframe as TradingView names it: M15, H1.
    pub tf: String,
    pub title: String,
    /// Vault-relative path of the PNG (forward slashes); the file may be missing.
    pub path: String,
    /// False when the skill's drawing check found a mismatch; `None` = not checked.
    pub verify_ok: Option<bool>,
    pub legend: Vec<ShotLegend>,
}

/// `state/shots_<D>_<region>.json` (`MacroShots`): the review's levels drawn on
/// TradingView and screenshotted. `ok: false` means no pictures, and `error`
/// says why («dry-run», «TradingView не запущен», «нет шкалы с планом: …»).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shots {
    pub date: String,
    pub region: Region,
    pub asof: String,
    pub ok: bool,
    pub error: Option<String>,
    pub shots: Vec<MacroShot>,
}

/// `MacroShots`, by its TS name.
pub type MacroShots = Shots;

/// What a screenshot is in Rust where the TS read it into a base64 `data:`
/// URL (`readShot`): its vault-relative path and size. The bytes are decoded
/// into a texture by the UI, off the frame (TZ: «Снимки PNG читаются в base64
/// и передаются строкой через IPC» is one of the slow paths we drop).
/// Serializes as `{"$shot": "<relative path>", "bytes": N}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShotRef {
    #[serde(rename = "$shot")]
    pub path: String,
    pub bytes: u64,
}

// ---------------------------------------------------------------- the owner's two files

/// `state/human_read_<D>_<region>.json`: what the owner expects of the
/// window, said before (or without) seeing the scale.
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

/// `DayAnswerRegion`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayAnswerRegion {
    pub trade: TradeTaken,
    /// Only meaningful when a trade was taken.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub by_rule: Option<ByRule>,
}

/// `Partial<Record<Region, DayAnswerRegion>>`; serializes only the regions present.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DayAnswerRegions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub eu: Option<DayAnswerRegion>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub us: Option<DayAnswerRegion>,
}

impl DayAnswerRegions {
    pub fn get(&self, r: Region) -> Option<&DayAnswerRegion> {
        match r {
            Region::Eu => self.eu.as_ref(),
            Region::Us => self.us.as_ref(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.eu.is_none() && self.us.is_none()
    }
}

/// `state/day_answers_<D>.json`: the two evening questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayAnswers {
    pub date: String,
    pub at: String,
    pub regions: DayAnswerRegions,
    /// «Что помогло или помешало» — one line, free text.
    pub note: String,
}

// ---------------------------------------------------------------- the memory cards (note 23 §2–3)

/// `RememberKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RememberKind {
    Lesson,
    Rule,
    Similar,
}

impl RememberKind {
    pub fn parse(s: &str) -> Option<RememberKind> {
        match s {
            "lesson" => Some(RememberKind::Lesson),
            "rule" => Some(RememberKind::Rule),
            "similar" => Some(RememberKind::Similar),
            _ => None,
        }
    }
}

/// One line of «Сегодня помни»: a lesson whose tags match today, a rule that is on today, or a similar day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RememberItem {
    pub kind: RememberKind,
    pub text: String,
    /// Why this line today («пятница в Европе», «те же теги: пила, ночь ≥ 0.8 ADR»).
    pub why: String,
    /// Where it comes from («lessons.md 08.10», «заметка 20, п. 7», «2026-10-08 eu»).
    #[serde(rename = "ref")]
    pub ref_: String,
    pub tags: Vec<String>,
}

/// `Remember.conditions`. Optional strings are left out when absent, as in the TS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RememberConditions {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub weekday: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub night: Option<String>,
    #[serde(serialize_with = "js::ser_opt_f64")]
    pub night_adr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub light: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub earnings_giant_prev: Option<bool>,
    pub tags: Vec<String>,
}

/// `Remember.similar`: the nearest day by factor vector, and how it ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RememberSimilar {
    pub date: String,
    pub region: String,
    #[serde(serialize_with = "js::ser_opt_f64")]
    pub distance: Option<f64>,
    #[serde(serialize_with = "js::ser_opt_f64")]
    pub needle: Option<f64>,
    pub dir: String,
    #[serde(serialize_with = "js::ser_opt_f64")]
    pub chg_adr: Option<f64>,
    pub human_side: String,
}

/// `state/remember_<D>_<region>.json` → «Сегодня помни».
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Remember {
    pub date: String,
    pub region: Region,
    pub asof: String,
    /// At most three ([`crate::normalize::REMEMBER_MAX`]); the skill chose them.
    pub items: Vec<RememberItem>,
    pub conditions: Option<RememberConditions>,
    pub similar: Option<RememberSimilar>,
    pub lesson_candidates: u64,
    pub enough_history: bool,
}

/// `DriftSignal`: one thing that changed («фаза сменилась на тренд», «фактор X три дня подряд мимо»).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DriftSignal {
    /// 'phase' | 'adr' | 'factor' | 'needle' | …
    pub kind: String,
    pub text: String,
    /// 'info' | 'warn'.
    pub severity: String,
    pub data: Map<String, Value>,
}

/// `state/drift_<D>_<region>.json` → «Что изменилось» (drawn only with `signals`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drift {
    pub date: String,
    pub region: Region,
    pub days: u64,
    pub enough_data: bool,
    pub signals: Vec<DriftSignal>,
}

/// `WeekSummary.share_proposal`: one sentence; the decision is the owner's, in chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShareProposal {
    pub factor: String,
    pub from: Option<NumOrStr>,
    pub to: Option<NumOrStr>,
    pub why: String,
}

/// `state/week_<D>.json` (D = the Friday), `WeekSummary` of macro.ts: the week in five lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Week {
    pub week_start: String,
    pub week_end: String,
    pub days: u64,
    pub enough_data: bool,
    /// Five lines: the market, what worked, the lesson that repeated, one idea to check, one share proposal.
    pub lines: Vec<String>,
    pub stats: Map<String, Value>,
    /// Observations about the trader himself («4 стопа из 5 — вход до закрытия IB»).
    pub trader: Vec<String>,
    pub share_proposal: Option<ShareProposal>,
}

/// `WeekSummary`, by its TS name.
pub type WeekSummary = Week;

/// `IdeaStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeaStatus {
    Open,
    Proposed,
    Rule,
    Dropped,
}

impl IdeaStatus {
    pub fn parse(s: &str) -> Option<IdeaStatus> {
        match s {
            "open" => Some(IdeaStatus::Open),
            "proposed" => Some(IdeaStatus::Proposed),
            "rule" => Some(IdeaStatus::Rule),
            "dropped" => Some(IdeaStatus::Dropped),
            _ => None,
        }
    }
}

/// `Idea.backtest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdeaBacktest {
    pub days: u64,
    pub hit: u64,
    #[serde(serialize_with = "js::ser_opt_f64")]
    pub rate: Option<f64>,
}

/// One idea the owner voiced, with its running score from the evening checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub date: String,
    pub text: String,
    pub side: String,
    pub region: String,
    pub source: String,
    pub confirmed: u64,
    pub refuted: u64,
    pub status: IdeaStatus,
    pub backtest: Option<IdeaBacktest>,
    pub last_check: String,
}

/// `state/ideas.json` — no date: the one list, as it stands now.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ideas {
    pub asof: String,
    pub items: Vec<Idea>,
}

/// One row of [`crate::stats::factor_stats`]: hit rate of a factor in a region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactorStat {
    pub region: Region,
    /// `None` only when the CSV has no `factor` column (TS: `undefined`, left out).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub factor: Option<String>,
    pub hits: u64,
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn raw_keeps_the_file_and_reads_leniently() {
        let v = json!({"verdict": "SHORT", "confidence": 7, "reasons": ["a", 3, "b"], "extra": {"x": 1}});
        let r: Raw<MacroVerdict> = Raw::new(v.clone());
        assert_eq!(r.verdict.as_deref(), Some("SHORT"));
        assert_eq!(r.confidence.as_deref(), Some("7"));
        assert_eq!(r.reasons, ["a", "b"]);
        assert_eq!(serde_json::to_value(&r).unwrap(), v);
        // Not an object at all: the default view, the raw value kept.
        let r: Raw<MacroVerdict> = Raw::new(json!(5));
        assert_eq!(r.get(), &MacroVerdict::default());
        assert_eq!(serde_json::to_value(&r).unwrap(), json!(5));
        assert!(!Raw::<MacroVerdict>::new(json!(false)).is_truthy());
    }

    #[test]
    fn scale_view_survives_wrong_types() {
        let v = json!({
            "factors": [{"id": "a", "side": "UP", "share": "x"}, null, 7],
            "result": {"needle": -58, "groups": {"g": {"share": 10}, "bad": 3}},
            "after": "nope",
            "levels": {"price": 25100, "adr": null}
        });
        let s: Raw<ScaleFile> = Raw::new(v);
        assert_eq!(s.factors.len(), 1);
        assert_eq!(s.factors[0].side, None);
        assert_eq!(s.factors[0].share, None);
        assert_eq!(s.result.needle(), -58.0);
        let groups = s.result.groups.as_ref().unwrap();
        assert_eq!(groups.0.len(), 1);
        assert_eq!(groups.get("g").and_then(|g| g.share), Some(10.0));
        assert_eq!(s.after, None);
        assert_eq!(
            s.levels,
            vec![("price".into(), Some(25100.0)), ("adr".into(), None)]
        );
    }

    #[test]
    fn shot_ref_serializes_as_the_golden_placeholder() {
        let r = ShotRef {
            path: "indices-kb/shots/a/b/m15.png".into(),
            bytes: 12,
        };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"$shot":"indices-kb/shots/a/b/m15.png","bytes":12}"#
        );
    }

    #[test]
    fn enums_spell_like_the_files() {
        assert_eq!(
            serde_json::to_value(HumanSide::NoTrade).unwrap(),
            json!("NO_TRADE")
        );
        assert_eq!(serde_json::to_value(Region::Us).unwrap(), json!("us"));
        assert_eq!(HumanSide::parse("NEUTRAL"), Some(HumanSide::Neutral));
        assert_eq!(HumanSide::parse("UP"), None);
    }
}
