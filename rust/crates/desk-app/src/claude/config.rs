//! What a Claude session is started with.
//!
//! The flags mirror what the Agent SDK 0.3.278 passes for the same options
//! (`ProcessTransport.initialize` in `sdk.mjs`), so a session started here
//! behaves like one the Electron app started through `query()`.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// What a macro chat may do: the skill runs its own scripts and writes its own
/// state files — the same session the Task Scheduler opens, minus subagents.
/// Port of `MACRO_ALLOWED` in `src/main/drivers/claude.ts`.
pub const MACRO_ALLOWED: &[&str] = &[
    "Bash",
    "PowerShell",
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "Skill",
    "WebSearch",
    "WebFetch",
    "TodoWrite",
    "mcp__tradingview",
    "mcp__mcp-tradingview",
    "mcp__gt-database",
];

/// Port of `MACRO_DISALLOWED`.
pub const MACRO_DISALLOWED: &[&str] = &["Task", "NotebookEdit"];

/// Auth variables the desk's own environment must never leak into a chat
/// (`buildEnv` in claude.ts): the subscription login in `~/.claude` is used.
pub const AUTH_ENV_REMOVED: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
];

/// Everything needed to start one `claude -p` process.
///
/// `Debug` prints env keys only: values carry the TradingView sessionid.
#[derive(Clone)]
pub struct ClaudeConfig {
    /// The executable (`claude`, or the full path the status probe found).
    pub program: PathBuf,
    /// Arguments placed before the driver's own flags (tests use this to run
    /// a fake; production leaves it empty).
    pub program_args: Vec<String>,
    /// Working directory: the vault for a macro or ask chat.
    pub cwd: PathBuf,
    /// Variables set on top of the inherited environment (the macro env).
    pub env: BTreeMap<String, String>,
    /// Variables removed from the inherited environment before `env` applies.
    pub env_remove: Vec<String>,
    /// Conversation to continue (`--resume=<id>`).
    pub resume: Option<String>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    /// Appended to Claude Code's preset prompt (`prompt/macro-review.md`, `prompt/ask.md`).
    pub append_system_prompt: Option<String>,
    /// `--permission-mode`; `dontAsk` for a macro chat (deny what is not allowed).
    pub permission_mode: Option<String>,
    pub max_turns: Option<u32>,
    /// Empty = the CLI's default model.
    pub model: Option<String>,
    /// `--mcp-config` JSON (`{"mcpServers":{…}}`), for `macroMcp: 'bundled'`.
    pub mcp_config: Option<String>,
    /// Grace period between asking the process to stop and killing it.
    pub stop_grace: Duration,
}

impl ClaudeConfig {
    /// A session with no tools policy, no prompt and the default grace.
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            program_args: Vec::new(),
            cwd: cwd.into(),
            env: BTreeMap::new(),
            env_remove: Vec::new(),
            resume: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            append_system_prompt: None,
            permission_mode: None,
            max_turns: None,
            model: None,
            mcp_config: None,
            stop_grace: Duration::from_millis(1500),
        }
    }

    /// The macro / ask profile (`optionsFor` with `profile: 'macro'`): cwd =
    /// vault, `MACRO_ALLOWED`, `dontAsk`, 200 turns, the appendix, the macro env
    /// over an auth-clean inherited env.
    pub fn macro_chat(
        program: impl Into<PathBuf>,
        vault: impl Into<PathBuf>,
        appendix: &str,
        macro_env: BTreeMap<String, String>,
    ) -> Self {
        let mut cfg = Self::new(program, vault);
        // `skills: 'all'` in the SDK only adds "Skill" to the allowed list,
        // which MACRO_ALLOWED already has.
        cfg.allowed_tools = MACRO_ALLOWED.iter().map(|s| s.to_string()).collect();
        cfg.disallowed_tools = MACRO_DISALLOWED.iter().map(|s| s.to_string()).collect();
        cfg.permission_mode = Some("dontAsk".into());
        cfg.max_turns = Some(200);
        let appendix = appendix.trim();
        if !appendix.is_empty() {
            cfg.append_system_prompt = Some(appendix.to_string());
        }
        cfg.env_remove = AUTH_ENV_REMOVED.iter().map(|s| s.to_string()).collect();
        cfg.env = macro_env;
        cfg.env.insert("DISABLE_AUTOUPDATER".into(), "1".into());
        cfg
    }

    /// The command line after `program_args`.
    ///
    /// List flags are passed comma-joined in ONE argument, as the SDK does:
    /// the CLI declares them variadic (`<tools...>`), and a separate word per
    /// tool would swallow whatever flag-less argument came next.
    pub fn args(&self) -> Vec<String> {
        let mut a: Vec<String> = [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            // Without it there are no `stream_event` frames, so no TextDelta
            // until a whole content block is done.
            "--include-partial-messages",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if let Some(id) = self.resume.as_deref().filter(|s| !s.is_empty()) {
            // `=` form: `-r/--resume [value]` takes an OPTIONAL value.
            a.push(format!("--resume={id}"));
        }
        if let Some(m) = self
            .model
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            a.push("--model".into());
            a.push(m.to_string());
        }
        if let Some(n) = self.max_turns {
            a.push("--max-turns".into());
            a.push(n.to_string());
        }
        if !self.allowed_tools.is_empty() {
            a.push("--allowedTools".into());
            a.push(self.allowed_tools.join(","));
        }
        if !self.disallowed_tools.is_empty() {
            a.push("--disallowedTools".into());
            a.push(self.disallowed_tools.join(","));
        }
        if let Some(mcp) = &self.mcp_config {
            a.push("--mcp-config".into());
            a.push(mcp.clone());
        }
        if let Some(mode) = &self.permission_mode {
            a.push("--permission-mode".into());
            a.push(mode.clone());
        }
        if let Some(p) = &self.append_system_prompt {
            a.push("--append-system-prompt".into());
            a.push(p.clone());
        }
        a
    }
}

impl fmt::Debug for ClaudeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeConfig")
            .field("program", &self.program)
            .field("program_args", &self.program_args)
            .field("cwd", &self.cwd)
            // Keys only: TV_SESSIONID's value must never reach a log.
            .field("env_keys", &self.env.keys().collect::<Vec<_>>())
            .field("env_remove", &self.env_remove)
            .field("resume", &self.resume)
            .field("allowed_tools", &self.allowed_tools)
            .field("disallowed_tools", &self.disallowed_tools)
            .field(
                "append_system_prompt_len",
                &self.append_system_prompt.as_ref().map(String::len),
            )
            .field("permission_mode", &self.permission_mode)
            .field("max_turns", &self.max_turns)
            .field("model", &self.model)
            .field("mcp_config", &self.mcp_config.is_some())
            .field("stop_grace", &self.stop_grace)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_profile_flags() {
        let mut env = BTreeMap::new();
        env.insert(
            "TV_SESSIONID".to_string(),
            "secret-cookie-value".to_string(),
        );
        let mut cfg = ClaudeConfig::macro_chat("claude", "/v", "  appendix text \n", env);
        cfg.resume = Some("abc-123".into());
        let a = cfg.args();
        assert_eq!(
            &a[..7],
            &[
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--include-partial-messages"
            ]
        );
        assert!(a.contains(&"--resume=abc-123".to_string()));
        let i = a.iter().position(|x| x == "--allowedTools").unwrap();
        assert_eq!(a[i + 1], MACRO_ALLOWED.join(","));
        let i = a.iter().position(|x| x == "--disallowedTools").unwrap();
        assert_eq!(a[i + 1], "Task,NotebookEdit");
        let i = a.iter().position(|x| x == "--permission-mode").unwrap();
        assert_eq!(a[i + 1], "dontAsk");
        let i = a
            .iter()
            .position(|x| x == "--append-system-prompt")
            .unwrap();
        assert_eq!(a[i + 1], "appendix text");
        let i = a.iter().position(|x| x == "--max-turns").unwrap();
        assert_eq!(a[i + 1], "200");
        assert_eq!(
            cfg.env.get("DISABLE_AUTOUPDATER").map(String::as_str),
            Some("1")
        );
        assert!(cfg.env_remove.contains(&"ANTHROPIC_API_KEY".to_string()));
    }

    #[test]
    fn debug_never_prints_env_values() {
        let mut env = BTreeMap::new();
        env.insert(
            "TV_SESSIONID".to_string(),
            "secret-cookie-value".to_string(),
        );
        let cfg = ClaudeConfig::macro_chat("claude", "/v", "", env);
        let s = format!("{cfg:?}");
        assert!(s.contains("TV_SESSIONID"));
        assert!(!s.contains("secret-cookie-value"));
        assert!(cfg.append_system_prompt.is_none());
        assert!(!cfg.args().contains(&"--append-system-prompt".to_string()));
    }

    #[test]
    fn empty_resume_and_model_are_left_out() {
        let mut cfg = ClaudeConfig::new("claude", "/v");
        cfg.resume = Some(String::new());
        cfg.model = Some("  ".into());
        let a = cfg.args();
        assert!(!a.iter().any(|x| x.starts_with("--resume")));
        assert!(!a.contains(&"--model".to_string()));
        assert_eq!(a.len(), 7);
    }
}
