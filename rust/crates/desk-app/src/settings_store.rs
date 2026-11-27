//! `settings.json` on disk: atomic writes, and on the very first start an
//! import of the Electron app's `config.json` (secrets are not carried over:
//! safeStorage blobs are tied to Chromium, TZ «Архитектура»).

use std::io;
use std::path::{Path, PathBuf};

use crate::paths::settings_file;
use crate::vault_write::atomic_write;
use desk_core::settings::{import_electron, Settings};

/// Where the settings came from on this start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Our own `settings.json`.
    Stored,
    /// First start: imported from the Electron app's config (and saved).
    Imported,
    /// First start, nothing to import: defaults (and saved).
    Fresh,
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    file: PathBuf,
}

impl SettingsStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            file: settings_file(data_dir),
        }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    /// Read the settings; on the first start import `electron` (if it is
    /// there) or take the defaults, fill in the default vault, and save.
    /// A broken `settings.json` is NOT replaced: it is the owner's file, so
    /// the defaults are used for this run and the error is returned beside them.
    pub fn load(
        &self,
        electron: Option<&Path>,
        default_vault: Option<&Path>,
    ) -> (Settings, Origin, Option<io::Error>) {
        match std::fs::read(&self.file) {
            Ok(bytes) => match serde_json::from_slice::<Settings>(strip_bom(&bytes)) {
                Ok(s) => (s, Origin::Stored, None),
                Err(e) => (
                    Settings::default(),
                    Origin::Stored,
                    Some(io::Error::other(e)),
                ),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let imported = electron
                    .and_then(|p| std::fs::read(p).ok())
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(strip_bom(&b)).ok())
                    .map(|v| import_electron(&v));
                let (mut s, origin) = match imported {
                    Some(s) => (s, Origin::Imported),
                    None => (Settings::default(), Origin::Fresh),
                };
                if s.vault.is_none() {
                    s.vault = default_vault.map(|v| v.to_string_lossy().into_owned());
                }
                let err = self.save(&s).err();
                (s, origin, err)
            }
            Err(e) => (Settings::default(), Origin::Stored, Some(e)),
        }
    }

    pub fn save(&self, s: &Settings) -> io::Result<()> {
        let dir = self
            .file
            .parent()
            .ok_or_else(|| io::Error::other("settings file has no folder"))?;
        std::fs::create_dir_all(dir)?;
        let bytes = serde_json::to_vec_pretty(s).map_err(io::Error::other)?;
        atomic_write(dir, "settings.json", &bytes).map(drop)
    }
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(b"\xef\xbb\xbf").unwrap_or(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_start_imports_the_electron_config_once() {
        let data = tempfile::tempdir().unwrap();
        let old = data.path().join("old-config.json");
        // A BOM, as PowerShell may leave; secrets must not come along.
        std::fs::write(
            &old,
            "\u{feff}{\"vaultPath\":\"C:\\\\Users\\\\T\\\\Documents\\\\Strategy Desk\",\"cdpPort\":9224,\"theme\":\"dark\",\"tvmcpPath\":\"\",\"apiKey\":\"sk-secret\"}",
        )
        .unwrap();
        let store = SettingsStore::new(data.path());
        let (s, origin, err) = store.load(Some(&old), Some(Path::new("/docs/Strategy Desk")));
        assert!(err.is_none(), "{err:?}");
        assert_eq!(origin, Origin::Imported);
        assert_eq!(
            s.vault.as_deref(),
            Some(r"C:\Users\T\Documents\Strategy Desk")
        );
        assert_eq!(s.cdp_port, 9224);
        assert_eq!(s.theme, desk_core::settings::Theme::Dark);
        assert_eq!(
            s.macro_mcp,
            desk_core::settings::MacroMcp::User,
            "an old config without macroMcp is the owner's"
        );
        let text = std::fs::read_to_string(store.file()).unwrap();
        assert!(!text.contains("sk-secret"));
        // Second start: our own file wins, the old one is not read again.
        std::fs::write(&old, "{\"vaultPath\":\"elsewhere\"}").unwrap();
        let (s2, origin, _) = store.load(Some(&old), None);
        assert_eq!(origin, Origin::Stored);
        assert_eq!(s2, s);
    }

    #[test]
    fn fresh_start_takes_defaults_and_the_default_vault() {
        let data = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(data.path());
        let (s, origin, err) = store.load(
            Some(&data.path().join("absent.json")),
            Some(Path::new("/docs/Strategy Desk")),
        );
        assert!(err.is_none());
        assert_eq!(origin, Origin::Fresh);
        assert_eq!(s.vault.as_deref(), Some("/docs/Strategy Desk"));
        assert_eq!(s.macro_mcp, desk_core::settings::MacroMcp::Bundled);
        assert!(store.file().exists());
    }

    #[test]
    fn a_broken_file_is_kept_and_reported() {
        let data = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(data.path());
        std::fs::write(store.file(), b"{broken").unwrap();
        let (s, _, err) = store.load(None, None);
        assert!(err.is_some());
        assert_eq!(s, Settings::default());
        assert_eq!(
            std::fs::read(store.file()).unwrap(),
            b"{broken",
            "never overwritten on its own"
        );
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let data = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(data.path());
        std::fs::write(
            store.file(),
            br#"{"vault":"/v","future_field":1,"schedule":{"enabled":true}}"#,
        )
        .unwrap();
        let (s, _, err) = store.load(None, None);
        assert!(err.is_none(), "{err:?}");
        assert_eq!(s.vault.as_deref(), Some("/v"));
        assert!(s.schedule.enabled);
        assert!(s.schedule.us, "a window is on unless switched off");
        let mut s = s;
        s.cdp_port = 9300;
        store.save(&s).unwrap();
        assert_eq!(store.load(None, None).0.cdp_port, 9300);
    }
}
