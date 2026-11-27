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

use std::path::Path;

use desk_core::schedule::{task_name, PlannedRun, LEGACY_TASKS, TASK_PREFIX};
use desk_core::windows::WindowId;

use crate::runtime::SpawnSpec;

/// User variables the old app wrote for its scheduled scripts.
pub const OLD_USER_VARS: [&str; 6] = [
    "MACRO_VAULT",
    "MACRO_PYTHON",
    "MACRO_CLAUDE",
    "TV_CDP_URL",
    "PYTHONIOENCODING",
    "TV_SESSIONID",
];

/// The job's name as registered: desk-core's `task_name` («Strategy Desk US
/// 1500»; a task is a file under `C:\Windows\System32\Tasks`, so no `:`).
pub fn job_name(run: &PlannedRun) -> String {
    task_name(run)
}

/// PowerShell single-quoted string.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Remove every job of ours (any minute) and the old desk's three.
fn remove_all() -> Vec<String> {
    let mut v: Vec<String> = LEGACY_TASKS
        .iter()
        .map(|n| format!("Unregister-ScheduledTask -TaskName {} -Confirm:$false -ErrorAction SilentlyContinue", ps_quote(n)))
        .collect();
    v.push(format!(
        "Get-ScheduledTask -TaskName {} -ErrorAction SilentlyContinue | Unregister-ScheduledTask -Confirm:$false",
        ps_quote(&format!("{TASK_PREFIX}*"))
    ));
    v
}

/// The registration script (shown to the owner before it runs): one job per
/// trigger of desk-core's `triggers`, weekdays, started late when the PC slept.
pub fn register_script(exe: &Path, runs: &[PlannedRun]) -> String {
    let mut lines = vec!["$ErrorActionPreference = 'Stop'".to_string()];
    lines.extend(remove_all());
    // Late (asleep at the minute) still starts; `run` then asks `due_window`.
    lines.push("$s = New-ScheduledTaskSettingsSet -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Hours 2)".into());
    for w in WindowId::ALL {
        let mine: Vec<&PlannedRun> = runs.iter().filter(|r| r.window == w).collect();
        if mine.is_empty() {
            continue;
        }
        lines.push(format!(
            "$a = New-ScheduledTaskAction -Execute {} -Argument {}",
            ps_quote(&exe.to_string_lossy()),
            ps_quote(&format!("run --window {}", w.as_str()))
        ));
        for r in mine {
            lines.push(format!(
                "$t = New-ScheduledTaskTrigger -Weekly -DaysOfWeek Monday,Tuesday,Wednesday,Thursday,Friday -At {}",
                ps_quote(&r.at_local.format("%H:%M").to_string())
            ));
            lines.push(format!(
                "Register-ScheduledTask -TaskName {} -Action $a -Trigger $t -Settings $s -Description {} -Force | Out-Null",
                ps_quote(&job_name(r)),
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
    let mut names: Vec<String> = vec![ps_quote(&format!("{TASK_PREFIX}*"))];
    names.extend(LEGACY_TASKS.iter().map(|n| ps_quote(n)));
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
        LEGACY_TASKS.contains(&self.name.as_str())
    }
}

pub fn parse_query(out: &str) -> Vec<JobInfo> {
    let ours = TASK_PREFIX;
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|').map(str::trim);
            let (name, state, action) =
                (it.next()?, it.next().unwrap_or(""), it.next().unwrap_or(""));
            (name.starts_with(ours) || LEGACY_TASKS.contains(&name)).then(|| JobInfo {
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

    fn t(s: &str) -> chrono::NaiveTime {
        chrono::NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    fn runs() -> Vec<PlannedRun> {
        let s = desk_core::settings::Settings::default();
        desk_core::schedule::triggers(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(), &s)
    }

    #[test]
    fn the_registration_script() {
        let s = register_script(
            Path::new(r"C:\Users\Тимур O'Neil\AppData\Local\Strategy Desk\strategy-desk.exe"),
            &runs(),
        );
        assert!(s.contains(r"-Execute 'C:\Users\Тимур O''Neil\AppData\Local\Strategy Desk\strategy-desk.exe' -Argument 'run --window eu'"), "{s}");
        let jobs: Vec<&str> = s
            .lines()
            .filter(|l| l.starts_with("Register-ScheduledTask"))
            .collect();
        assert_eq!(jobs.len(), 4, "09:30, 15:00, 16:00, 20:00: {s}");
        assert!(
            s.contains("-TaskName 'Strategy Desk US 1500'")
                && s.contains("-TaskName 'Strategy Desk US 1600'"),
            "{s}"
        );
        assert!(
            !jobs
                .iter()
                .any(|j| j.split('\'').nth(1).is_some_and(|n| n.contains(':'))),
            "no colon in a task name"
        );
        for old in LEGACY_TASKS {
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
        for n in LEGACY_TASKS {
            assert!(u.contains(&format!(
                "Unregister-ScheduledTask -TaskName '{n}' -Confirm:$false"
            )));
        }
        assert!(u.contains("'Strategy Desk *'"));
        assert!(u.contains("'MACRO_VAULT', $null, 'User'"));
        let q = query_script();
        assert!(q.starts_with("Get-ScheduledTask -TaskName 'Strategy Desk *','Trading Macro EU','Trading Macro US','Trading Review'"));
        let jobs = parse_query("Strategy Desk EU 0930|Ready|C:\\x\\strategy-desk.exe run --window eu\r\nOther|Ready|x\nTrading Review|Disabled|powershell -File macro_review.ps1\n\n");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].state, "Ready");
        assert!(!jobs[0].is_old());
        assert!(jobs[1].is_old());
        let review = PlannedRun {
            window: WindowId::Review,
            at_local: t("20:00"),
        };
        assert_eq!(job_name(&review), "Strategy Desk Review 2000");
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
