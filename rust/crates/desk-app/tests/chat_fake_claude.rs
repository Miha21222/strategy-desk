//! A whole chat turn through `AppHandle` against a fake `claude`.
//!
//! The fake is this test binary re-run with `--exact fake_claude_entry`
//! (no cross-crate binary to locate, works on Linux and Windows). It speaks
//! the same stream as desk-bench's `fake-claude`: lines from
//! `desk_bench::stream::{init_line, turn}`, one turn per user line on stdin,
//! `FAKE_CLAUDE_EVENTS` / `_RATE` / `_DIE_AFTER` as there. It ignores
//! interrupts, like a CLI that does not answer in time, so «Стоп» here is
//! the kill-after-grace path.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use desk_app::chat::model::keys;
use desk_app::chat::ClaudeLaunch;
use desk_app::claude::keys as driver_keys;
use desk_app::{AppConfig, AppHandle, ChatCommand, ChatItem, ChatKind, Command, Snapshot};
use desk_bench::stream::{init_line, session_id, turn, StreamSpec};

const FAKE: &str = "DESK_TEST_FAKE_CLAUDE";
const ARGV_OUT: &str = "DESK_TEST_FAKE_ARGV";
const SEED: u64 = 23;

#[test]
fn fake_claude_entry() {
    if std::env::var_os(FAKE).is_some() {
        fake_main();
    }
}

fn env_num(k: &str, d: u64) -> u64 {
    std::env::var(k)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(d)
}

fn fake_main() -> ! {
    if let Ok(p) = std::env::var(ARGV_OUT) {
        let argv: Vec<String> = std::env::args().skip_while(|a| a != "--").skip(1).collect();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            let _ = writeln!(f, "{}", serde_json::to_string(&argv).unwrap_or_default());
        }
    }
    // FAKE_RESUME_MISSING: answer a --resume like claude 2.1.332 does for a
    // conversation it does not have (recorded: a startup result, exit 1).
    if std::env::var_os("FAKE_RESUME_MISSING").is_some() {
        if let Some(id) =
            std::env::args().find_map(|a| a.strip_prefix("--resume=").map(str::to_owned))
        {
            println!(
                r#"{{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":0,"session_id":"{id}","total_cost_usd":0,"usage":{{"input_tokens":0,"output_tokens":0}},"errors":["No conversation found with session ID: {id}"]}}"#
            );
            std::process::exit(1);
        }
    }
    let spec = StreamSpec {
        events: env_num("FAKE_CLAUDE_EVENTS", 200) as usize,
        seed: SEED,
        tool_every: 4,
    };
    let rate = env_num("FAKE_CLAUDE_RATE", 0) as f64;
    let die_after = env_num("FAKE_CLAUDE_DIE_AFTER", 0) as usize;
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mut out = std::io::stdout().lock();
    let mut written = 0usize;
    let mut n = 0usize;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if !line.contains(r#""type":"user""#) {
            continue; // control requests (interrupt) are not answered
        }
        let mut lines = Vec::new();
        if n == 0 {
            lines.push(init_line(SEED, &cwd));
        }
        lines.extend(turn(&spec, n, "2026-11-26"));
        n += 1;
        let start = Instant::now();
        for (i, l) in lines.iter().enumerate() {
            if die_after > 0 && written >= die_after {
                let _ = out.write_all(&l.as_bytes()[..l.len() / 2]);
                let _ = out.flush();
                std::process::exit(3);
            }
            if rate > 0.0 {
                let due = Duration::from_secs_f64(i as f64 / rate);
                if let Some(wait) = due.checked_sub(start.elapsed()) {
                    let _ = out.flush();
                    std::thread::sleep(wait);
                }
            }
            let _ = writeln!(out, "{l}");
            written += 1;
        }
        let _ = out.flush();
    }
    std::process::exit(0)
}

struct Rig {
    app: AppHandle,
    _vault: tempfile::TempDir,
    data: PathBuf,
    argv: PathBuf,
    repaints: Arc<AtomicU64>,
}

fn launch(env: &[(&str, String)], argv: &Path) -> ClaudeLaunch {
    let mut l = ClaudeLaunch {
        program: Some(std::env::current_exe().expect("test exe")),
        program_args: [
            "--exact",
            "fake_claude_entry",
            "--nocapture",
            "--test-threads=1",
            "-q",
            "--",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        stop_grace: Some(Duration::from_millis(300)),
        ..ClaudeLaunch::default()
    };
    l.env.insert(FAKE.into(), "1".into());
    l.env.insert(ARGV_OUT.into(), argv.display().to_string());
    for (k, v) in env {
        l.env.insert(k.to_string(), v.clone());
    }
    l
}

fn rig(env: &[(&str, String)], data: Option<PathBuf>) -> Rig {
    let vault = tempfile::tempdir().expect("vault");
    std::fs::create_dir_all(vault.path().join("indices-kb/state")).expect("state");
    let data = data.unwrap_or_else(|| tempfile::tempdir().expect("data").keep());
    let argv = data.join("argv.jsonl");
    let app = AppHandle::start(AppConfig {
        vault: Some(vault.path().to_path_buf()),
        data_dir: Some(data.clone()),
        claude: launch(env, &argv),
        ..AppConfig::default()
    })
    .expect("start");
    let repaints = Arc::new(AtomicU64::new(0));
    let r = Arc::clone(&repaints);
    app.set_repaint(Box::new(move || {
        r.fetch_add(1, Ordering::SeqCst);
    }));
    Rig {
        app,
        _vault: vault,
        data,
        argv,
        repaints,
    }
}

fn wait_until(
    app: &AppHandle,
    limit: Duration,
    what: &str,
    f: impl Fn(&Snapshot) -> bool,
) -> Snapshot {
    let deadline = Instant::now() + limit;
    loop {
        let s = app.snapshot();
        if f(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {limit:?} waiting for {what}; errors {:?}",
            s.errors
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

const T: Duration = Duration::from_secs(30);

fn open_id(s: &Snapshot) -> String {
    s.chats.open.clone().expect("an open chat")
}

fn items(s: &Snapshot, id: &str) -> Vec<ChatItem> {
    s.chats
        .transcripts
        .get(id)
        .map(|t| t.items.clone())
        .unwrap_or_default()
}

fn busy(s: &Snapshot, id: &str) -> Option<bool> {
    s.chats.list.iter().find(|m| m.id == id).map(|m| m.busy)
}

fn new_chat(app: &AppHandle) -> String {
    app.send(Command::Chat(ChatCommand::New {
        kind: ChatKind::Ask,
    }));
    open_id(&wait_until(app, T, "a new chat", |s| {
        s.chats.open.is_some()
    }))
}

fn send_and_wait(app: &AppHandle, id: &str, text: &str) -> Snapshot {
    let users = |s: &Snapshot| {
        items(s, id)
            .iter()
            .filter(|i| matches!(i, ChatItem::User { .. }))
            .count()
    };
    let before = users(&app.snapshot());
    app.send(Command::Chat(ChatCommand::Send {
        chat_id: id.into(),
        text: text.into(),
    }));
    wait_until(app, T, "the turn", |s| {
        users(s) > before && busy(s, id) == Some(false)
    })
}

fn argv_lines(p: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).expect("argv json"))
        .collect()
}

#[test]
fn a_turn_streams_persists_and_resumes_after_a_restart() {
    let r = rig(&[], None);
    let id = new_chat(&r.app);
    let s = send_and_wait(&r.app, &id, "что было вчера?");
    let its = items(&s, &id);
    assert!(matches!(&its[0], ChatItem::User { text, .. } if text == "что было вчера?"));
    assert!(
        its.iter().any(
            |i| matches!(i, ChatItem::Assistant { text, live: false, .. } if !text.is_empty())
        ),
        "{its:?}"
    );
    assert!(
        its.iter()
            .any(|i| matches!(i, ChatItem::Tool { ok: Some(_), .. })),
        "tool rows with results"
    );
    assert!(!its
        .iter()
        .any(|i| matches!(i, ChatItem::Assistant { live: true, .. })));
    let meta = s
        .chats
        .list
        .iter()
        .find(|m| m.id == id)
        .expect("meta")
        .clone();
    let sid = session_id(SEED);
    assert_eq!(meta.session_id.as_deref(), Some(sid.as_str()));
    assert_eq!(meta.title, "что было вчера?");
    assert!(meta.cost_usd > 0.0);
    // A second turn goes to the same process (no new argv line).
    send_and_wait(&r.app, &id, "и ещё");
    assert_eq!(argv_lines(&r.argv).len(), 1);
    assert!(!argv_lines(&r.argv)[0]
        .iter()
        .any(|a| a.starts_with("--resume")));
    // The file on disk has the id; wait for the writer by reading back.
    let file = r.data.join("chats").join(format!("{id}.json"));
    let deadline = Instant::now() + T;
    while !std::fs::read_to_string(&file)
        .unwrap_or_default()
        .contains(&sid)
    {
        assert!(
            Instant::now() < deadline,
            "the session id never reached {}",
            file.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let data = r.data.clone();
    drop(r);

    // A new desk on the same data dir: the chat is listed, and its next turn
    // starts a process with --resume=<id>.
    let r = rig(&[], Some(data));
    let s = wait_until(&r.app, T, "the stored chat", |s| {
        s.chats.list.iter().any(|m| m.id == id)
    });
    assert!(
        s.chats.transcripts.is_empty(),
        "transcripts load on open, not at start"
    );
    r.app.send(Command::Chat(ChatCommand::Open {
        chat_id: id.clone(),
    }));
    let s = wait_until(&r.app, T, "the transcript", |s| !items(s, &id).is_empty());
    assert!(items(&s, &id).len() >= 4, "both turns are back");
    send_and_wait(&r.app, &id, "продолжим");
    let argv = argv_lines(&r.argv);
    assert_eq!(argv.len(), 2);
    assert!(
        argv[1].contains(&format!("--resume={sid}")),
        "{:?}",
        argv[1]
    );
}

/// TEST_PLAN C3: a resume of a conversation the CLI no longer has is retried
/// once without --resume; the owner sees the answer, not the error.
#[test]
fn a_lost_conversation_is_retried_once_without_resume() {
    let r = rig(&[], None);
    let id = new_chat(&r.app);
    send_and_wait(&r.app, &id, "первый");
    let data = r.data.clone();
    drop(r);
    let r = rig(&[("FAKE_RESUME_MISSING", "1".into())], Some(data));
    wait_until(&r.app, T, "the stored chat", |s| {
        s.chats.list.iter().any(|m| m.id == id)
    });
    r.app.send(Command::Chat(ChatCommand::Open {
        chat_id: id.clone(),
    }));
    wait_until(&r.app, T, "the transcript", |s| !items(s, &id).is_empty());
    send_and_wait(&r.app, &id, "второй");
    // The failed turn and the retry are one busy span: busy=false comes after the retry.
    let s = wait_until(&r.app, T, "the retried answer", |s| {
        items(s, &id)
            .iter()
            .filter(|i| matches!(i, ChatItem::Assistant { .. }))
            .count()
            >= 2
            && busy(s, &id) == Some(false)
    });
    let its = items(&s, &id);
    assert!(
        !its.iter().any(
            |i| matches!(i, ChatItem::Notice { key, .. } if key == driver_keys::RESUME_MISSING)
        ),
        "{its:?}"
    );
    assert_eq!(
        its.iter()
            .filter(|i| matches!(i, ChatItem::User { .. }))
            .count(),
        2,
        "the turn is not duplicated"
    );
    let argv = argv_lines(&r.argv);
    assert_eq!(argv.len(), 3, "{argv:?}");
    assert!(argv[1].iter().any(|a| a.starts_with("--resume=")));
    assert!(
        !argv[2].iter().any(|a| a.starts_with("--resume")),
        "the retry starts fresh"
    );
    let sid = session_id(SEED);
    assert_eq!(
        s.chats
            .list
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.session_id.as_deref()),
        Some(sid.as_str())
    );
}

#[test]
fn stop_mid_answer_frees_the_chat() {
    // 5000 lines at 500/s: ten seconds of answer, stopped after the first words.
    let r = rig(
        &[
            ("FAKE_CLAUDE_EVENTS", "5000".into()),
            ("FAKE_CLAUDE_RATE", "500".into()),
        ],
        None,
    );
    let id = new_chat(&r.app);
    r.app.send(Command::Chat(ChatCommand::Send {
        chat_id: id.clone(),
        text: "длинный обзор".into(),
    }));
    wait_until(&r.app, T, "the first words", |s| {
        items(s, &id)
            .iter()
            .any(|i| matches!(i, ChatItem::Assistant { live: true, .. }))
    });
    let t0 = Instant::now();
    r.app.send(Command::Chat(ChatCommand::Stop {
        chat_id: id.clone(),
    }));
    let s = wait_until(&r.app, T, "the stop", |s| busy(s, &id) == Some(false));
    assert!(
        t0.elapsed() < Duration::from_secs(5),
        "stopped in {:?}",
        t0.elapsed()
    );
    let its = items(&s, &id);
    assert_eq!(
        its.last(),
        Some(&ChatItem::Notice {
            key: keys::STOPPED.into(),
            vars: vec![]
        })
    );
    assert!(!its
        .iter()
        .any(|i| matches!(i, ChatItem::Assistant { live: true, .. })));
    assert!(!its
        .iter()
        .any(|i| matches!(i, ChatItem::Notice { key, .. } if key == driver_keys::PROCESS_DIED)));
}

#[test]
fn process_death_is_a_notice_and_frees_the_chat() {
    let r = rig(&[("FAKE_CLAUDE_DIE_AFTER", "40".into())], None);
    let id = new_chat(&r.app);
    let s = send_and_wait(&r.app, &id, "x");
    let its = items(&s, &id);
    assert!(
        its.iter()
            .any(|i| matches!(i, ChatItem::Notice { key, .. } if key == driver_keys::PROCESS_DIED)),
        "{its:?}"
    );
    assert!(!its
        .iter()
        .any(|i| matches!(i, ChatItem::Assistant { live: true, .. })));
}

#[test]
fn a_missing_binary_is_a_notice() {
    let mut r = rig(&[], None);
    drop(std::mem::replace(
        &mut r.app,
        AppHandle::start(AppConfig {
            vault: Some(r._vault.path().to_path_buf()),
            data_dir: Some(r.data.clone()),
            claude: ClaudeLaunch {
                program: Some(r.data.join("no-such-claude")),
                ..ClaudeLaunch::default()
            },
            ..AppConfig::default()
        })
        .expect("start"),
    ));
    let id = new_chat(&r.app);
    let s = send_and_wait(&r.app, &id, "x");
    assert!(
        items(&s, &id)
            .iter()
            .any(|i| matches!(i, ChatItem::Notice { key, .. } if key == keys::NO_CLAUDE)),
        "{:?}",
        items(&s, &id)
    );
}

#[test]
fn ten_thousand_events_stay_under_thirty_publishes_a_second() {
    // 10k lines at 4000/s: about 2.5 s of stream.
    let r = rig(
        &[
            ("FAKE_CLAUDE_EVENTS", "10000".into()),
            ("FAKE_CLAUDE_RATE", "4000".into()),
        ],
        None,
    );
    let id = new_chat(&r.app);
    let before = r.repaints.load(Ordering::SeqCst);
    let t0 = Instant::now();
    let s = send_and_wait(&r.app, &id, "поток");
    let secs = t0.elapsed().as_secs_f64();
    let publishes = r.repaints.load(Ordering::SeqCst) - before;
    assert!(
        secs > 2.0,
        "the stream took {secs:.2} s: the rate was not applied"
    );
    // ≤ 30/s, plus one for the send itself and one for the final frame.
    let limit = (secs * 30.0).ceil() as u64 + 2;
    assert!(
        publishes <= limit,
        "{publishes} publishes in {secs:.2} s (limit {limit})"
    );
    // Coalescing loses nothing: the answer is exactly the stream's text
    // deltas, in order, and nothing is left live.
    let spec = StreamSpec {
        events: 10_000,
        seed: SEED,
        tool_every: 4,
    };
    let want: String = turn(&spec, 0, "2026-11-26")
        .iter()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v["type"] == "stream_event" && v["event"]["delta"]["type"] == "text_delta")
        .filter_map(|v| v["event"]["delta"]["text"].as_str().map(str::to_owned))
        .collect();
    let got: String = items(&s, &id)
        .iter()
        .filter_map(|i| match i {
            ChatItem::Assistant { text, live, .. } => {
                assert!(!live);
                Some(text.as_str())
            }
            _ => None,
        })
        .collect();
    assert!(!want.is_empty());
    assert_eq!(got.len(), want.len());
    assert!(got == want, "the folded answer differs from the stream");
    eprintln!("10k events: {publishes} publishes in {secs:.2} s");
}
