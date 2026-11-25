//! The demo vault through the whole app path: `AppHandle` → disk reads →
//! `desk_core::load_day` → `Snapshot`, compared with what the old app's
//! `loadDay()` gave (rust/fixtures/golden/expected/day_<D>.json), plus one
//! watcher round trip on a temp copy.
//!
//! The comparison helpers are QA's (`desk-core/tests/common`), included by
//! path so there is one definition of «equal to the golden file».

#[path = "../../desk-core/tests/common/mod.rs"]
mod common;

use std::path::Path;
use std::time::{Duration, Instant};

use std::collections::BTreeMap;

use chrono::{NaiveDate, Utc};
use common::{expected_days, golden, read_json, Cmp, Report};
use desk_app::watch::WatchOptions;
use desk_app::{AppConfig, AppHandle, Command, Snapshot};
use desk_core::vault::{
    ByRule, DayAnswerRegion, DayAnswerRegions, DayAnswers, HumanRead, TradeTaken,
};
use desk_core::{HumanSide, Region};

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("ISO date")
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
            "timed out after {limit:?} waiting for {what} (rev {})",
            s.rev
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read fixture dir").flatten() {
        let target = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), &target).expect("copy fixture");
        }
    }
}

#[test]
fn every_demo_day_through_the_app_matches_the_old_app() {
    let app = AppHandle::start(AppConfig {
        vault: Some(golden().join("vault")),
        watch: WatchOptions::default(),
    })
    .expect("start");
    let days = expected_days(&golden().join("expected"));
    let index = read_json(&golden().join("expected/index.json"));
    let s = wait_until(&app, Duration::from_secs(10), "the index", |s| s.index.ok);
    let listed: Vec<String> = s.index.days.clone();
    assert_eq!(
        serde_json::to_value(&listed).unwrap(),
        index["days"],
        "day strip = listDays()"
    );
    assert_eq!(s.days_index.len(), listed.len());

    let mut r = Report::default();
    for (d, path) in &days {
        let before = app.snapshot().rev;
        app.send(Command::OpenDay(date(d)));
        let s = wait_until(&app, Duration::from_secs(10), d, |s| {
            s.rev > before && s.day.as_ref().is_some_and(|x| x.date == date(d))
        });
        let day = s.day.as_ref().expect("day");
        let actual = serde_json::to_value(&**day).expect("Day serializes");
        r.json(
            format!("day {d} via AppHandle"),
            &read_json(path),
            &actual,
            Cmp::default(),
        );
    }
    r.finish("AppHandle over the demo vault (rust/fixtures/golden/expected/day_*.json)");
}

#[test]
fn a_state_file_written_into_the_open_day_is_on_screen_within_a_second() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir(&golden().join("vault"), tmp.path());
    let app = AppHandle::start(AppConfig {
        vault: Some(tmp.path().to_path_buf()),
        watch: WatchOptions::default(),
    })
    .expect("start");
    // «today» of the demo vault: no owner's read yet.
    let d = date("2026-10-26");
    app.send(Command::OpenDay(d));
    let s = wait_until(&app, Duration::from_secs(10), "the day", |s| {
        s.day.as_ref().is_some_and(|x| x.date == d)
    });
    assert!(
        s.day.as_ref().is_some_and(|x| x.eu.human.is_none()),
        "fixture: no read on 2026-10-26"
    );
    // No settling wait: the watcher is armed before AppHandle::start's actor
    // handles its first command, and the day above was such a command.

    let rev = app.snapshot().rev;
    let t0 = Instant::now();
    desk_app::vault_write::write_human_read(
        tmp.path(),
        d,
        Region::Eu,
        HumanSide::Short,
        true,
        Utc::now(),
    )
    .expect("write");
    let s = wait_until(&app, Duration::from_secs(1), "the new rev", |s| s.rev > rev);
    let took = t0.elapsed();
    let human = s
        .day
        .as_ref()
        .and_then(|x| x.eu.human.clone())
        .expect("the read is in the day");
    assert_eq!(human.side, HumanSide::Short);
    eprintln!("write → new snapshot: {took:?}");
}

/// Every file under `root`: vault-relative path → bytes (directories by
/// themselves too, as `<dir>/` → empty, so a new folder is a change).
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).expect("read dir").flatten() {
            let p = e.path();
            let rel = p
                .strip_prefix(root)
                .expect("under root")
                .to_string_lossy()
                .replace('\\', "/");
            if p.is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                walk(root, &p, out);
            } else {
                out.insert(rel, std::fs::read(&p).expect("read file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// Paths added, removed or changed between two trees.
fn changed(before: &BTreeMap<String, Vec<u8>>, after: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut keys: Vec<&String> = before.keys().chain(after.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .cloned()
        .collect()
}

/// TEST_PLAN «only two files»: a write through the app changes exactly the
/// owner's file it names, with desk-core's exact text, and nothing else.
#[test]
fn owner_writes_touch_exactly_their_own_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    copy_dir(&golden().join("vault"), tmp.path());
    let app = AppHandle::start(AppConfig {
        vault: Some(tmp.path().to_path_buf()),
        watch: WatchOptions::default(),
    })
    .expect("start");
    let d = date("2026-10-26");
    app.send(Command::OpenDay(d));
    wait_until(&app, Duration::from_secs(10), "the day", |s| {
        s.day.as_ref().is_some_and(|x| x.date == d)
    });

    // 1. «Твой взгляд»: a new file.
    let before = tree(tmp.path());
    let rev = app.snapshot().rev;
    app.send(Command::WriteHumanRead {
        date: d,
        region: Region::Us,
        side: HumanSide::NoTrade,
        before_scale: true,
    });
    wait_until(&app, Duration::from_secs(5), "the read on screen", |s| {
        s.rev > rev && s.day.as_ref().is_some_and(|x| x.us.human.is_some())
    });
    let after = tree(tmp.path());
    let rel = "indices-kb/state/human_read_2026-10-26_us.json";
    assert_eq!(changed(&before, &after), [rel], "only the owner's file");
    let text = String::from_utf8(after[rel].clone()).expect("utf-8");
    let rec: HumanRead = serde_json::from_str(&text).expect("a HumanRead");
    assert_eq!(
        (rec.date.as_str(), rec.region, rec.side, rec.before_scale),
        ("2026-10-26", Region::Us, HumanSide::NoTrade, true)
    );
    assert_eq!(
        text,
        desk_core::owner::owner_file_text(&rec).expect("text"),
        "desk-core's exact text"
    );

    // 2. «Твой день»: a new file, then an overwrite of it.
    let regions = DayAnswerRegions {
        eu: Some(DayAnswerRegion {
            trade: TradeTaken::Short,
            by_rule: Some(ByRule::Unsure),
        }),
        us: None,
    };
    for note in ["ждал\n вторую   манипуляцию", "второй ответ"] {
        let before = tree(tmp.path());
        let rev = app.snapshot().rev;
        app.send(Command::WriteDayAnswers {
            date: d,
            regions: regions.clone(),
            note: note.into(),
        });
        let want = desk_core::js::collapse_spaces(note);
        wait_until(&app, Duration::from_secs(5), "the answers on screen", |s| {
            s.rev > rev
                && s.day
                    .as_ref()
                    .and_then(|x| x.answers.as_ref())
                    .is_some_and(|a| a.note == want.trim())
        });
        let after = tree(tmp.path());
        let rel = "indices-kb/state/day_answers_2026-10-26.json";
        assert_eq!(changed(&before, &after), [rel], "only the owner's file");
        let text = String::from_utf8(after[rel].clone()).expect("utf-8");
        let rec: DayAnswers = serde_json::from_str(&text).expect("a DayAnswers");
        assert_eq!(rec.regions, regions);
        assert_eq!(rec.note, want.trim());
        assert_eq!(
            text,
            desk_core::owner::owner_file_text(&rec).expect("text"),
            "desk-core's exact text"
        );
    }

    // 3. A refused write changes nothing at all.
    let before = tree(tmp.path());
    let errors = app.snapshot().errors.len();
    app.send(Command::WriteHumanRead {
        date: NaiveDate::from_ymd_opt(10000, 1, 1).expect("date"),
        region: Region::Eu,
        side: HumanSide::Long,
        before_scale: true,
    });
    let s = wait_until(&app, Duration::from_secs(5), "the error", |s| {
        s.errors.len() > errors
    });
    assert_eq!(
        s.errors.last().map(|e| e.key.as_str()),
        Some("error.vault.bad_date")
    );
    assert!(changed(&before, &tree(tmp.path())).is_empty());
}
