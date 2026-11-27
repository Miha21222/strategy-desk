//! The Python environment of tvmcp and the vault tools — port of
//! `src/main/runtime-spec.ts` (pure) and the marker half of `runtime.ts`,
//! without the knowledge base and the headless browser (TZ item 20).
//!
//! BUNDLED: the installer carries the tvmcp payload (`pyproject.toml` +
//! `uv.lock` + sources) and a `uv`; `uv sync --frozen` builds the venv under
//! `<data>/py`. EXTERNAL: a complete checkout named in the settings plus a
//! `uv` on PATH — then nothing is provisioned (`preferExternal`, the owner's
//! rule of 2026-10-12: what is on the PC is not installed again).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::macro_env::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Bundled,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    /// The shipped tvmcp payload; `None` = external (dev run or own checkout).
    pub payload_dir: Option<PathBuf>,
    /// The shipped uv; `None` = `uv` from PATH.
    pub uv_bin: Option<PathBuf>,
    /// UV_PROJECT_ENVIRONMENT.
    pub venv_dir: PathBuf,
    /// UV_CACHE_DIR.
    pub cache_dir: PathBuf,
    /// UV_PYTHON_INSTALL_DIR.
    pub python_install_dir: PathBuf,
    /// NUMBA_CACHE_DIR: numba writes next to its code otherwise.
    pub numba_cache_dir: PathBuf,
    /// PYTHONPYCACHEPREFIX: same reason, for .pyc files.
    pub pycache_dir: PathBuf,
    /// «provisioned, from this uv.lock».
    pub marker: PathBuf,
}

impl RuntimePaths {
    /// The paths under `root` (`<data>/py`), with or without a payload.
    pub fn under(root: &Path, payload_dir: Option<PathBuf>, uv_bin: Option<PathBuf>) -> Self {
        Self {
            payload_dir,
            uv_bin,
            venv_dir: root.join("venv"),
            cache_dir: root.join("cache"),
            python_install_dir: root.join("python"),
            numba_cache_dir: root.join("numba"),
            pycache_dir: root.join("pycache"),
            marker: root.join("provisioned.json"),
        }
    }

    pub fn mode(&self) -> Mode {
        if self.payload_dir.is_some() {
            Mode::Bundled
        } else {
            Mode::External
        }
    }
}

fn venv_python(venv: &Path, p: Platform) -> PathBuf {
    match p {
        Platform::Windows => venv.join("Scripts").join("python.exe"),
        Platform::Posix => venv.join("bin").join("python"),
    }
}

/// What a checkout must have to be used instead of the payload.
pub fn external_clone_files(clone: &str, p: Platform) -> Vec<PathBuf> {
    let c = clone.trim();
    if c.is_empty() {
        return Vec::new();
    }
    let c = Path::new(c);
    vec![c.join("pyproject.toml"), venv_python(&c.join(".venv"), p)]
}

/// `preferExternal`.
pub fn prefer_external(
    clone: &str,
    has: impl Fn(&Path) -> bool,
    uv_on_path: bool,
    p: Platform,
) -> bool {
    let files = external_clone_files(clone, p);
    !files.is_empty() && uv_on_path && files.iter().all(|f| has(f))
}

/// The interpreter the vault tools run on: the provisioned venv (bundled),
/// the checkout's `.venv` (external), or `None` (tools fall back to PATH).
pub fn tools_python(paths: &RuntimePaths, tvmcp_path: &str, p: Platform) -> Option<PathBuf> {
    match paths.mode() {
        Mode::Bundled => Some(venv_python(&paths.venv_dir, p)),
        Mode::External => {
            let c = tvmcp_path.trim();
            (!c.is_empty()).then(|| venv_python(&Path::new(c).join(".venv"), p))
        }
    }
}

/// `runtimeEnv`: the cache and install locations of the provisioned runtime.
pub fn runtime_env(paths: &RuntimePaths) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    if paths.mode() == Mode::External {
        return env;
    }
    let s = |p: &Path| p.to_string_lossy().into_owned();
    env.insert("UV_PROJECT_ENVIRONMENT".into(), s(&paths.venv_dir));
    env.insert("UV_CACHE_DIR".into(), s(&paths.cache_dir));
    env.insert("UV_PYTHON_INSTALL_DIR".into(), s(&paths.python_install_dir));
    env.insert("NUMBA_CACHE_DIR".into(), s(&paths.numba_cache_dir));
    env.insert("PYTHONPYCACHEPREFIX".into(), s(&paths.pycache_dir));
    env
}

/// A program and its arguments, shown before it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl SpawnSpec {
    /// One line for the UI (quoted where it has spaces).
    pub fn display(&self) -> String {
        std::iter::once(self.program.to_string_lossy().into_owned())
            .chain(self.args.iter().cloned())
            .map(|a| {
                if a.contains(' ') {
                    format!("\"{a}\"")
                } else {
                    a
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// `provisionSpec` without the kb extra: `uv sync --frozen` into our venv.
/// `None` in external mode (nothing to provision).
pub fn provision_spec(paths: &RuntimePaths) -> Option<SpawnSpec> {
    let payload = paths.payload_dir.as_ref()?;
    Some(SpawnSpec {
        program: paths.uv_bin.clone().unwrap_or_else(|| PathBuf::from("uv")),
        args: vec![
            "sync".into(),
            "--frozen".into(),
            "--no-dev".into(),
            "--managed-python".into(),
            "--project".into(),
            payload.to_string_lossy().into_owned(),
        ],
    })
}

/// A stable fingerprint of `uv.lock` (FNV-1a 64): «built from this lock».
/// Not a security hash — it only answers «did the payload change».
pub fn lock_hash(lock: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in lock {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// What the runtime is right now (the preflight's `runtime` fact).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFact {
    pub mode: Mode,
    /// The payload has its pyproject and uv.lock.
    pub payload_ok: bool,
    /// Built, from the current uv.lock.
    pub provisioned: bool,
    /// Built, from an older uv.lock.
    pub stale: bool,
}

/// Read the marker and the lock. External mode is always «ok, not ours».
pub fn fact(paths: &RuntimePaths, p: Platform) -> RuntimeFact {
    let Some(payload) = &paths.payload_dir else {
        return RuntimeFact {
            mode: Mode::External,
            payload_ok: true,
            provisioned: false,
            stale: false,
        };
    };
    let lock = std::fs::read(payload.join("uv.lock")).ok();
    let payload_ok = lock.is_some() && payload.join("pyproject.toml").is_file();
    let want = lock.as_deref().map(lock_hash).unwrap_or_default();
    let have = std::fs::read(&paths.marker)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| {
            v.get("lock_hash")
                .and_then(|h| h.as_str())
                .map(str::to_owned)
        });
    let python = venv_python(&paths.venv_dir, p).is_file();
    let provisioned = python && !want.is_empty() && have.as_deref() == Some(want.as_str());
    let stale = python && have.is_some() && !provisioned;
    RuntimeFact {
        mode: Mode::Bundled,
        payload_ok,
        provisioned,
        stale,
    }
}

/// Written only after a clean `uv sync`, so a half-done sync never looks done.
pub fn write_marker(paths: &RuntimePaths, uv: &str, now_ms: i64) -> std::io::Result<()> {
    let Some(payload) = &paths.payload_dir else {
        return Ok(());
    };
    let lock = std::fs::read(payload.join("uv.lock"))?;
    let v = serde_json::json!({ "lock_hash": lock_hash(&lock), "uv": uv, "at": now_ms });
    let dir = paths
        .marker
        .parent()
        .ok_or_else(|| std::io::Error::other("marker has no folder"))?;
    std::fs::create_dir_all(dir)?;
    crate::vault_write::atomic_write(dir, "provisioned.json", v.to_string().as_bytes()).map(drop)
}

/// «Очистить рантайм»: delete `<data>/py`. Only ever on the owner's press.
pub fn clear(root: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(root) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Where the installer put the payload and uv: next to the exe, under
/// `resources/` or directly. TODO(stage 7): fixed by the release layout.
pub fn find_payload(exe_dir: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let uv_name = if cfg!(windows) { "uv.exe" } else { "uv" };
    let roots = [exe_dir.join("resources"), exe_dir.to_path_buf()];
    let payload = roots
        .iter()
        .map(|r| r.join("tvmcp"))
        .find(|p| p.join("pyproject.toml").is_file());
    let uv = roots.iter().map(|r| r.join(uv_name)).find(|p| p.is_file());
    (payload, uv)
}

/// `uv` reachable the way a shell reaches it.
pub fn uv_on_path() -> bool {
    let name = if cfg!(windows) { "uv.exe" } else { "uv" };
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// The paths this desk runs with: bundled when a payload ships and no
/// complete checkout + uv on PATH says otherwise.
pub fn resolve(data_root: &Path, exe_dir: Option<&Path>, tvmcp_path: &str) -> RuntimePaths {
    let (mut payload, uv) = exe_dir.map(find_payload).unwrap_or((None, None));
    if payload.is_some()
        && prefer_external(tvmcp_path, |p| p.is_file(), uv_on_path(), Platform::host())
    {
        payload = None;
    }
    RuntimePaths::under(data_root, payload, uv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_is_preferred_only_for_a_complete_checkout_with_uv() {
        let have = |p: &Path| p.ends_with("pyproject.toml") || p.ends_with("python.exe");
        assert!(prefer_external(r"C:\tvmcp", have, true, Platform::Windows));
        assert!(
            !prefer_external(r"C:\tvmcp", have, false, Platform::Windows),
            "no uv on PATH"
        );
        assert!(
            !prefer_external("", have, true, Platform::Windows),
            "no path set = fresh install"
        );
        assert!(
            !prefer_external(
                r"C:\tvmcp",
                |p: &Path| p.ends_with("pyproject.toml"),
                true,
                Platform::Windows
            ),
            "no venv"
        );
        assert_eq!(
            external_clone_files("/t", Platform::Posix),
            vec![
                PathBuf::from("/t/pyproject.toml"),
                PathBuf::from("/t/.venv/bin/python")
            ]
        );
    }

    #[test]
    fn provision_and_env_in_bundled_mode_only() {
        let root = Path::new("/data/py");
        let ext = RuntimePaths::under(root, None, None);
        assert_eq!(ext.mode(), Mode::External);
        assert!(provision_spec(&ext).is_none());
        assert!(runtime_env(&ext).is_empty());
        assert_eq!(
            tools_python(&ext, "/t", Platform::Posix),
            Some(PathBuf::from("/t/.venv/bin/python"))
        );
        assert_eq!(tools_python(&ext, "", Platform::Posix), None);

        let b = RuntimePaths::under(
            root,
            Some("/app/resources/tvmcp".into()),
            Some("/app/resources/uv".into()),
        );
        let spec = provision_spec(&b).unwrap();
        assert_eq!(spec.program, PathBuf::from("/app/resources/uv"));
        assert_eq!(
            spec.args,
            [
                "sync",
                "--frozen",
                "--no-dev",
                "--managed-python",
                "--project",
                "/app/resources/tvmcp"
            ]
        );
        let env = runtime_env(&b);
        assert_eq!(
            env["UV_PROJECT_ENVIRONMENT"],
            Path::new("/data/py/venv").to_string_lossy()
        );
        assert!(env.contains_key("NUMBA_CACHE_DIR") && env.contains_key("PYTHONPYCACHEPREFIX"));
        assert_eq!(
            tools_python(&b, "/ignored", Platform::Windows),
            Some(
                Path::new("/data/py/venv")
                    .join("Scripts")
                    .join("python.exe")
            )
        );
        let no_uv = RuntimePaths::under(root, Some("/p".into()), None);
        assert_eq!(provision_spec(&no_uv).unwrap().program, PathBuf::from("uv"));
    }

    #[test]
    fn the_marker_follows_the_lock() {
        let d = tempfile::tempdir().unwrap();
        let payload = d.path().join("tvmcp");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(
            payload.join("pyproject.toml"),
            "[project]\nname = \"tvmcp\"\n",
        )
        .unwrap();
        std::fs::write(payload.join("uv.lock"), "lock v1").unwrap();
        let paths = RuntimePaths::under(&d.path().join("py"), Some(payload.clone()), None);
        let f = fact(&paths, Platform::host());
        assert!(f.payload_ok && !f.provisioned && !f.stale);
        let py = venv_python(&paths.venv_dir, Platform::host());
        std::fs::create_dir_all(py.parent().unwrap()).unwrap();
        std::fs::write(&py, b"").unwrap();
        write_marker(&paths, "uv", 1).unwrap();
        assert!(fact(&paths, Platform::host()).provisioned);
        std::fs::write(payload.join("uv.lock"), "lock v2").unwrap();
        let f = fact(&paths, Platform::host());
        assert!(!f.provisioned && f.stale, "built from another lock");
        clear(&d.path().join("py")).unwrap();
        clear(&d.path().join("py")).unwrap();
        assert!(!d.path().join("py").exists());
    }

    #[test]
    fn display_quotes_spaces() {
        let s = SpawnSpec {
            program: "C:/Program Files/uv.exe".into(),
            args: vec!["sync".into(), "--project".into(), "C:/a b".into()],
        };
        assert_eq!(
            s.display(),
            "\"C:/Program Files/uv.exe\" sync --project \"C:/a b\""
        );
        assert_eq!(lock_hash(b"x"), lock_hash(b"x"));
        assert_ne!(lock_hash(b"x"), lock_hash(b"y"));
    }
}
