//! Who ran the numbers. Never the host name or user name: a report goes into
//! git, and the machine is identified well enough by CPU + RAM + OS.

use crate::report::Machine;
use std::fs;

pub fn detect() -> Machine {
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let mut m = Machine {
        logical_cpus,
        power: "unknown".into(),
        ..Machine::default()
    };
    if cfg!(target_os = "linux") {
        m.os = fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("PRETTY_NAME=")
                        .map(|v| v.trim_matches('"').to_string())
                })
            })
            .unwrap_or_else(|| "Linux".into());
        if let Ok(k) = fs::read_to_string("/proc/sys/kernel/osrelease") {
            m.os = format!("{} (kernel {})", m.os, k.trim());
        }
        m.cpu = fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("model name")
                        .map(|v| v.trim_start_matches([' ', '\t', ':']).to_string())
                })
            })
            .unwrap_or_default();
        m.ram_gb = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|t| {
                t.lines().find_map(|l| {
                    l.strip_prefix("MemTotal:")
                        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<f64>().ok())
                })
            })
            .map(|kb| (kb / 1024.0 / 1024.0 * 10.0).round() / 10.0)
            .unwrap_or(0.0);
        if std::path::Path::new("/.dockerenv").exists()
            || fs::read_to_string("/proc/1/cgroup")
                .map(|c| c.contains("docker") || c.contains("kubepods"))
                .unwrap_or(false)
        {
            m.noise
                .push("container: shared cloud CPU, other tenants".into());
        }
    } else {
        m.os = std::env::consts::OS.into();
        m.cpu = std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default();
    }
    m
}
