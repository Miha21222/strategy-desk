//! The three Task Scheduler jobs (TZ item 18): each runs
//! `strategy-desk.exe run --window <eu|us|review>` on weekdays at every
//! minute `planned_runs` can give for its window (two for the US window: the
//! weeks Europe and America change clocks on different days). The run itself
//! decides with `due_window` whether the minute is its own.
//!
//! Unlike the Electron app, nothing is written into the user's environment:
//! `run` reads the settings and the sessionid (Credential Manager) itself.
//! Registering also REMOVES the cookie the old app left in HKCU.
//!
//! Everything here builds text; the scripts run through the task runner
//! (Windows only). The builders are what the tests pin down.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use chrono::{Days, NaiveDate, NaiveTime};
use desk_core::windows::WindowId;

use crate::core_wip::{planned_runs, Settings};
use crate::runtime::SpawnSpec;

/// The job names the old app registered too, so registering replaces them
/// instead of leaving a second set that runs the old scripts.
pub const TASKS: [(WindowId, &str); 3] = [
    (WindowId::Eu, "Trading Macro EU"),
    (WindowId::Us, "Trading Macro US"),
    (WindowId::Review, "Trading Review"),
];

/// User variables the old app wrote for its scheduled scripts.
pub const OLD_USER_VARS: [&str; 6] = [
    "MACRO_VAULT",
    "MACRO_PYTHON",
    "MACRO_CLAUDE",
    "TV_CDP_URL",
    "PYTHONIOENCODING",
    "TV_SESSIONID",
];

pub fn task_name(w: WindowId) -> &'static str {
    TASKS
        .iter()
        .find(|(id, _)| *id == w)
        .map_or("Trading Macro", |(_, n)| n)
}

/// Every local minute a window may run at over a year from `from` — the
/// union of `planned_runs`, because a trigger is a fixed weekly time.
pub fn trigger_times(from: NaiveDate, s: &Settings) -> HashMap<WindowId, BTreeSet<NaiveTime>> {
    let mut out: HashMap<WindowId, BTreeSet<NaiveTime>> = HashMap::new();
    for d in 0..366u64 {
        let Some(date) = from.checked_add_days(Days::new(d)) else {
            break;
        };
        for r in planned_runs(date, s) {
            out.entry(r.window).or_default().insert(r.at_local);
        }
    }
    out
}

/// PowerShell single-quoted string.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The registration script (shown to the owner before it runs).
pub fn register_script(exe: &Path, times: &HashMap<WindowId, BTreeSet<NaiveTime>>) -> String {
    let mut lines = vec![
        "$ErrorActionPreference = 'Stop'".to_string(),
        // Late (asleep at the minute) still starts; `run` then finds it is not
        // its minute any more and exits without work.
        "$s = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 2)".to_string(),
    ];
    for (w, name) in TASKS {
        let Some(at) = times.get(&w).filter(|t| !t.is_empty()) else {
            continue;
        };
        let triggers: Vec<String> = at
            .iter()
            .map(|t| {
                format!(
                    "(New-ScheduledTaskTrigger -Weekly -DaysOfWeek Monday,Tuesday,Wednesday,Thursday,Friday -At {})",
                    ps_quote(&t.format("%H:%M").to_string())
                )
            })
            .collect();
        lines.push(format!(
            "$a = New-ScheduledTaskAction -Execute {} -Argument {}",
            ps_quote(&exe.to_string_lossy()),
            ps_quote(&format!("run --window {}", w.as_str()))
        ));
        lines.push(format!("$t = @({})", triggers.join(", ")));
        lines.push(format!(
            "Register-ScheduledTask -TaskName {} -Action $a -Trigger $t -Settings $s -Description {} -Force | Out-Null",
            ps_quote(name),
            ps_quote(&format!("Strategy Desk: macro review, window {}", w.as_str()))
        ));
    }
    // The old app kept the cookie in HKCU for its scheduled scripts; gone.
    lines.push("[Environment]::SetEnvironmentVariable('TV_SESSIONID', $null, 'User')".into());
    lines.push("'registered'".into());
    lines.join("\n")
}

pub fn unregister_script() -> String {
    let mut lines: Vec<String> = TASKS
        .iter()
        .map(|(_, n)| format!("Unregister-ScheduledTask -TaskName {} -Confirm:$false -ErrorAction SilentlyContinue", ps_quote(n)))
        .collect();
    lines.extend(OLD_USER_VARS.iter().map(|v| {
        format!(
            "[Environment]::SetEnvironmentVariable({}, $null, 'User')",
            ps_quote(v)
        )
    }));
    lines.push("'unregistered'".into());
    lines.join("\n")
}

/// Read-only: the three jobs, one `name|state|action` line each.
pub fn query_script() -> String {
    let names: Vec<String> = TASKS.iter().map(|(_, n)| ps_quote(n)).collect();
    format!(
        "Get-ScheduledTask -TaskName {} -ErrorAction SilentlyContinue | ForEach-Object {{ $_.TaskName + '|' + $_.State + '|' + (($_.Actions | ForEach-Object {{ $_.Execute + ' ' + $_.Arguments }}) -join ';') }}",
        names.join(",")
    )
}

/// One registered job as `Get-ScheduledTask` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInfo {
    pub name: String,
    pub state: String,
    pub action: String,
}

impl JobInfo {
    /// Registered by an Electron desk (runs the vault's scripts), not by us.
    pub fn is_old(&self) -> bool {
        !self.action.contains("run --window")
    }
}

pub fn parse_query(out: &str) -> Vec<JobInfo> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|').map(str::trim);
            let (name, state, action) =
                (it.next()?, it.next().unwrap_or(""), it.next().unwrap_or(""));
            TASKS.iter().any(|(_, n)| *n == name).then(|| JobInfo {
                name: name.into(),
                state: state.into(),
                action: action.into(),
            })
        })
        .collect()
}

/// `powershell … -EncodedCommand <base64 of UTF-16LE>`: no quoting layer
/// between us and PowerShell, so Cyrillic paths and quotes arrive intact.
pub fn powershell(script: &str) -> SpawnSpec {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    SpawnSpec {
        program: "powershell".into(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-EncodedCommand".into(),
            base64(&utf16),
        ],
    }
}

/// Standard base64 with padding (PowerShell's `-EncodedCommand` alphabet).
pub fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    #[test]
    fn the_us_window_gets_both_minutes_of_a_year() {
        let s = Settings::default();
        let times = trigger_times(NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), &s);
        assert_eq!(times[&WindowId::Eu], [t("09:30")].into_iter().collect());
        assert_eq!(
            times[&WindowId::Us],
            [t("15:00"), t("16:00")].into_iter().collect(),
            "the mismatch weeks move the US window an hour earlier"
        );
        assert_eq!(times[&WindowId::Review], [t("20:00")].into_iter().collect());
    }

    #[test]
    fn the_registration_script() {
        let mut times = HashMap::new();
        times.insert(WindowId::Eu, [t("09:30")].into_iter().collect());
        times.insert(WindowId::Us, [t("15:00"), t("16:00")].into_iter().collect());
        let s = register_script(
            Path::new(r"C:\Users\Тимур O'Neil\AppData\Local\Strategy Desk\strategy-desk.exe"),
            &times,
        );
        assert!(s.contains(r"-Execute 'C:\Users\Тимур O''Neil\AppData\Local\Strategy Desk\strategy-desk.exe' -Argument 'run --window eu'"), "{s}");
        assert!(
            s.contains("-At '15:00'), (New-ScheduledTaskTrigger"),
            "two triggers for US: {s}"
        );
        assert!(s.contains("Register-ScheduledTask -TaskName 'Trading Macro US'"));
        assert!(!s.contains("Trading Review"), "no times, no job");
        assert!(s.contains("-DaysOfWeek Monday,Tuesday,Wednesday,Thursday,Friday"));
        assert!(
            s.contains("SetEnvironmentVariable('TV_SESSIONID', $null, 'User')"),
            "the old cookie is removed"
        );
        assert!(
            !s.to_lowercase().contains("sessionid ="),
            "no cookie is ever written"
        );
    }

    #[test]
    fn unregister_and_query() {
        let u = unregister_script();
        for (_, n) in TASKS {
            assert!(u.contains(&format!(
                "Unregister-ScheduledTask -TaskName '{n}' -Confirm:$false"
            )));
        }
        assert!(u.contains("'MACRO_VAULT', $null, 'User'"));
        let q = query_script();
        assert!(q.starts_with(
            "Get-ScheduledTask -TaskName 'Trading Macro EU','Trading Macro US','Trading Review'"
        ));
        let jobs = parse_query("Trading Macro EU|Ready|C:\\x\\strategy-desk.exe run --window eu\r\nOther|Ready|x\nTrading Review|Disabled|powershell -File macro_review.ps1\n\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].state, "Ready");
        assert!(!jobs[0].is_old());
        assert!(jobs[1].is_old());
    }

    #[test]
    fn encoded_command_is_utf16le_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        let spec = powershell("'я'");
        assert_eq!(spec.args[4], "-EncodedCommand");
        // 'я' = U+0027 U+044F U+0027 → 27 00 4F 04 27 00
        assert_eq!(spec.args[5], base64(&[0x27, 0, 0x4f, 0x04, 0x27, 0]));
        assert_eq!(ps_quote("a'b"), "'a''b'");
    }
}
