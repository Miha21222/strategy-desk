//! The perf report: `docs/rust/perf/<date>-<commit>.json`, schema `desk-perf/1`.
//!
//! docs/rust/PERF.md is the human description and
//! docs/rust/perf/report.schema.json the machine one; bench.ps1 writes the
//! same keys by hand and report.py reads them. Change all four together.

use crate::stats::{self, round3};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "desk-perf/1";

/// How a metric's samples are compared with its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stat {
    Median,
    P95,
    P99,
    Max,
    Mean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    /// Not measured in this run; `note` says why.
    Skip,
    /// Measured, but has no target or is not comparable here (a stub binary).
    Info,
}

/// One row of the TEAM.md «Метрики» table (plus the stress rows).
#[derive(Debug, Clone, Copy)]
pub struct MetricDef {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub target: Option<f64>,
    pub stat: Stat,
}

/// The catalog. Ids are stable; labels are for people.
pub const METRICS: &[MetricDef] = &[
    MetricDef {
        id: "cold_start_ms",
        label: "Холодный старт до первого кадра",
        unit: "ms",
        target: Some(300.0),
        stat: Stat::Median,
    },
    // 26.11 (TEAM.md): ~132 ms of a warm start is the window + OpenGL context
    // in the driver, before our code runs; 150 was not reachable by us.
    MetricDef {
        id: "warm_start_ms",
        label: "Тёплый старт",
        unit: "ms",
        target: Some(200.0),
        stat: Stat::Median,
    },
    // Start → the day's data drawn (`--bench-data-ready`): what the owner
    // feels as «долгая загрузка», as opposed to the first (possibly empty) frame.
    MetricDef {
        id: "data_ready_cold_ms",
        label: "Данные дня на экране, холодный старт",
        unit: "ms",
        target: Some(500.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "data_ready_warm_ms",
        label: "Данные дня на экране, тёплый старт",
        unit: "ms",
        target: Some(300.0),
        stat: Stat::Median,
    },
    // 26.11 (TEAM.md): ~81 MB Private is the OpenGL driver before our code;
    // measured with a chart screenshot on screen.
    MetricDef {
        id: "idle_working_set_mb",
        label: "RAM в покое (Working Set)",
        unit: "MB",
        target: Some(100.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "idle_private_mb",
        label: "RAM в покое (Private Bytes)",
        unit: "MB",
        target: Some(100.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "idle_cpu_pct",
        label: "CPU в покое",
        unit: "%",
        target: Some(1.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "idle_frames_per_min",
        label: "Кадров в минуту в покое",
        unit: "1/min",
        target: None,
        stat: Stat::Median,
    },
    MetricDef {
        id: "frame_p50_ms",
        label: "Кадр p50 при стриме",
        unit: "ms",
        target: Some(8.0),
        stat: Stat::Median,
    },
    // Samples are per-10-s windows of DESK_FRAME_STATS; p95/p99 are judged
    // on the worst window, so one janky stretch is not averaged away.
    MetricDef {
        id: "frame_p95_ms",
        label: "Кадр p95 при стриме",
        unit: "ms",
        target: Some(16.0),
        stat: Stat::Max,
    },
    MetricDef {
        id: "frame_p99_ms",
        label: "Кадр p99 при стриме",
        unit: "ms",
        target: Some(33.0),
        stat: Stat::Max,
    },
    // How much the stream's p50 frame grows per minute of answer (weighted
    // slope over the windows): no target, a finding to track — 27.11: ~5 ms/min.
    MetricDef {
        id: "frame_p50_growth_ms_per_min",
        label: "Рост p50 кадра за минуту стрима",
        unit: "ms/min",
        target: None,
        stat: Stat::Max,
    },
    // Same per-window rule, while `--bench-switch` flips days as fast as it
    // can: the stand-in for scrolling, a different load than the chat stream.
    MetricDef {
        id: "switch_frame_p50_ms",
        label: "Кадр p50 при переключении дней",
        unit: "ms",
        target: Some(8.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "switch_frame_p95_ms",
        label: "Кадр p95 при переключении дней",
        unit: "ms",
        target: Some(16.0),
        stat: Stat::Max,
    },
    MetricDef {
        id: "switch_frame_p99_ms",
        label: "Кадр p99 при переключении дней",
        unit: "ms",
        target: Some(33.0),
        stat: Stat::Max,
    },
    MetricDef {
        id: "day_load_ms",
        label: "Загрузка дня (~22 файла)",
        unit: "ms",
        target: Some(30.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "day_load_real_size_ms",
        label: "Загрузка дня, файлы реального размера",
        unit: "ms",
        target: Some(30.0),
        stat: Stat::Median,
    },
    MetricDef {
        id: "vault_x10_parse_ms",
        label: "Разбор всего vault ×10",
        unit: "ms",
        target: None,
        stat: Stat::Median,
    },
    MetricDef {
        id: "vault_x10_list_days_ms",
        label: "Список дней vault ×10",
        unit: "ms",
        target: None,
        stat: Stat::Median,
    },
    MetricDef {
        id: "stream_parse_10k_ms",
        label: "Разбор 10 тыс. событий stream-json",
        unit: "ms",
        target: None,
        stat: Stat::Median,
    },
    MetricDef {
        id: "stream_pipe_10k_ms",
        label: "10 тыс. событий через трубу fake-claude",
        unit: "ms",
        target: None,
        stat: Stat::Median,
    },
    MetricDef {
        id: "watch_latency_ms",
        label: "Перерисовка после записи в state/",
        unit: "ms",
        target: Some(350.0),
        stat: Stat::P95,
    },
    MetricDef {
        id: "switch_ms",
        label: "Переключение экрана / дня",
        unit: "ms",
        target: Some(100.0),
        stat: Stat::P95,
    },
    MetricDef {
        id: "exe_mb",
        label: "Размер exe",
        unit: "MB",
        target: Some(15.0),
        stat: Stat::Max,
    },
    MetricDef {
        id: "install_dir_mb",
        label: "Размер установленной папки",
        unit: "MB",
        target: None,
        stat: Stat::Max,
    },
    MetricDef {
        id: "installer_mb",
        label: "Размер установщика",
        unit: "MB",
        target: Some(20.0),
        stat: Stat::Max,
    },
    MetricDef {
        id: "soak_private_growth_pct",
        label: "Рост Private Bytes за сутки",
        unit: "%",
        target: Some(5.0),
        stat: Stat::Max,
    },
];

pub fn def(id: &str) -> Option<&'static MetricDef> {
    METRICS.iter().find(|m| m.id == id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub samples: Vec<f64>,
    pub median: Option<f64>,
    pub p95: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// Lower is better for every metric here; `value <= target` passes.
    pub target: Option<f64>,
    pub stat: Stat,
    /// The number compared with the target (`stat` of `samples`).
    pub value: Option<f64>,
    pub status: Status,
    pub method: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Secondary series of the same runs (e.g. `window_ms`, `self_reported_ms`).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Metric {
    /// From samples, with status computed from the catalog target.
    pub fn measured(id: &str, samples: Vec<f64>, method: &str) -> Metric {
        let d = def(id);
        let stat = d.map(|d| d.stat).unwrap_or(Stat::Median);
        let target = d.and_then(|d| d.target);
        let value = match stat {
            Stat::Median => stats::median(&samples),
            Stat::P95 => stats::percentile(&samples, 95.0),
            Stat::P99 => stats::percentile(&samples, 99.0),
            Stat::Max => stats::percentile(&samples, 100.0),
            Stat::Mean => stats::mean(&samples),
        };
        let status = match (value, target) {
            (Some(v), Some(t)) if v <= t => Status::Pass,
            (Some(_), Some(_)) => Status::Fail,
            _ => Status::Info,
        };
        Metric {
            id: id.into(),
            label: d.map(|d| d.label).unwrap_or(id).into(),
            unit: d.map(|d| d.unit).unwrap_or("").into(),
            median: stats::median(&samples).map(round3),
            p95: stats::percentile(&samples, 95.0).map(round3),
            min: samples.iter().copied().reduce(f64::min).map(round3),
            max: stats::percentile(&samples, 100.0).map(round3),
            samples: samples.into_iter().map(round3).collect(),
            target,
            stat,
            value: value.map(round3),
            status,
            method: method.into(),
            note: String::new(),
            extra: serde_json::Map::new(),
        }
    }

    pub fn skipped(id: &str, why: &str) -> Metric {
        let mut m = Metric::measured(id, Vec::new(), "");
        m.status = Status::Skip;
        m.note = why.into();
        m
    }

    /// Keep the numbers but do not judge them (stub binary, wrong machine).
    pub fn info(mut self, why: &str) -> Metric {
        self.status = Status::Info;
        self.note = why.into();
        self
    }

    pub fn with_note(mut self, note: &str) -> Metric {
        self.note = note.into();
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Machine {
    pub os: String,
    pub cpu: String,
    pub logical_cpus: usize,
    pub ram_gb: f64,
    /// "ac" | "battery" | "unknown"
    pub power: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub power_plan: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub noise: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Build {
    /// "rust" | "electron" | "synthetic"
    pub kind: String,
    pub version: String,
    pub commit: String,
    /// File name only: a full path would carry the user's account name.
    pub exe: String,
    pub profile: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Harness {
    pub script: String,
    pub runs: usize,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cold_method: String,
    pub idle_seconds: u64,
    pub settle_seconds: u64,
    /// Where the vault of the run came from ("-Vault", "env DESK_VAULT",
    /// "electron settings", "none"), never the path itself.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vault: String,
}

/// One column of an A/B run (`bench.ps1 -Variants`): the same start metrics
/// under other environment switches or another exe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub env: serde_json::Map<String, serde_json::Value>,
    /// File name only, like `Build::exe`.
    pub exe: String,
    /// The start protocol the build spoke: "data-ready" | "first-frame" | "window".
    #[serde(default)]
    pub mode: String,
    /// Matrix runs (`-Exes` × `-Opts`): which build and which `DESK_OPT`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub build: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub opt: String,
    pub metrics: Vec<Metric>,
    /// `DESK_STARTUP_TRACE` medians: run kind → phase → {n, ms, private_mb, ws_mb}.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub phases: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema: String,
    /// false = numbers from a box that is not the owner's laptop.
    pub representative: bool,
    pub label: String,
    pub created_at: String,
    pub date: String,
    pub machine: Machine,
    pub build: Build,
    pub harness: Harness,
    /// For a variant run: the first variant's numbers (the reference column).
    pub metrics: Vec<Metric>,
    /// Every variant, the first one included; empty without `-Variants`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<Variant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Report {
    pub fn summary_counts(&self) -> (usize, usize, usize) {
        let c = |s: Status| self.metrics.iter().filter(|m| m.status == s).count();
        (c(Status::Pass), c(Status::Fail), c(Status::Skip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_unique_and_targets_from_team_md() {
        let mut ids: Vec<&str> = METRICS.iter().map(|m| m.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), METRICS.len());
        assert_eq!(def("cold_start_ms").and_then(|d| d.target), Some(300.0));
        assert_eq!(def("warm_start_ms").and_then(|d| d.target), Some(200.0));
        assert_eq!(def("idle_private_mb").and_then(|d| d.target), Some(100.0));
        assert_eq!(
            def("idle_working_set_mb").and_then(|d| d.target),
            Some(100.0)
        );
        assert_eq!(def("frame_p99_ms").and_then(|d| d.target), Some(33.0));
        assert_eq!(def("installer_mb").and_then(|d| d.target), Some(20.0));
        assert_eq!(
            def("data_ready_cold_ms").and_then(|d| d.target),
            Some(500.0)
        );
        assert_eq!(
            def("data_ready_warm_ms").and_then(|d| d.target),
            Some(300.0)
        );
    }

    #[test]
    fn pass_fail_by_stat() {
        let m = Metric::measured("warm_start_ms", vec![100.0, 120.0, 400.0], "t");
        assert_eq!(m.value, Some(120.0));
        assert_eq!(m.status, Status::Pass);
        let m = Metric::measured("watch_latency_ms", vec![100.0, 120.0, 400.0], "t");
        assert_eq!(m.value, Some(400.0));
        assert_eq!(m.status, Status::Fail);
        let m = Metric::measured("vault_x10_parse_ms", vec![1.0], "t");
        assert_eq!(m.status, Status::Info);
        assert_eq!(Metric::skipped("switch_ms", "no hook").status, Status::Skip);
    }

    #[test]
    fn serializes_schema_keys() {
        let r = Report {
            schema: SCHEMA.into(),
            representative: false,
            label: "t".into(),
            created_at: "2026-11-25T00:00:00Z".into(),
            date: "2026-11-25".into(),
            machine: Machine::default(),
            build: Build::default(),
            harness: Harness::default(),
            metrics: vec![Metric::measured("exe_mb", vec![3.0], "size")],
            variants: vec![Variant {
                id: "v1".into(),
                label: "base".into(),
                env: serde_json::Map::new(),
                exe: "strategy-desk.exe".into(),
                mode: "data-ready".into(),
                build: "glow".into(),
                opt: "default".into(),
                metrics: vec![Metric::measured("data_ready_warm_ms", vec![250.0], "t")],
                phases: serde_json::json!({"warm": {"day_presented": {"n": 5, "ms": 240.0}}}),
            }],
            notes: vec![],
        };
        let v = serde_json::to_value(&r).unwrap_or_default();
        for k in [
            "schema",
            "representative",
            "machine",
            "build",
            "harness",
            "metrics",
        ] {
            assert!(v.get(k).is_some(), "{k}");
        }
        assert_eq!(v["metrics"][0]["status"], "pass");
        assert_eq!(v["metrics"][0]["stat"], "max");
        assert_eq!(v["variants"][0]["metrics"][0]["status"], "pass");
        let back: Report = serde_json::from_value(v).expect("round trip");
        assert_eq!(back.summary_counts(), (1, 0, 0));
    }
}
