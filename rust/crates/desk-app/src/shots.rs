//! The I/O half of `readShot` (`src/main/macro-state.ts`): read a chart
//! screenshot the day names, off the UI thread, after checking that it really
//! is one of the skill's pictures. desk-core's `shot_ref` checks the path
//! shape, the size and the PNG magic; this checks where the file REALLY is.
//!
//! The rule is the TS one: the canonical path (symlinks and junctions
//! resolved) must lie inside the canonical `<vault>/indices-kb/shots/`, so a
//! link placed inside `shots/` cannot lead out of it.

use std::fmt;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use desk_core::names::is_shot_path;
use desk_core::shots::{shot_ref, SHOT_MAX_BYTES};
use desk_core::vault::{paths, ShotRef};

use crate::day::full_path;

/// A screenshot ready for the UI to decode into a texture.
#[derive(Clone, PartialEq, Eq)]
pub struct LoadedShot {
    pub shot: ShotRef,
    /// The PNG file as it is on disk.
    pub png: Arc<[u8]>,
}

impl fmt::Debug for LoadedShot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoadedShot")
            .field("shot", &self.shot)
            .field("png_len", &self.png.len())
            .finish()
    }
}

/// Why a picture is not shown. Not a user error: the tile just stays empty,
/// as in the TS (`readShot` → null).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotRefused {
    /// Not a `.png` path under `indices-kb/shots/`.
    BadPath,
    /// Missing or unreadable.
    Missing,
    /// Resolves outside `indices-kb/shots/` (a link out).
    Outside,
    /// Not a file, too big, too small, or not a PNG.
    NotAShot,
}

/// Read `rel` (vault-relative, `/` or `\`) for the UI.
pub fn load_shot(vault: &Path, rel: &str) -> Result<LoadedShot, ShotRefused> {
    if !is_shot_path(rel) {
        return Err(ShotRefused::BadPath);
    }
    let rel = rel.replace('\\', "/");
    let root = paths::SHOTS
        .iter()
        .fold(vault.to_path_buf(), |p, s| p.join(s));
    let real_root = std::fs::canonicalize(&root).map_err(|_| ShotRefused::Missing)?;
    let real = std::fs::canonicalize(full_path(vault, &rel)).map_err(|_| ShotRefused::Missing)?;
    if real == real_root || !real.starts_with(&real_root) {
        return Err(ShotRefused::Outside);
    }
    let meta = std::fs::metadata(&real).map_err(|_| ShotRefused::Missing)?;
    if !meta.is_file() || meta.len() > SHOT_MAX_BYTES {
        return Err(ShotRefused::NotAShot);
    }
    let mut png = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(0));
    // `take`: a file that grew since the stat is still capped.
    std::fs::File::open(&real)
        .and_then(|f| f.take(SHOT_MAX_BYTES + 1).read_to_end(&mut png))
        .map_err(|_| ShotRefused::Missing)?;
    let shot = shot_ref(&rel, &png, png.len() as u64).ok_or(ShotRefused::NotAShot)?;
    Ok(LoadedShot {
        shot,
        png: png.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PNG: &[u8] = &[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];

    fn vault() -> tempfile::TempDir {
        let v = tempfile::tempdir().unwrap();
        let s = v.path().join("indices-kb").join("shots").join("2026-11-25");
        fs::create_dir_all(&s).unwrap();
        fs::write(s.join("eu_h1.png"), PNG).unwrap();
        fs::write(s.join("fake.png"), b"GIF89a-not-a-png").unwrap();
        v
    }

    #[test]
    fn reads_a_real_shot() {
        let v = vault();
        let s = load_shot(v.path(), "indices-kb/shots/2026-11-25/eu_h1.png").unwrap();
        assert_eq!(
            s.shot,
            ShotRef {
                path: "indices-kb/shots/2026-11-25/eu_h1.png".into(),
                bytes: PNG.len() as u64
            }
        );
        assert_eq!(&*s.png, PNG);
        // Backslashes as the skill may write them on Windows.
        let b = load_shot(v.path(), r"indices-kb\shots\2026-11-25\eu_h1.png").unwrap();
        assert_eq!(b.shot.path, "indices-kb/shots/2026-11-25/eu_h1.png");
    }

    #[test]
    fn refuses_what_is_not_a_shot() {
        let v = vault();
        assert_eq!(
            load_shot(v.path(), "indices-kb/shots/2026-11-25/fake.png"),
            Err(ShotRefused::NotAShot)
        );
        assert_eq!(
            load_shot(v.path(), "indices-kb/shots/2026-11-25/none.png"),
            Err(ShotRefused::Missing)
        );
        assert_eq!(
            load_shot(v.path(), "indices-kb/shots/../state/x.png"),
            Err(ShotRefused::BadPath)
        );
        assert_eq!(
            load_shot(v.path(), "indices-kb/state/2026/x.png"),
            Err(ShotRefused::BadPath)
        );
        assert_eq!(load_shot(v.path(), "/etc/x.png"), Err(ShotRefused::BadPath));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_shots_is_refused() {
        let v = vault();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.png"), PNG).unwrap();
        let link = v.path().join("indices-kb/shots/2026-11-25/link.png");
        std::os::unix::fs::symlink(outside.path().join("secret.png"), &link).unwrap();
        assert_eq!(
            load_shot(v.path(), "indices-kb/shots/2026-11-25/link.png"),
            Err(ShotRefused::Outside)
        );
        // A link that stays inside is fine.
        let inner = v.path().join("indices-kb/shots/2026-11-25/inner.png");
        std::os::unix::fs::symlink(
            v.path().join("indices-kb/shots/2026-11-25/eu_h1.png"),
            &inner,
        )
        .unwrap();
        assert!(load_shot(v.path(), "indices-kb/shots/2026-11-25/inner.png").is_ok());
    }
}
