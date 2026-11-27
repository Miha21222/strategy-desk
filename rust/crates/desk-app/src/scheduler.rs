//! The Task Scheduler jobs (TZ item 18): `strategy-desk.exe run --window
//! <eu|us|review>` on weekdays, one job per window and minute — every minute
//! `planned_runs` can give over a year (09:30, 15:00, 16:00, 20:00 today:
//! the US window moves an hour earlier in the weeks Europe and America change
//! clocks on different days). The run decides with `due_window` whether the
//! minute is its own.
//!
//! Registration removes the Electron desk's three jobs (`Trading Macro EU`,
//! `Trading Macro US`, `Trading Review`, from the vault's
//! `macro_schedule.ps1`) — or every review would run twice — and the cookie
//! that desk kept in HKCU. Nothing is written into the user's environment:
//! `run` reads the settings and the Credential Manager itself.
//!
//! Everything here builds text; the scripts run through the task runner
//! (Windows only). The builders are what the tests pin down.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use chrono::{Days, NaiveDate, NaiveTime};
use desk_core::windows::WindowId;

use crate::core_wip::{planned_runs, Settings};
use crate::runtime::SpawnSpec;

/// The Electron desk's jobs.
pub const OLD_TASKS: [&str; 3] = ["Trading Macro EU", "Trading Macro US", "Trading Review"];

/// Our jobs are `Strategy Desk <window> <HHMM>` (a task name cannot hold `:`).
pub const PREFIX: &str = "Strategy Desk";

/// User variables the old app wrote for its scheduled scripts.
pub const OLD_USER_VARS: [&str; 6] = [
    "MACRO_VAULT",
    "MACRO_PYTHON",
    "MACRO_CLAUDE",
    "TV_CDP_URL",
    "PYTHONIOENCODING",
    "TV_SESSIONID",
];

pub fn job_name(w: WindowId, at: NaiveTime) -> String {
    format!("{PREFIX} {} {}", w.as_str(), at.format("%H%M"))
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

/// Remove every job of ours (any minute) and the old desk's three.
fn remove_all() -> Vec<String> {
    let mut v: Vec<String> = OLD_TASKS
        .iter()
        .map(|n| format!("Unregister-ScheduledTask -TaskName {} -Confirm:$false -ErrorAction SilentlyContinue", ps_quote(n)))
        .collect();
    v.push(format!(
        "Get-ScheduledTask -TaskName {} -ErrorAction SilentlyContinue | Unregister-ScheduledTask -Confirm:$false",
        ps_quote(&format!("{PREFIX} *"))
    ));
    v
}

/// The registration script (shown to the owner before it runs).
pub fn register_script(exe: &Path, times: &HashMap<WindowId, BTreeSet<NaiveTime>>) -> String {
    let mut lines = vec!["$ErrorActionPreference = 'Stop'".to_string()];
    lines.extend(remove_all());
    // Late (asleep at the minute) still starts; `run` then finds it is not
    // its minute any more and exits without work.
    lines.push("$s = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 2)".into());
    for w in WindowId::ALL {
        let Some(at) = times.get(&w) else { continue };
        lines.push(format!(
            "$a = New-ScheduledTaskAction -Execute {} -Argument {}",
            ps_quote(&exe.to_string_lossy()),
            ps_quote(&format!("run --window {}", w.as_str()))
        ));
        for t in at {
            lines.push(format!(
                "$t = New-ScheduledTaskTrigger -Weekly -DaysOfWeek Monday,Tuesday,Wednesday,Thursday,Friday -At {}",
                ps_quote(&t.format("%H:%M").to_string())
            ));
            lines.push(format!(
                "Register-ScheduledTask -TaskName {} -Action $a -Trigger $t -Settings $s -Description {} -Force | Out-Null",
                ps_quote(&job_name(w, *t)),
                ps_quote(&format!("Strategy Desk: macro review, window {}", w.as_str()))
            ));
        }
    }
    // The old app kept the cookie in HKCU for its scheduled scripts; gone.
    lines.push("[Environment]::SetEnvironmentVariable('TV_SESSIONID', $null, 'User')".into());
    lines.push("'registered'".into());
    lines.join("\n")
}

pub fn unregister_script() -> String {
    let mut lines = remove_all();
    lines.extend(OLD_USER_VARS.iter().map(|v| {
        format!(
            "[Environment]::SetEnvironmentVariable({}, $null, 'User')",
            ps_quote(v)
        )
    }));
    lines.push("'unregistered'".into());
    lines.join("\n")
}

/// Read-only: our jobs and the old ones, one `name|state|action` line each.
pub fn query_script() -> String {
    let mut names: Vec<String> = vec![ps_quote(&format!("{PREFIX} *"))];
    names.extend(OLD_TASKS.iter().map(|n| ps_quote(n)));
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
        OLD_TASKS.contains(&self.name.as_str())
    }
}

pub fn parse_query(out: &str) -> Vec<JobInfo> {
    let ours = format!("{PREFIX} ");
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|').map(str::trim);
            let (name, state, action) =
                (it.next()?, it.next().unwrap_or(""), it.next().unwrap_or(""));
            (name.starts_with(&ours) || OLD_TASKS.contains(&name)).then(|| JobInfo {
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
    fn a_year_gives_four_minutes() {
        let times = trigger_times(
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            &Settings::default(),
        );
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
        let times = trigger_times(
            NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            &Settings::default(),
        );
        let s = register_script(
            Path::new(r"C:\Users\Тимур O'Neil\AppData\Local\Strategy Desk\strategy-desk.exe"),
            &times,
        );
        assert!(s.contains(r"-Execute 'C:\Users\Тимур O''Neil\AppData\Local\Strategy Desk\strategy-desk.exe' -Argument 'run --window eu'"), "{s}");
        let jobs: Vec<&str> = s
            .lines()
            .filter(|l| l.starts_with("Register-ScheduledTask"))
            .collect();
        assert_eq!(jobs.len(), 4, "09:30, 15:00, 16:00, 20:00: {s}");
        assert!(
            s.contains("-TaskName 'Strategy Desk us 1500'")
                && s.contains("-TaskName 'Strategy Desk us 1600'")
        );
        for old in OLD_TASKS {
            assert!(
                s.contains(&format!(
                    "Unregister-ScheduledTask -TaskName '{old}' -Confirm:$false"
                )),
                "{old} removed"
            );
        }
        assert!(s.contains("Get-ScheduledTask -TaskName 'Strategy Desk *' -ErrorAction SilentlyContinue | Unregister-ScheduledTask"), "stale minutes go too");
        let first_register = s.find("Register-ScheduledTask -TaskName").unwrap_or(0);
        assert!(
            s.find("Unregister-ScheduledTask").unwrap_or(usize::MAX) < first_register,
            "removal first"
        );
        assert!(
            s.contains("-DaysOfWeek Monday,Tuesday,Wednesday,Thursday,Friday")
                && s.contains("-StartWhenAvailable")
        );
        assert!(
            s.contains("SetEnvironmentVariable('TV_SESSIONID', $null, 'User')"),
            "the old cookie is removed"
        );
    }

    #[test]
    fn unregister_and_query() {
        let u = unregister_script();
        for n in OLD_TASKS {
            assert!(u.contains(&format!(
                "Unregister-ScheduledTask -TaskName '{n}' -Confirm:$false"
            )));
        }
        assert!(u.contains("'Strategy Desk *'"));
        assert!(u.contains("'MACRO_VAULT', $null, 'User'"));
        let q = query_script();
        assert!(q.starts_with("Get-ScheduledTask -TaskName 'Strategy Desk *','Trading Macro EU','Trading Macro US','Trading Review'"));
        let jobs = parse_query("Strategy Desk eu 0930|Ready|C:\\x\\strategy-desk.exe run --window eu\r\nOther|Ready|x\nTrading Review|Disabled|powershell -File macro_review.ps1\n\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].state, "Ready");
        assert!(!jobs[0].is_old());
        assert!(jobs[1].is_old());
        assert_eq!(
            job_name(WindowId::Review, t("20:00")),
            "Strategy Desk review 2000"
        );
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
