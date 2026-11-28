//! Stage 5 end to end: `AppHandle` → `Command::Chat` → a real child process
//! speaking stream-json → `Snapshot.chats`, checked against
//!
//! * the OLD app's fold of the same streams (rust/fixtures/chat/expected/*.json,
//!   made by rust/fixtures/tools/capture-chat.sh from drivers/claude.ts +
//!   agent.ts + app.tsx) — TEAM.md rule 4, Rust against the old behaviour; and
//! * the TZ / CONTRACTS rules where the old behaviour is a bug (marked
//!   «decided» below and in docs/rust/TEST_PLAN.md, quirks C1–C6).
//!
//! The fake `claude` is this test binary, re-run as
//! `<exe> --exact fake_claude_entry --nocapture -- <claude's flags>` (the
//! pattern of desk-app's own process tests). It learns what to do from
//! `.qa-fake.json` in its cwd — the chat's cwd is the vault, and every test
//! has its own temp vault, so tests running in parallel do not mix.

#[path = "../../desk-core/tests/common/mod.rs"]
mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use common::{fixtures, golden, read_json, str_of};
use serde_json::{json, Value};

const T: Duration = Duration::from_secs(30);

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("ISO date")
}

fn chat_fixtures() -> PathBuf {
    fixtures().join("chat")
}

// =========================================================== the API under test
//
// The only place that names desk-app's stage-5 types. Proposed where the
// contract is silent: `AppConfig::data_dir` (where chats are stored instead of
// %APPDATA%) and `AppConfig::claude` (program + leading args, like
// `ClaudeConfig::program_args`, so a test can run itself as the fake).
mod api {
    use std::path::{Path, PathBuf};

    use chrono::NaiveDate;
    pub use desk_app::chat::{ChatCommand, ChatId, ChatItem, ChatKind, ChatMeta};
    use desk_app::watch::WatchOptions;
    pub use desk_app::{AppConfig, AppHandle, Command, Snapshot};
    use desk_core::windows::WindowId;

    pub struct Launch {
        pub program: PathBuf,
        pub args: Vec<String>,
    }

    pub fn start(vault: &Path, data_dir: &Path, claude: Launch) -> AppHandle {
        AppHandle::start(AppConfig {
            vault: Some(vault.to_path_buf()),
            watch: WatchOptions::default(),
            data_dir: Some(data_dir.to_path_buf()),
            claude: desk_app::chat::ClaudeLaunch {
                program: Some(claude.program),
                program_args: claude.args,
                ..Default::default()
            },
            ..AppConfig::default()
        })
        .expect("start")
    }

    pub fn open_macro(app: &AppHandle, d: NaiveDate) {
        app.send(Command::Chat(ChatCommand::OpenMacro { date: d }));
    }
    pub fn new_ask(app: &AppHandle) {
        app.send(Command::Chat(ChatCommand::New {
            kind: ChatKind::Ask,
        }));
    }
    pub fn send(app: &AppHandle, id: &ChatId, text: &str) {
        app.send(Command::Chat(ChatCommand::Send {
            chat_id: id.clone(),
            text: text.into(),
        }));
    }
    pub fn run(app: &AppHandle, d: NaiveDate) {
        app.send(Command::Chat(ChatCommand::Run {
            window: WindowId::Eu,
            date: d,
        }));
    }
    pub fn stop(app: &AppHandle, id: &ChatId) {
        app.send(Command::Chat(ChatCommand::Stop {
            chat_id: id.clone(),
        }));
    }

    /// The chat on screen.
    pub fn open(s: &Snapshot) -> Option<ChatId> {
        s.chats.open.clone()
    }
    pub fn meta<'a>(s: &'a Snapshot, id: &ChatId) -> Option<&'a ChatMeta> {
        s.chats.list.iter().find(|c| &c.id == id)
    }
    pub fn items(s: &Snapshot, id: &ChatId) -> Vec<ChatItem> {
        s.chats
            .transcripts
            .get(id)
            .map(|t| t.items.clone())
            .unwrap_or_default()
    }
    pub fn is_ask(m: &ChatMeta) -> bool {
        matches!(m.kind, ChatKind::Ask)
    }
}

use api::ChatItem;

// =========================================================== the neutral view
//
// The old screen keeps one assistant message per turn with its tools inside;
// the Rust transcript has User / Assistant / Tool / Notice items. Both are
// reduced to the same per-turn record: what the user sent, the whole answer
// text, the tools (id, name, ok — the one-line summary is new by design), the
// notices by kind, and whether the answer is still marked live.

#[derive(Debug, Clone, PartialEq, Default)]
struct TurnView {
    user: String,
    text: String,
    tools: Vec<(String, String, Option<bool>)>,
    notices: Vec<&'static str>,
    live: bool,
}

/// An old notice text → its kind (texts from ru.ts: chat.stopped, chat.busy,
/// chat.error, notice.retry-after-crash).
fn old_notice_kind(text: &str) -> &'static str {
    match text {
        "Остановлено." => "stopped",
        "Этот чат ещё отвечает — дождитесь завершения." => {
            "busy"
        }
        t if t.starts_with("Процесс Claude упал") => "retry",
        t if t.starts_with("Ошибка") => "error",
        _ => "other",
    }
}

/// A Rust notice key → its kind. Keys: desk-app `claude::keys` (chat.error.*)
/// plus whatever stage 5 adds for stop / busy / retry.
fn rust_notice_kind(key: &str) -> &'static str {
    if key.contains("stop") || key.contains("interrupt") {
        "stopped"
    } else if key.contains("busy") {
        "busy"
    } else if key.contains("retry") {
        "retry"
    } else {
        "error"
    }
}

fn old_view(expected: &Value) -> Vec<TurnView> {
    let mut out: Vec<TurnView> = Vec::new();
    for m in expected["screen"].as_array().expect("screen") {
        match str_of(&m["role"]) {
            "user" => out.push(TurnView {
                user: str_of(&m["text"]).into(),
                ..TurnView::default()
            }),
            "assistant" => {
                let t = out.last_mut().expect("an answer follows a user message");
                t.text.push_str(m["text"].as_str().unwrap_or_default());
                t.live |= m["live"] == Value::Bool(true);
                if let Some(tools) = m["tools"].as_array() {
                    for x in tools {
                        t.tools.push((
                            str_of(&x["id"]).into(),
                            str_of(&x["name"]).into(),
                            x["ok"].as_bool(),
                        ));
                    }
                }
            }
            "notice" => out
                .last_mut()
                .expect("a notice follows a turn")
                .notices
                .push(old_notice_kind(str_of(&m["text"]))),
            other => panic!("unknown role {other}"),
        }
    }
    out
}

fn rust_view(items: &[ChatItem]) -> Vec<TurnView> {
    let mut out: Vec<TurnView> = Vec::new();
    for it in items {
        match it {
            ChatItem::User { text, .. } => out.push(TurnView {
                user: text.clone(),
                ..TurnView::default()
            }),
            ChatItem::Assistant { text, live, .. } => {
                let t = out.last_mut().expect("answer after a user item");
                t.text.push_str(text);
                t.live |= *live;
            }
            ChatItem::Tool { id, name, ok, .. } => out
                .last_mut()
                .expect("tool after a user item")
                .tools
                .push((id.clone(), name.clone(), *ok)),
            ChatItem::Notice { key, .. } => out
                .last_mut()
                .expect("notice after a user item")
                .notices
                .push(rust_notice_kind(key)),
        }
    }
    out
}

// =========================================================== the fake claude

/// What the fake does, written to `<vault>/.qa-fake.json` by the test.
#[derive(Clone)]
struct Fake {
    /// Stream file; one turn (up to and including `result`) per user line.
    stream: PathBuf,
    /// replay | die (write the whole stream — its last line is half a line —
    /// then exit with `exit`) | interrupt (wait for the interrupt request before
    /// line `hold_at`) | hold (wait for a file `<vault>/.qa-go` before line `hold_at`)
    mode: &'static str,
    hold_at: usize,
    exit: i32,
    /// A state file the fake writes into the vault, as the skill's scale.py would.
    agent_writes: Option<&'static str>,
}

impl Fake {
    fn replay(stream: PathBuf) -> Self {
        Fake {
            stream,
            mode: "replay",
            hold_at: 0,
            exit: 0,
            agent_writes: None,
        }
    }
    fn install(&self, vault: &Path, record: &Path) {
        let v = json!({
            "stream": self.stream, "mode": self.mode, "hold_at": self.hold_at,
            "exit": self.exit, "agent_writes": self.agent_writes, "record": record,
        });
        std::fs::write(vault.join(".qa-fake.json"), v.to_string()).expect("fake control file");
    }
}

/// The child side. Runs only when the cwd holds `.qa-fake.json`, i.e. when
/// desk-app started this binary as `claude` inside a test vault.
#[test]
fn fake_claude_entry() {
    let Ok(ctl) = std::fs::read_to_string(".qa-fake.json") else {
        return;
    };
    let ctl: Value = serde_json::from_str(&ctl).expect("control");
    let argv: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
    let record = PathBuf::from(str_of(&ctl["record"]));
    let note = |v: Value| {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&record)
            .expect("record");
        let _ = writeln!(f, "{v}");
    };
    let env: BTreeMap<String, String> = [
        "MACRO_VAULT",
        "PYTHONIOENCODING",
        "TV_CDP_URL",
        "TV_SESSIONID",
        "MACRO_PYTHON",
        "MACRO_CLAUDE",
    ]
    .iter()
    .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
    .collect();
    note(
        json!({"start": true, "argv": argv, "cwd": std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(), "env_keys": env.keys().collect::<Vec<_>>()}),
    );
    // Bytes, not a String: fake_garbage.jsonl carries a line that is not
    // UTF-8 on purpose, and it must reach the desk as it is.
    let text = std::fs::read(str_of(&ctl["stream"])).unwrap_or_default();
    let lines: Vec<&[u8]> = text
        .split(|b| *b == b'\n')
        .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
        .filter(|l| !l.iter().all(u8::is_ascii_whitespace))
        .collect();
    let has = |l: &[u8], pat: &[u8]| l.windows(pat.len()).any(|w| w == pat);
    let mut turns: Vec<Vec<&[u8]>> = vec![Vec::new()];
    for l in &lines {
        turns.last_mut().expect("a turn").push(l);
        if has(l, br#""type":"result""#) {
            turns.push(Vec::new());
        }
    }
    turns.retain(|t| !t.is_empty());
    if turns.is_empty() {
        turns.push(Vec::new());
    }
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stdout().lock();
    let mut read = |want: &str| -> Option<String> {
        loop {
            let mut s = String::new();
            match input.read_line(&mut s) {
                Ok(0) | Err(_) => return None,
                Ok(_) if s.contains(want) => return Some(s),
                Ok(_) => {}
            }
        }
    };
    let mode = str_of(&ctl["mode"]).to_owned();
    let hold_at = ctl["hold_at"].as_u64().unwrap_or(0) as usize;
    let die = mode == "die";
    for (k, turn) in turns.iter().enumerate() {
        let Some(user) = read(r#""type":"user""#) else {
            std::process::exit(0)
        };
        note(json!({"user": user.trim()}));
        if k == 0 {
            if let Some(rel) = ctl["agent_writes"].as_str() {
                // The skill writes state through scale.py; desk-app must not count it as its own.
                let p = Path::new(rel);
                let _ = std::fs::create_dir_all(p.parent().unwrap_or(Path::new(".")));
                let _ = std::fs::write(p, r#"{"written_by":"agent"}"#);
            }
        }
        for (i, l) in turn.iter().enumerate() {
            if die && i + 1 == turn.len() && l.trim_ascii_end().last() != Some(&b'}') {
                // fake_die.jsonl ends in half a line: write it without a newline and die.
                let _ = out.write_all(l);
                let _ = out.flush();
                std::process::exit(ctl["exit"].as_i64().unwrap_or(3) as i32);
            }
            if mode == "interrupt"
                && k == 0
                && i == hold_at
                && read(r#""subtype":"interrupt""#).is_none()
            {
                std::process::exit(0);
            }
            if mode == "hold" && k == 0 && i == hold_at {
                while !Path::new(".qa-go").exists() {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            let _ = out.write_all(l);
            let _ = out.write_all(b"\n");
            let _ = out.flush();
        }
        if die {
            std::process::exit(ctl["exit"].as_i64().unwrap_or(3) as i32);
        }
    }
    let _ = read("\u{0}never");
    std::process::exit(0)
}

fn fake_launch() -> api::Launch {
    api::Launch {
        program: std::env::current_exe().expect("test exe"),
        args: ["--exact", "fake_claude_entry", "--nocapture", "--"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    }
}

fn records(p: &Path) -> Vec<Value> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

// =========================================================== harness

struct Rig {
    _tmp: tempfile::TempDir,
    vault: PathBuf,
    data: PathBuf,
    record: PathBuf,
    app: api::AppHandle,
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read dir").flatten() {
        let t = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t);
        } else {
            std::fs::copy(e.path(), &t).expect("copy");
        }
    }
}

fn rig(fake: &Fake) -> Rig {
    let tmp = tempfile::tempdir().expect("tmp");
    let vault = tmp.path().join("vault");
    copy_dir(&golden().join("vault"), &vault);
    let data = tmp.path().join("appdata");
    let record = tmp.path().join("fake-record.jsonl");
    fake.install(&vault, &record);
    let app = api::start(&vault, &data, fake_launch());
    Rig {
        _tmp: tmp,
        vault,
        data,
        record,
        app,
    }
}

fn wait(app: &api::AppHandle, what: &str, f: impl Fn(&api::Snapshot) -> bool) -> api::Snapshot {
    let deadline = Instant::now() + T;
    loop {
        let s = app.snapshot();
        if f(&s) {
            return s;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn open_chat(r: &Rig, d: &str) -> api::ChatId {
    api::open_macro(&r.app, date(d));
    let s = wait(&r.app, "the macro chat", |s| api::open(s).is_some());
    api::open(&s).expect("open chat")
}

/// Wait until the chat holds `turns` turns, the last one answered (text or a
/// notice), nothing live and the chat not busy. Robust against the moment
/// between a Send and the user item / busy flag appearing.
fn settled_on(app: &api::AppHandle, id: &api::ChatId, turns: usize) -> api::Snapshot {
    wait(app, &format!("{turns} settled turn(s)"), |s| {
        let v = rust_view(&api::items(s, id));
        v.len() == turns
            && v.last()
                .is_some_and(|t| (!t.text.is_empty() || !t.notices.is_empty()) && !t.live)
            && api::meta(s, id).is_some_and(|m| !m.busy)
    })
}

fn settled(r: &Rig, id: &api::ChatId, turns: usize) -> api::Snapshot {
    settled_on(&r.app, id, turns)
}

fn expected(case: &str) -> Value {
    read_json(
        &chat_fixtures()
            .join("expected")
            .join(format!("{case}.json")),
    )
}

fn stream(name: &str) -> PathBuf {
    chat_fixtures().join("streams").join(name)
}

/// Every file of a folder with its bytes, for the vault invariant.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    common::read_vault(root)
}

// =========================================================== against the old app's fold

/// The cases where the old screen is the reference as is (one stream, one or
/// more plain turns). The other expected files are checked by their own tests below.
#[test]
fn transcripts_match_the_old_fold() {
    for (case, streams) in [
        ("text_turn", vec!["text_turn.jsonl"]),
        ("tool_turn", vec!["tool_turn.jsonl"]),
        ("garbage_lines", vec!["fake_garbage.jsonl"]),
        ("fake_tools", vec!["fake_tools.jsonl"]),
    ] {
        let exp = expected(case);
        let r = rig(&Fake::replay(stream(streams[0])));
        let id = open_chat(&r, "2026-11-25");
        for (n, t) in exp["user_turns"]
            .as_array()
            .expect("turns")
            .iter()
            .enumerate()
        {
            api::send(&r.app, &id, str_of(t));
            settled(&r, &id, n + 1);
        }
        let s = r.app.snapshot();
        assert_eq!(
            rust_view(&api::items(&s, &id)),
            old_view(&exp),
            "{case}: transcript = the old screen"
        );
        let m = api::meta(&s, &id).expect("meta");
        assert_eq!(
            m.session_id.as_deref(),
            exp["chat"]["session_id"].as_str(),
            "{case}: session id"
        );
        assert!(
            (m.cost_usd - exp["chat"]["cost_usd"].as_f64().unwrap_or(0.0)).abs() < 1e-9,
            "{case}: cost"
        );
    }
}

/// One macro chat per day, titled by the old rule; the second open is the same chat.
#[test]
fn one_macro_chat_per_day_with_the_old_title() {
    let r = rig(&Fake::replay(stream("text_turn.jsonl")));
    let a = open_chat(&r, "2026-11-25");
    api::open_macro(&r.app, date("2026-11-26"));
    let s = wait(&r.app, "the 26th's chat", |s| {
        api::open(s).is_some_and(|x| x != a)
    });
    let b = api::open(&s).expect("b");
    api::open_macro(&r.app, date("2026-11-25"));
    let s = wait(&r.app, "back to the 25th", |s| {
        api::open(s).as_ref() == Some(&a)
    });
    let titles: Vec<&str> = s.chats.list.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(s.chats.list.len(), 2, "two days, two chats: {titles:?}");
    assert_eq!(
        api::meta(&s, &a).expect("a").title,
        desk_core::prompts::macro_chat_title(date("2026-11-25"))
    );
    assert_eq!(
        api::meta(&s, &a).expect("a").title,
        "Обзор дня 25.11",
        "the old title rule (TEST_PLAN Q9)"
    );
    assert_eq!(api::meta(&s, &b).expect("b").date, Some(date("2026-11-26")));
    assert_eq!(s.chats.list[0].id, b, "newest first");
}

/// «Запустить обзор» = the macro chat of that day + exactly run_command's text.
#[test]
fn run_sends_run_command_exactly() {
    let r = rig(&Fake::replay(stream("tool_turn.jsonl")));
    api::run(&r.app, date("2026-11-25"));
    let s = wait(&r.app, "the run's chat", |s| api::open(s).is_some());
    let id = api::open(&s).expect("id");
    let s = settled(&r, &id, 1);
    let v = rust_view(&api::items(&s, &id));
    assert_eq!(v[0].user, "/macro-review eu 2026-11-25");
    assert_eq!(
        v[0].user,
        desk_core::prompts::run_command(desk_core::windows::WindowId::Eu, date("2026-11-25"))
    );
    let rec = records(&r.record);
    let user = rec
        .iter()
        .find_map(|x| x["user"].as_str())
        .expect("the fake got a user frame");
    assert!(user.contains("/macro-review eu 2026-11-25"), "{user}");
    assert_eq!(api::meta(&s, &id).expect("m").title, "Обзор дня 25.11");
}

/// «+ Новый чат»: kind ask, prompt/ask.md (never a review, never state), cwd = vault,
/// the macro env; title from the first message, ≤ 60 characters (old rule).
#[test]
fn new_chat_is_ask_with_the_ask_prompt() {
    let exp = expected("ask_chat");
    let r = rig(&Fake::replay(stream("text_turn.jsonl")));
    api::new_ask(&r.app);
    let s = wait(&r.app, "the new chat", |s| api::open(s).is_some());
    let id = api::open(&s).expect("id");
    assert!(api::is_ask(api::meta(&s, &id).expect("m")));
    api::send(&r.app, &id, str_of(&exp["user_turns"][0]));
    let s = settled(&r, &id, 1);
    assert_eq!(
        api::meta(&s, &id).expect("m").title,
        str_of(&exp["chat"]["title"]),
        "title = first message, 60 chars"
    );
    let start = records(&r.record)
        .into_iter()
        .find(|x| x["start"] == Value::Bool(true))
        .expect("started");
    let argv: Vec<String> = start["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(|a| str_of(a).to_owned())
        .collect();
    let ask = std::fs::read_to_string(common::fixtures().join("../../prompt/ask.md"))
        .expect("prompt/ask.md");
    let review = std::fs::read_to_string(common::fixtures().join("../../prompt/macro-review.md"))
        .expect("macro-review.md");
    let joined = argv.join("\n");
    assert!(
        joined.contains(ask.trim()),
        "the ask appendix is passed (its rule: no review, no writes into state/)"
    );
    assert!(!joined.contains(review.trim()), "not the review appendix");
    assert!(
        joined.contains("Do not write into `indices-kb/state/`"),
        "the no-state rule is in what the agent gets"
    );
    assert_eq!(
        Path::new(str_of(&start["cwd"])).canonicalize().ok(),
        r.vault.canonicalize().ok(),
        "cwd = vault"
    );
    let keys: Vec<&str> = start["env_keys"]
        .as_array()
        .expect("env")
        .iter()
        .map(str_of)
        .collect();
    for k in ["MACRO_VAULT", "PYTHONIOENCODING", "TV_CDP_URL"] {
        assert!(
            keys.contains(&k),
            "{k} in the env, as the old macroSpec: {keys:?}"
        );
    }
    assert!(
        argv.iter()
            .any(|a| a.contains("Bash") && a.contains("Skill")),
        "MACRO_ALLOWED"
    );
    assert!(
        argv.iter().any(|a| a.contains("Task")),
        "MACRO_DISALLOWED has Task"
    );
}

/// Two turns: the second resumes the first's session; cost = the CLI's running
/// total (the larger value, agent.ts nextCost) = 0.0911.
#[test]
fn second_turn_resumes_and_cost_is_the_running_total() {
    let exp = expected("two_turns_resume_and_cost");
    let both = tempfile::NamedTempFile::new().expect("tmp");
    let mut s = std::fs::read_to_string(stream("text_turn.jsonl")).expect("a");
    s.push_str(&std::fs::read_to_string(stream("tool_turn.jsonl")).expect("b"));
    std::fs::write(both.path(), s).expect("write");
    let r = rig(&Fake::replay(both.path().to_path_buf()));
    let id = open_chat(&r, "2026-11-25");
    for (n, t) in exp["user_turns"]
        .as_array()
        .expect("turns")
        .iter()
        .enumerate()
    {
        api::send(&r.app, &id, str_of(t));
        settled(&r, &id, n + 1);
    }
    let s = r.app.snapshot();
    assert_eq!(rust_view(&api::items(&s, &id)), old_view(&exp));
    let m = api::meta(&s, &id).expect("m");
    assert!((m.cost_usd - 0.0911).abs() < 1e-9, "cost {}", m.cost_usd);
    assert_eq!(m.session_id.as_deref(), exp["chat"]["session_id"].as_str());
}

/// Restart: chats and transcripts come back from data_dir; the next Send
/// starts `claude --resume=<the last session id>`.
#[test]
fn chats_survive_restart_and_resume() {
    let r = rig(&Fake::replay(stream("text_turn.jsonl")));
    let id = open_chat(&r, "2026-11-25");
    api::send(&r.app, &id, "Как открывается Европа?");
    let s = settled(&r, &id, 1);
    let before = rust_view(&api::items(&s, &id));
    let Rig {
        _tmp: keep,
        vault: tmp_vault,
        data,
        record,
        app,
    } = r;
    drop(app); // «Desk closed»: the next AppHandle starts from what is on disk
    assert!(
        std::fs::read_dir(&data).map(|d| d.count()).unwrap_or(0) > 0,
        "something was stored in data_dir"
    );
    let app = api::start(&tmp_vault, &data, fake_launch());
    let s = wait(&app, "the stored chats", |s| api::meta(s, &id).is_some());
    let m = api::meta(&s, &id).expect("m");
    assert_eq!(
        m.session_id.as_deref(),
        Some("5f0c2a8e-1111-4c1e-9a55-000000000001")
    );
    assert!(!m.busy, "a restored chat is idle");
    api::open_macro(&app, date("2026-11-25"));
    let s = wait(&app, "the transcript", |s| !api::items(s, &id).is_empty());
    assert_eq!(
        rust_view(&api::items(&s, &id)),
        before,
        "transcript survives restart"
    );
    api::send(&app, &id, "Продолжим");
    settled_on(&app, &id, 2);
    let starts: Vec<Value> = records(&record)
        .into_iter()
        .filter(|x| x["start"] == Value::Bool(true))
        .collect();
    let last: Vec<&str> = starts.last().expect("restarted")["argv"]
        .as_array()
        .expect("argv")
        .iter()
        .map(str_of)
        .collect();
    assert!(
        last.contains(&"--resume=5f0c2a8e-1111-4c1e-9a55-000000000001"),
        "{last:?}"
    );
    drop(keep);
}

/// «Стоп» mid-answer: the partial text stays, the turn ends, the chat is free.
/// Decided (C2/C4): one «stopped» notice and no error notice; nothing stays live.
#[test]
fn stop_mid_answer() {
    let exp = expected("stop_mid_answer");
    let mut f = Fake::replay(stream("interrupted.jsonl"));
    f.mode = "interrupt";
    f.hold_at = 4;
    let r = rig(&f);
    let id = open_chat(&r, "2026-11-25");
    api::send(&r.app, &id, str_of(&exp["user_turns"][0]));
    wait(&r.app, "the first delta", |s| {
        rust_view(&api::items(s, &id))
            .first()
            .is_some_and(|t| !t.text.is_empty())
    });
    let t0 = Instant::now();
    api::stop(&r.app, &id);
    let s = wait(&r.app, "the stop", |s| {
        api::meta(s, &id).is_some_and(|m| !m.busy)
    });
    assert!(
        t0.elapsed() < Duration::from_secs(3),
        "stop ends the turn within the grace period"
    );
    let v = rust_view(&api::items(&s, &id));
    let old = old_view(&exp);
    assert_eq!(
        v[0].text, old[0].text,
        "the partial answer is kept, as in the old app"
    );
    assert_eq!(
        v[0].notices,
        vec!["stopped"],
        "decided: «Остановлено.» only (old: {:?})",
        old[0].notices
    );
    assert!(
        !v[0].live,
        "decided: nothing stays live (old screen left it live: {})",
        old[0].live
    );
}

/// The process dies mid-answer: partial text kept, one error notice, chat free;
/// the next Send starts a new process with --resume.
#[test]
fn process_death_mid_answer() {
    let exp = expected("process_died");
    let mut f = Fake::replay(stream("fake_die.jsonl"));
    f.mode = "die";
    f.exit = 3;
    let r = rig(&f);
    let id = open_chat(&r, "2026-11-25");
    api::send(&r.app, &id, str_of(&exp["user_turns"][0]));
    let s = settled(&r, &id, 1);
    let v = rust_view(&api::items(&s, &id));
    let old = old_view(&exp);
    assert_eq!(v[0].text, old[0].text, "the partial answer is kept");
    assert_eq!(
        v[0].notices,
        vec!["error"],
        "one error notice (old: «Ошибка: Claude Code process exited with code 3»)"
    );
    assert!(!v[0].live, "decided (C4): the cut answer is not left live");
    assert!(!api::meta(&s, &id).expect("m").busy);
}

/// No claude anywhere: one notice `chat.error.no_claude`, no panic, chat free.
#[test]
fn no_claude_binary() {
    let tmp = tempfile::tempdir().expect("tmp");
    let vault = tmp.path().join("vault");
    copy_dir(&golden().join("vault"), &vault);
    let app = api::start(
        &vault,
        &tmp.path().join("appdata"),
        api::Launch {
            program: tmp.path().join("no-such-claude"),
            args: Vec::new(),
        },
    );
    api::open_macro(&app, date("2026-11-25"));
    let s = wait(&app, "the chat", |s| api::open(s).is_some());
    let id = api::open(&s).expect("id");
    api::send(&app, &id, "Привет");
    let s = wait(&app, "the notice", |s| {
        api::items(s, &id)
            .iter()
            .any(|i| matches!(i, ChatItem::Notice { .. }))
    });
    let keys: Vec<String> = api::items(&s, &id)
        .iter()
        .filter_map(|i| {
            if let ChatItem::Notice { key, .. } = i {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(keys, vec!["chat.error.no_claude".to_string()]);
    assert!(!api::meta(&s, &id).expect("m").busy);
}

/// A busy chat does not take a Send: the refused text is not in the transcript,
/// the running answer is not split (decided, C5; the old screen split it and
/// kept the refused user message).
#[test]
fn busy_chat_rejects_send() {
    let exp = expected("busy");
    let mut f = Fake::replay(stream("text_turn.jsonl"));
    f.mode = "hold";
    f.hold_at = 6;
    let r = rig(&f);
    let id = open_chat(&r, "2026-11-25");
    api::send(&r.app, &id, "Как открывается Европа?");
    wait(&r.app, "busy", |s| {
        api::meta(s, &id).is_some_and(|m| m.busy)
    });
    api::send(&r.app, &id, "И ещё вопрос");
    std::thread::sleep(Duration::from_millis(100));
    std::fs::write(r.vault.join(".qa-go"), b"").expect("go");
    let s = settled(&r, &id, 1);
    let v = rust_view(&api::items(&s, &id));
    assert_eq!(v.len(), 1, "one turn only: {v:?}");
    assert_eq!(
        v[0].text, "Привет! Европа открывается спокойно.",
        "the answer is whole"
    );
    let old: Vec<String> = old_view(&exp).iter().map(|t| t.user.clone()).collect();
    assert_eq!(
        old,
        vec!["Как открывается Европа?", "И ещё вопрос"],
        "for the record: the old screen kept the refused message"
    );
    assert_eq!(
        records(&r.record)
            .iter()
            .filter(|x| x["user"].is_string())
            .count(),
        1,
        "the fake got one user frame"
    );
    let _ = std::fs::remove_file(r.vault.join(".qa-go"));
}

/// Decided (C1): a login failure is shown (the old screen showed nothing).
#[test]
fn auth_error_is_visible() {
    let exp = expected("auth_error");
    assert_eq!(
        old_view(&exp)[0].notices,
        Vec::<&str>::new(),
        "for the record: the old app was silent"
    );
    let r = rig(&Fake::replay(stream("auth_error.jsonl")));
    let id = open_chat(&r, "2026-11-25");
    api::send(&r.app, &id, "Привет");
    let s = settled(&r, &id, 1);
    assert_eq!(rust_view(&api::items(&s, &id))[0].notices, vec!["error"]);
}

/// 10 000 lines (the SYNTH_10K rule of rust/fixtures/tools/chat/chat-fold.ts):
/// the same answer as the old fold, no lost line, and the snapshot does not
/// change more than ~30 times a second while it streams.
#[test]
fn ten_thousand_events() {
    let exp = expected("stream_10k");
    let mut lines = vec![r#"{"type":"system","subtype":"init","session_id":"qa-10k","model":"claude-fake","claude_code_version":"0.0.0-qa","tools":["Bash"],"mcp_servers":[]}"#.to_string()];
    for i in 0..10_000 {
        if i % 50 == 49 {
            lines.push(format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"toolu_{i}","name":"Bash","input":{{"command":"echo {i}"}}}}]}},"parent_tool_use_id":null,"session_id":"qa-10k"}}"#));
            lines.push(format!(r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"toolu_{i}","content":"{i}","is_error":{}}}]}},"parent_tool_use_id":null,"session_id":"qa-10k"}}"#, i % 500 == 499));
        } else {
            lines.push(format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"w{i} "}}}},"parent_tool_use_id":null,"session_id":"qa-10k"}}"#));
        }
    }
    lines.push(r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"session_id":"qa-10k","total_cost_usd":1.2345,"usage":{"input_tokens":1000,"output_tokens":10000}}"#.into());
    let body = lines.join("\n") + "\n";
    let f = tempfile::NamedTempFile::new().expect("tmp");
    std::fs::write(f.path(), &body).expect("write");
    assert_eq!(
        sha256_hex(body.as_bytes()),
        str_of(&exp["streams"][0]["sha256"]),
        "the generated stream is the one the old fold read"
    );
    let r = rig(&Fake::replay(f.path().to_path_buf()));
    let id = open_chat(&r, "2026-11-25");
    let rev0 = r.app.snapshot().rev;
    let t0 = Instant::now();
    api::send(&r.app, &id, "/macro-review eu 2026-11-25");
    let s = settled(&r, &id, 1);
    let secs = t0.elapsed().as_secs_f64();
    let v = rust_view(&api::items(&s, &id));
    let want = &exp["screen"][1];
    assert_eq!(
        sha256_hex(v[0].text.as_bytes()),
        str_of(&want["text"]["sha256"]),
        "same answer text, every delta"
    );
    assert_eq!(
        v[0].tools.len() as u64,
        want["tools"]["count"].as_u64().unwrap_or(0)
    );
    assert_eq!(
        v[0].tools.iter().filter(|t| t.2 == Some(true)).count() as u64,
        want["tools"]["ok"].as_u64().unwrap_or(0)
    );
    let revs = (s.rev - rev0) as f64;
    assert!(
        revs <= 30.0 * secs + 10.0,
        "{revs} snapshots in {secs:.2} s: more than 30 per second"
    );
    assert!((api::meta(&s, &id).expect("m").cost_usd - 1.2345).abs() < 1e-9);
}

/// The «only two files» invariant while chatting: the agent writes state (as
/// scale.py would), the desk writes nothing into the vault — chats live in data_dir.
#[test]
fn chatting_writes_nothing_into_the_vault() {
    let mut f = Fake::replay(stream("tool_turn.jsonl"));
    f.agent_writes = Some("indices-kb/state/scale_2026-11-25_eu.json");
    let r = rig(&f);
    let before = tree(&r.vault);
    assert!(before.contains_key(".qa-fake.json"));
    api::run(&r.app, date("2026-11-25"));
    let s = wait(&r.app, "the run's chat", |s| api::open(s).is_some());
    let id = api::open(&s).expect("id");
    settled(&r, &id, 1);
    api::new_ask(&r.app);
    let s = wait(&r.app, "the ask chat", |s| {
        api::open(s).is_some_and(|x| x != id)
    });
    let ask = api::open(&s).expect("ask");
    api::send(&r.app, &ask, "Какой урок повторяется?");
    settled(&r, &ask, 1);
    let Rig {
        _tmp: keep,
        vault,
        data,
        app,
        ..
    } = r;
    drop(app);
    let after = tree(&vault);
    let mut changed: Vec<&String> = after
        .keys()
        .filter(|k| before.get(*k) != after.get(*k))
        .collect();
    changed.extend(before.keys().filter(|k| !after.contains_key(*k)));
    changed.retain(|k| k.as_str() != "indices-kb/state/scale_2026-11-25_eu.json"); // the agent's own write
    assert!(
        changed.is_empty(),
        "the desk changed the vault while chatting: {changed:?}"
    );
    assert!(
        std::fs::read_dir(&data).map(|d| d.count()).unwrap_or(0) > 0,
        "chats went to data_dir"
    );
    drop(keep);
}

fn sha256_hex(bytes: &[u8]) -> String {
    // Small, dependency-free SHA-256 (FIPS 180-4) for the fixture checks.
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = bytes.to_vec();
    let bits = (bytes.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, x) in w.iter_mut().take(16).enumerate() {
            *x = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for (ki, wi) in k.iter().zip(w.iter()) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*ki)
                .wrapping_add(*wi);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (hi, vi) in h.iter_mut().zip(v) {
            *hi = hi.wrapping_add(vi);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
