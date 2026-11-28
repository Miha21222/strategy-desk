//! Start TradingView Desktop with `--remote-debugging-port` — port of
//! `src/main/tv-launch.ts`. Never kills a running TradingView: one started
//! without the flag is reported, and the owner closes it himself.
//!
//! TradingView Desktop on Windows is an MSIX package (`TradingView.Desktop`):
//! no Uninstall key, the exe lives under `InstallLocation`, which
//! `Get-AppxPackage` reports.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::status::{cdp_probe, tv_running};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launched {
    /// The port already answers: nothing was started.
    AlreadyUp,
    /// Started, and the port answered.
    Started,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("TradingView runs without the debugging port")]
    RunningWithoutFlag,
    #[error("TradingView Desktop not found")]
    ExeNotFound,
    #[error("could not start TradingView: {0}")]
    SpawnFailed(String),
    #[error("TradingView started but the port did not answer")]
    Timeout,
}

impl LaunchError {
    pub fn key(&self) -> &'static str {
        match self {
            LaunchError::RunningWithoutFlag => "tv.error.running_without_flag",
            LaunchError::ExeNotFound => "tv.error.exe_not_found",
            LaunchError::SpawnFailed(_) => "tv.error.spawn_failed",
            LaunchError::Timeout => "tv.error.timeout",
        }
    }
}

pub fn flag(port: u16) -> String {
    format!("--remote-debugging-port={port}")
}

/// The MSIX package's folder (read-only query).
pub const APPX_QUERY: &str =
    "(Get-AppxPackage -Name 'TradingView.Desktop' | Select-Object -First 1).InstallLocation";

/// Where TradingView.exe may be: the configured path, the MSIX package's
/// folder (`appx` = what [`APPX_QUERY`] answered), the old per-user installer.
pub fn candidates(
    configured: &str,
    appx: Option<&str>,
    local_app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if !configured.trim().is_empty() {
        v.push(PathBuf::from(configured.trim()));
    }
    if let Some(a) = appx.map(str::trim).filter(|a| !a.is_empty()) {
        v.push(Path::new(a).join("TradingView.exe"));
    }
    if let Some(l) = local_app_data {
        v.push(
            l.join("Programs")
                .join("TradingView")
                .join("TradingView.exe"),
        );
    }
    v
}

async fn appx_location() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    crate::status::output(
        Path::new("powershell"),
        &["-NoProfile", "-NonInteractive", "-Command", APPX_QUERY],
        Duration::from_secs(15),
    )
    .await
}

pub async fn find_exe(configured: &str) -> Option<PathBuf> {
    let appx = appx_location().await;
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let found = candidates(configured, appx.as_deref(), local.as_deref())
        .into_iter()
        .find(|p| p.is_file());
    if found.is_some() || cfg!(windows) {
        return found;
    }
    // Linux / macOS dev runs: whatever `tradingview` PATH has.
    Some(PathBuf::from("tradingview"))
}

/// Start it (detached) and wait up to `wait` for the port.
pub async fn launch(port: u16, configured: &str, wait: Duration) -> Result<Launched, LaunchError> {
    let probe = Duration::from_millis(1500);
    if cdp_probe(port, probe).await.up {
        return Ok(Launched::AlreadyUp);
    }
    if tokio::task::spawn_blocking(tv_running)
        .await
        .unwrap_or(false)
    {
        return Err(LaunchError::RunningWithoutFlag);
    }
    let exe = find_exe(configured).await.ok_or(LaunchError::ExeNotFound)?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg(flag(port))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: it outlives the desk.
        cmd.creation_flags(0x0000_0008 | 0x0000_0200);
    }
    // The child is not waited on: TradingView is the owner's app, not ours.
    drop(
        cmd.spawn()
            .map_err(|e| LaunchError::SpawnFailed(e.to_string()))?,
    );
    let until = tokio::time::Instant::now() + wait;
    while tokio::time::Instant::now() < until {
        if cdp_probe(port, probe).await.up {
            return Ok(Launched::Started);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(LaunchError::Timeout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_it_is_looked_for() {
        let c = candidates(
            "  ",
            Some(r"C:\Program Files\WindowsApps\TradingView.Desktop_2.9.0_x64__n534cwy3pjxzj "),
            Some(Path::new(r"C:\Users\T\AppData\Local")),
        );
        assert_eq!(c.len(), 2);
        assert!(c[0].ends_with("TradingView.exe"));
        assert!(c[0].to_string_lossy().contains("TradingView.Desktop_2.9.0"));
        assert!(c[1].to_string_lossy().contains("Programs"));
        let c = candidates("D:/tv/TradingView.exe", None, None);
        assert_eq!(c, vec![PathBuf::from("D:/tv/TradingView.exe")]);
        assert_eq!(flag(9223), "--remote-debugging-port=9223");
        assert!(APPX_QUERY.contains("Get-AppxPackage -Name 'TradingView.Desktop'"));
        assert_eq!(
            LaunchError::RunningWithoutFlag.key(),
            "tv.error.running_without_flag"
        );
    }
}
