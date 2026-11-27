//! Where the desk keeps its own files (never in the vault).
//!
//! `<data>` = `%APPDATA%\strategy-desk-rs` on Windows, `$XDG_DATA_HOME/
//! strategy-desk-rs` (or `~/.local/share/…`) on Linux:
//! `settings.json`, `chats/`, `logs/`, `py/` (the provisioned Python).

use std::path::{Path, PathBuf};

pub use crate::chat::store::{default_data_dir, APP_DIR};

/// The desk's data folder, or the override (tests, `--data-dir`).
pub fn data_dir(override_dir: Option<&Path>) -> Option<PathBuf> {
    override_dir
        .map(Path::to_path_buf)
        .or_else(default_data_dir)
}

pub fn settings_file(data: &Path) -> PathBuf {
    data.join("settings.json")
}

pub fn logs_dir(data: &Path) -> PathBuf {
    data.join("logs")
}

/// The provisioned runtime's root (`runtimeRoot()` of the old app).
pub fn runtime_root(data: &Path) -> PathBuf {
    data.join("py")
}

/// The Electron app's `config.json` (its `userData`): `%APPDATA%\strategy-desk`
/// on Windows, `~/.config/strategy-desk` on Linux.
pub fn electron_config() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        dirs::data_dir()
    } else {
        dirs::config_dir()
    };
    base.map(|d| d.join("strategy-desk").join("config.json"))
}

/// Where a fresh install's vault goes when nothing names one
/// (`defaultVaultPath()`: Documents\Strategy Desk).
pub fn default_vault() -> Option<PathBuf> {
    dirs::document_dir().map(|d| d.join("Strategy Desk"))
}

/// The folder of the running executable (resources are looked for there).
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}
