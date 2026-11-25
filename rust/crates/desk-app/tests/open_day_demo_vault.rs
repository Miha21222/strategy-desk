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

use chrono::{NaiveDate, Utc};
use common::{expected_days, golden, read_json, Cmp, Report};
use desk_app::watch::WatchOptions;
use desk_app::{AppConfig, AppHandle, Command, Snapshot};
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
    let s = wait_until(&app, Duration::from_secs(10), "the index", |s| {
        s.index.is_some()
    });
    let listed: Vec<String> = s.index.as_ref().map(|i| i.days.clone()).unwrap_or_default();
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
    // Let the watcher's first pass settle (it arms at start, this is margin).
    std::thread::sleep(Duration::from_millis(300));

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
