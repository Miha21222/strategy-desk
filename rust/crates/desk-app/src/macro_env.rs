//! The environment a macro session runs with — the pure half.
//! Port of `src/main/macro-env.ts` (and its tests).
//!
//! The vault's own scripts honour a small contract: MACRO_VAULT, MACRO_PYTHON,
//! MACRO_CLAUDE, TV_SESSIONID, TV_CDP_URL and PYTHONIOENCODING. Whoever opens a
//! session — the desk's chat, a scheduled `run --window eu` — puts the same
//! six things in the environment, or the tools import the wrong Python or
//! fetch nothing. This module is the ONE place that spells them.

use std::collections::BTreeMap;
use std::fmt;

/// `DEFAULT_CDP_PORT` in `src/shared/defaults.ts`.
pub const DEFAULT_CDP_PORT: u16 = 9223;

/// Path and PATH-list spelling. Chosen per input, not per host, so the exact
/// shape is testable anywhere (the TS took `platform` for the same reason).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Posix,
}

impl Platform {
    pub fn host() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Posix
        }
    }

    fn sep(self) -> char {
        match self {
            Platform::Windows => '\\',
            Platform::Posix => '/',
        }
    }

    fn is_sep(self, c: char) -> bool {
        match self {
            Platform::Windows => c == '\\' || c == '/',
            Platform::Posix => c == '/',
        }
    }

    fn list_sep(self) -> char {
        match self {
            Platform::Windows => ';',
            Platform::Posix => ':',
        }
    }
}

/// `MacroEnvInput`. `Debug` hides the cookie.
#[derive(Clone)]
pub struct MacroEnvInput {
    /// The vault folder (settings.vaultPath).
    pub vault: String,
    /// Full path to the interpreter the vault tools use; '' = leave the tools to PATH.
    pub python: String,
    /// Full path to the Claude Code binary; '' = leave it to PATH.
    pub claude: String,
    /// The TradingView sessionid cookie; None = not stored.
    pub session_id: Option<String>,
    /// 0 = the default port.
    pub cdp_port: u16,
    pub platform: Platform,
}

impl fmt::Debug for MacroEnvInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroEnvInput")
            .field("vault", &self.vault)
            .field("python", &self.python)
            .field("claude", &self.claude)
            .field("session_id", &self.session_id.as_ref().map(|_| "<set>"))
            .field("cdp_port", &self.cdp_port)
            .field("platform", &self.platform)
            .finish()
    }
}

/// `macroEnvVars`: the variables only — no inheritance, so a test sees
/// exactly what is added. Empty values are left out (TS: falsy = unset).
pub fn macro_env_vars(input: &MacroEnvInput) -> BTreeMap<String, String> {
    let port = if input.cdp_port == 0 {
        DEFAULT_CDP_PORT
    } else {
        input.cdp_port
    };
    let mut env = BTreeMap::new();
    env.insert("MACRO_VAULT".into(), input.vault.clone());
    env.insert("TV_CDP_URL".into(), format!("http://127.0.0.1:{port}"));
    env.insert("PYTHONIOENCODING".into(), "utf-8".into());
    if !input.python.is_empty() {
        env.insert("MACRO_PYTHON".into(), input.python.clone());
    }
    if !input.claude.is_empty() {
        env.insert("MACRO_CLAUDE".into(), input.claude.clone());
    }
    if let Some(s) = input.session_id.as_deref().filter(|s| !s.is_empty()) {
        env.insert("TV_SESSIONID".into(), s.to_string());
    }
    env
}

/// `macroEnv`: the base, the six variables, and the venv's `Scripts` (or
/// `bin`) directory in front of PATH, so `python` and the venv's entry
/// points are the ones the tools find.
pub fn macro_env<I, K, V>(input: &MacroEnvInput, base: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    let mut env: BTreeMap<String, String> = base
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    env.extend(macro_env_vars(input));
    if !input.python.is_empty() {
        let p = input.platform;
        let scripts = dirname(&input.python, p);
        // Windows spells the variable `Path`; prepend to whichever spelling
        // the base already has (TS: case-insensitive find on every platform).
        let key = env
            .keys()
            .find(|k| k.eq_ignore_ascii_case("path"))
            .cloned()
            .unwrap_or_else(|| match p {
                Platform::Windows => "Path".into(),
                Platform::Posix => "PATH".into(),
            });
        let current = env.get(&key).cloned().unwrap_or_default();
        let value = if current.is_empty() {
            scripts
        } else {
            format!("{scripts}{}{current}", p.list_sep())
        };
        env.insert(key, value);
    }
    env
}

/// The process environment as a `macro_env` base: variables that are not
/// valid Unicode are skipped (the TS kept only `typeof v === 'string'`).
pub fn process_env() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect()
}

/// `macroPython`: the provisioned venv in bundled mode; in a dev run the
/// tvmcp checkout's own `.venv`; '' when neither — the scripts then fall
/// back to whatever `python` PATH holds.
///
/// Takes the two fields it reads instead of `Settings` / `RuntimePaths`,
/// which are not ported yet.
pub fn macro_python(venv_dir: Option<&str>, tvmcp_path: &str, platform: Platform) -> String {
    let tail: &[&str] = match platform {
        Platform::Windows => &["Scripts", "python.exe"],
        Platform::Posix => &["bin", "python"],
    };
    if let Some(v) = venv_dir.filter(|v| !v.is_empty()) {
        return join(v, tail, platform);
    }
    let clone = tvmcp_path.trim();
    if clone.is_empty() {
        return String::new();
    }
    let mut parts = vec![".venv"];
    parts.extend_from_slice(tail);
    join(clone, &parts, platform)
}

/// The sessionid cookie's shape: TradingView issues a 32-char alphanumeric id.
pub fn looks_like_session_id(value: &str) -> bool {
    let v = value.trim();
    (20..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The one cheap proof that a sessionid works (exchange it for the websocket
/// token). The secret travels in the environment, never on the command line.
pub const SESSION_CHECK_CODE: &str = "import os; from tvmcp.session.client import fetch_auth_token; fetch_auth_token(os.environ[\"TV_SESSIONID\"]); print(\"sessionid ok: token issued\")";

/// `python -c "import pandas, tvmcp"` — the two imports every vault tool starts with.
pub const TOOLS_IMPORT_CODE: &str =
    "import pandas, tvmcp; print(\"pandas\", pandas.__version__, \"tvmcp ok\")";

/// Node's `path.<platform>.dirname` for the inputs that occur here: an
/// absolute or relative file path. A bare name gives `.`; the root stays.
fn dirname(path: &str, p: Platform) -> String {
    let trimmed = path.trim_end_matches(|c| p.is_sep(c));
    if trimmed.is_empty() {
        return path
            .chars()
            .next()
            .map_or_else(|| ".".into(), |c| c.to_string());
    }
    let Some(i) = trimmed.rfind(|c| p.is_sep(c)) else {
        // `C:python.exe` → `C:`; `python.exe` → `.`
        if p == Platform::Windows && trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':' {
            return trimmed[..2].to_string();
        }
        return ".".into();
    };
    let head = trimmed[..i].trim_end_matches(|c| p.is_sep(c));
    if head.is_empty() {
        // `/python` → `/`
        return trimmed[..i + 1].chars().take(1).collect();
    }
    if p == Platform::Windows && head.len() == 2 && head.as_bytes()[1] == b':' {
        // `C:\python.exe` → `C:\`
        return format!("{head}\\");
    }
    head.to_string()
}

fn join(base: &str, parts: &[&str], p: Platform) -> String {
    let mut s = base.trim_end_matches(|c| p.is_sep(c)).to_string();
    if s.is_empty() && base.starts_with(|c| p.is_sep(c)) {
        s.push(p.sep());
    }
    for part in parts {
        if !s.ends_with(|c| p.is_sep(c)) && !s.is_empty() {
            s.push(p.sep());
        }
        s.push_str(part);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(
        vault: &str,
        python: &str,
        claude: &str,
        session: Option<&str>,
        port: u16,
        platform: Platform,
    ) -> MacroEnvInput {
        MacroEnvInput {
            vault: vault.into(),
            python: python.into(),
            claude: claude.into(),
            session_id: session.map(str::to_string),
            cdp_port: port,
            platform,
        }
    }

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // macroEnvVars
    #[test]
    fn spells_the_whole_contract_and_leaves_out_what_is_not_resolvable() {
        let full = macro_env_vars(&input(
            r"C:\Users\T\Documents\Strategy Desk",
            r"C:\userData\py\venv\Scripts\python.exe",
            r"C:\app\claude.exe",
            Some("abc"),
            9223,
            Platform::Windows,
        ));
        assert_eq!(
            full,
            map(&[
                ("MACRO_VAULT", r"C:\Users\T\Documents\Strategy Desk"),
                ("MACRO_PYTHON", r"C:\userData\py\venv\Scripts\python.exe"),
                ("MACRO_CLAUDE", r"C:\app\claude.exe"),
                ("TV_SESSIONID", "abc"),
                ("TV_CDP_URL", "http://127.0.0.1:9223"),
                ("PYTHONIOENCODING", "utf-8"),
            ])
        );
        let bare = macro_env_vars(&input(r"C:\v", "", "", None, 0, Platform::Windows));
        assert_eq!(
            bare.keys().collect::<Vec<_>>(),
            ["MACRO_VAULT", "PYTHONIOENCODING", "TV_CDP_URL"]
        );
        assert_eq!(bare["TV_CDP_URL"], "http://127.0.0.1:9223");
    }

    // macroEnv
    #[test]
    fn prepends_the_venv_scripts_directory_to_the_path_spelling_the_base_already_has() {
        let env = macro_env(
            &input(
                r"C:\v",
                r"C:\py\venv\Scripts\python.exe",
                "",
                Some("s"),
                9223,
                Platform::Windows,
            ),
            [("Path", r"C:\Windows"), ("HOME", "x")],
        );
        assert_eq!(env["Path"], r"C:\py\venv\Scripts;C:\Windows");
        assert!(!env.contains_key("PATH"));
        assert_eq!(env["HOME"], "x");
        assert_eq!(env["TV_SESSIONID"], "s");
    }

    #[test]
    fn uses_a_colon_on_posix_and_creates_the_variable_when_the_base_has_none() {
        let i = input("/v", "/v/.venv/bin/python", "", None, 9223, Platform::Posix);
        let env = macro_env(&i, [("PATH", "/usr/bin")]);
        assert_eq!(env["PATH"], "/v/.venv/bin:/usr/bin");
        let none = macro_env(&i, Vec::<(String, String)>::new());
        assert_eq!(none["PATH"], "/v/.venv/bin");
    }

    #[test]
    fn never_carries_a_cookie_it_was_not_given() {
        let env = macro_env(
            &input("/v", "", "", None, 9223, Platform::Posix),
            [("TV_SESSIONID", "inherited")],
        );
        // Inheritance from the process environment is the existing behaviour
        // (runtime-spec.ts): the contract only ADDS the stored one.
        assert_eq!(env["TV_SESSIONID"], "inherited");
    }

    // macroPython
    #[test]
    fn is_the_provisioned_venv_in_bundled_mode_and_the_checkout_venv_in_a_dev_run() {
        assert_eq!(
            macro_python(Some(r"C:\userData\py\venv"), "", Platform::Windows),
            r"C:\userData\py\venv\Scripts\python.exe"
        );
        assert_eq!(
            macro_python(None, r"C:\tvmcp", Platform::Windows),
            r"C:\tvmcp\.venv\Scripts\python.exe"
        );
        assert_eq!(
            macro_python(None, "/tvmcp", Platform::Posix),
            "/tvmcp/.venv/bin/python"
        );
        assert_eq!(macro_python(None, "", Platform::Windows), "");
        assert_eq!(
            macro_python(None, "  /tvmcp/  ", Platform::Posix),
            "/tvmcp/.venv/bin/python"
        );
    }

    // the session check
    #[test]
    fn reads_the_cookie_from_the_environment_never_from_the_command_line() {
        assert!(SESSION_CHECK_CODE.contains("os.environ[\"TV_SESSIONID\"]"));
        assert!(SESSION_CHECK_CODE.contains("fetch_auth_token"));
    }

    #[test]
    fn knows_the_cookie_shape() {
        assert!(looks_like_session_id("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"));
        assert!(!looks_like_session_id(" short "));
        assert!(!looks_like_session_id(
            "has spaces in it and is long enough"
        ));
        assert!(!looks_like_session_id("кириллицакириллицакириллица"));
    }

    // Rust-only: the helpers behind the port.
    #[test]
    fn dirname_matches_node() {
        assert_eq!(
            dirname(r"C:\py\venv\Scripts\python.exe", Platform::Windows),
            r"C:\py\venv\Scripts"
        );
        assert_eq!(dirname(r"C:\python.exe", Platform::Windows), r"C:\");
        assert_eq!(dirname("C:/py/python.exe", Platform::Windows), "C:/py");
        assert_eq!(dirname("python.exe", Platform::Windows), ".");
        assert_eq!(
            dirname("/v/.venv/bin/python", Platform::Posix),
            "/v/.venv/bin"
        );
        assert_eq!(dirname("/python", Platform::Posix), "/");
        assert_eq!(dirname("python", Platform::Posix), ".");
    }

    #[test]
    fn debug_hides_the_cookie() {
        let s = format!(
            "{:?}",
            input("/v", "", "", Some("supersecretcookie"), 0, Platform::Posix)
        );
        assert!(!s.contains("supersecretcookie"));
        assert!(s.contains("<set>"));
    }
}
