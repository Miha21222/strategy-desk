//! How a chat's `claude` process is started: which binary, which prompt
//! appendix, which environment (`macroSpec` in `src/main/agent.ts`).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::model::ChatKind;
use crate::claude::process::CLAUDE_BIN_ENV;
use crate::claude::ClaudeConfig;
use crate::macro_env::{macro_env_vars, MacroEnvInput, Platform};

/// Appendix to Claude Code's preset prompt for the day's review chat.
pub const MACRO_PROMPT: &str = include_str!("../../../../../prompt/macro-review.md");
/// Appendix for «+ Новый чат» (answer from memory, never run a review).
pub const ASK_PROMPT: &str = include_str!("../../../../../prompt/ask.md");

/// Seams for tests and benches; production leaves everything empty and the
/// binary is found as CONTRACTS says. `Debug` shows env keys only.
#[derive(Clone, Default)]
pub struct ClaudeLaunch {
    /// Use this binary (before `DESK_CLAUDE_BIN` and PATH).
    pub program: Option<PathBuf>,
    /// Arguments before claude's own flags.
    pub program_args: Vec<String>,
    /// Extra environment on top of the macro env. Never logged.
    pub env: BTreeMap<String, String>,
    /// Stop → kill grace; `None` = the driver's default.
    pub stop_grace: Option<Duration>,
}

impl std::fmt::Debug for ClaudeLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeLaunch")
            .field("program", &self.program)
            .field("program_args", &self.program_args)
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("stop_grace", &self.stop_grace)
            .finish()
    }
}

pub fn prompt_for(kind: ChatKind) -> &'static str {
    match kind {
        ChatKind::Macro => MACRO_PROMPT,
        ChatKind::Ask => ASK_PROMPT,
    }
}

/// The binary: the explicit one, then `DESK_CLAUDE_BIN`, then `claude` on
/// PATH (`claude.exe` on Windows — the official installer's native binary;
/// an npm `.cmd` shim cannot be spawned without a shell and is not looked for).
pub fn resolve_claude(
    explicit: Option<&Path>,
    env_bin: Option<OsString>,
    path: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    if let Some(v) = env_bin.filter(|v| !v.is_empty()) {
        return Some(v.into());
    }
    let name = if cfg!(windows) {
        "claude.exe"
    } else {
        "claude"
    };
    std::env::split_paths(&path?)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// [`resolve_claude`] with this process's environment.
pub fn find_claude(launch: &ClaudeLaunch) -> Option<PathBuf> {
    resolve_claude(
        launch.program.as_deref(),
        std::env::var_os(CLAUDE_BIN_ENV),
        std::env::var_os("PATH"),
    )
}

/// The session of one chat: the macro profile with the kind's appendix,
/// cwd = vault, the vault contract in the env, `--resume` when known.
pub fn session_config(
    kind: ChatKind,
    vault: &Path,
    claude: &Path,
    resume: Option<&str>,
    launch: &ClaudeLaunch,
) -> ClaudeConfig {
    let mut env = macro_env_vars(&MacroEnvInput {
        vault: vault.to_string_lossy().into_owned(),
        // TODO(stage 6): the provisioned venv's python and the stored
        // sessionid come from the store; until then the tools use PATH.
        python: String::new(),
        claude: claude.to_string_lossy().into_owned(),
        session_id: None,
        cdp_port: 0,
        platform: Platform::host(),
    });
    env.extend(launch.env.clone());
    let mut cfg = ClaudeConfig::macro_chat(claude, vault, prompt_for(kind), env);
    cfg.program_args = launch.program_args.clone();
    cfg.resume = resume.map(str::to_string);
    if let Some(g) = launch.stop_grace {
        cfg.stop_grace = g;
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_are_the_shipped_files() {
        assert!(MACRO_PROMPT.contains("Strategy Desk"));
        assert!(ASK_PROMPT.contains("a question to the agent"));
        assert_ne!(prompt_for(ChatKind::Macro), prompt_for(ChatKind::Ask));
    }

    #[test]
    fn binary_order_explicit_env_path() {
        let dir = tempfile::tempdir().unwrap();
        let name = if cfg!(windows) {
            "claude.exe"
        } else {
            "claude"
        };
        std::fs::write(dir.path().join(name), b"").unwrap();
        let path = std::env::join_paths([PathBuf::from("/no/such/dir"), dir.path().to_path_buf()])
            .unwrap();
        assert_eq!(
            resolve_claude(None, None, Some(path.clone())),
            Some(dir.path().join(name))
        );
        assert_eq!(
            resolve_claude(None, Some("/bench/fake".into()), Some(path.clone())),
            Some(PathBuf::from("/bench/fake"))
        );
        assert_eq!(
            resolve_claude(None, Some("".into()), Some(path.clone())),
            Some(dir.path().join(name))
        );
        assert_eq!(
            resolve_claude(
                Some(Path::new("/x/c")),
                Some("/bench/fake".into()),
                Some(path)
            ),
            Some(PathBuf::from("/x/c"))
        );
        assert_eq!(resolve_claude(None, None, None), None);
        assert_eq!(
            resolve_claude(None, None, Some("/no/such/dir".into())),
            None
        );
    }

    #[test]
    fn session_config_is_the_macro_profile() {
        let mut launch = ClaudeLaunch::default();
        launch.env.insert("FAKE".into(), "1".into());
        let cfg = session_config(
            ChatKind::Ask,
            Path::new("/v"),
            Path::new("/bin/claude"),
            Some("sess-9"),
            &launch,
        );
        assert_eq!(cfg.cwd, PathBuf::from("/v"));
        assert_eq!(cfg.resume.as_deref(), Some("sess-9"));
        assert_eq!(cfg.append_system_prompt.as_deref(), Some(ASK_PROMPT.trim()));
        assert_eq!(cfg.env.get("MACRO_VAULT").map(String::as_str), Some("/v"));
        assert_eq!(
            cfg.env.get("MACRO_CLAUDE").map(String::as_str),
            Some("/bin/claude")
        );
        assert_eq!(cfg.env.get("FAKE").map(String::as_str), Some("1"));
        assert!(cfg.args().contains(&"--resume=sess-9".to_string()));
        assert!(cfg.allowed_tools.contains(&"Skill".to_string()));
    }
}
