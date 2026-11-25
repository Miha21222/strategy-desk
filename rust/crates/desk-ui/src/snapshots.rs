//! egui_kittest tests of the gauge and the screen.
//!
//! Tree tests (accessible names, «Сначала человек», glyph coverage, the
//! needle easing) run everywhere. Image snapshots render through wgpu on a
//! software adapter (lavapipe on Linux) into `crates/desk-ui/snapshots/`; they
//! are compared on Linux only, because another rasteriser (WARP on
//! windows-latest) draws anti-aliased edges a few pixels differently. Update
//! them with `UPDATE_SNAPSHOTS=1 cargo test -p desk-ui`.
//!
//! The screen tests read the demo vault (`rust/fixtures/golden/vault`) the way
//! desk-app does, synchronously, and hand the snapshot to the app.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDate;
use desk_app::{Command, ShotState, Snapshot};
use desk_core::zones::Zones;
use desk_core::{Region, WindowId};
use eframe::egui::{self, Frame, Margin, Theme, ThemePreference, Ui};
use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;

use crate::app::{demo_vault, DeskApp};
use crate::clock::{parse_now, Clock};
use crate::gauge::{Gauge, NeedleAnim, NEEDLE_TRAVEL_S};
use crate::i18n;
use crate::theme::{self, Palette};

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn setup(ctx: &egui::Context) {
    crate::fonts::install(ctx);
    theme::apply(ctx, ThemePreference::System);
}

// ---------------------------------------------------------------- the gauge

/// The gauge on a card background, like in the hero card.
fn gauge_ui(ui: &mut Ui, value: f32) {
    let p = Palette::of(ui);
    Frame::new()
        .fill(p.bg_raised)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            Gauge::new(value, Zones::default()).show(ui);
        });
}

fn gauge_harness(theme: Theme, value: f32) -> Harness<'static, f32> {
    Harness::builder()
        .with_size(egui::vec2(360.0, 290.0))
        .with_theme(theme)
        .with_context_setup(setup)
        .build_ui_state(|ui, v: &mut f32| gauge_ui(ui, *v), value)
}

#[test]
fn gauge_names_value_and_zone() {
    for (v, expect) in [
        (-80.0, "−80%, сильный шорт"),
        (0.0, "0%, нет перевеса"),
        (30.0, "+30%, лонг"),
        (100.0, "+100%, сильный лонг"),
    ] {
        let mut h = gauge_harness(Theme::Dark, v);
        h.run();
        let name = format!("{}: {expect}", i18n::t("macro.gauge"));
        h.get_by_label(&name);
    }
}

#[test]
fn gauge_follows_zone_inputs() {
    let wide = Zones::new(35.0, 70.0);
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 290.0))
        .build_ui(|ui| {
            Gauge::new(30.0, wide.clone()).show(ui);
        });
    h.run();
    h.get_by_label_contains("+30%, нет перевеса");
}

/// Target needle + the id the gauge keeps its animation under.
struct Travel {
    target: f32,
    id: Option<egui::Id>,
}

#[test]
fn needle_travels_in_point_three_seconds() {
    let step = 1.0 / 60.0;
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 290.0))
        .with_step_dt(step)
        .with_max_steps(120)
        .build_ui_state(
            |ui, s: &mut Travel| {
                // Same id the gauge derives for its default salt in this ui.
                s.id = Some(ui.make_persistent_id(egui::IdSalt::new("gauge")));
                Gauge::new(s.target, Zones::default()).show(ui);
            },
            Travel {
                target: -80.0,
                id: None,
            },
        );
    h.run();
    let shown = |h: &Harness<'_, Travel>| -> f32 {
        let id = h.state().id.unwrap_or(egui::Id::NULL);
        h.ctx
            .data(|d| d.get_temp::<NeedleAnim>(id))
            .map_or(f32::NAN, |a| a.shown)
    };
    assert_eq!(shown(&h), -80.0);

    h.state_mut().target = 100.0;
    let frames = (NEEDLE_TRAVEL_S / step).round() as usize;
    h.run_steps(frames / 2);
    let mid = shown(&h);
    assert!(mid > -80.0 && mid < 100.0, "mid-flight: {mid}");
    // Eased: past the linear midpoint (+10) at half time.
    assert!(mid > 10.0, "cubic-out leads the linear value: {mid}");
    // A little past 0.3 s the needle has arrived and egui stops repainting.
    h.run_steps(frames / 2 + 2);
    assert_eq!(shown(&h), 100.0);
    h.run();
}

#[test]
fn every_string_has_glyphs() {
    // The harness panics on a character no font can draw, so laying out the
    // whole dictionary proves Inter + egui's fallback cover our text.
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 1600.0))
        .with_context_setup(setup)
        .build_ui(|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in i18n::all() {
                    ui.label(s);
                    ui.label(egui::RichText::new(s.to_uppercase()).family(crate::fonts::bold()));
                }
                ui.label("−+%·«»—…ё Ё ✓ ✗ ⚠ × ‹ › ≈ ← →");
            });
        });
    h.run();
}

// ---------------------------------------------------------------- the screen

/// What desk-app would hold after `OpenDay(date)` and every `LoadShot`.
fn demo_snapshot(date: NaiveDate) -> Snapshot {
    let vault = demo_vault();
    let (index, marks) = desk_app::day::read_index(&vault);
    let day = desk_core::load_day(&desk_app::day::read_day(&vault, date), date);
    let shots: HashMap<String, ShotState> = day
        .shot_paths()
        .into_iter()
        .map(|p| {
            let s = match desk_app::shots::load_shot(&vault, p) {
                Ok(l) => ShotState::Ready(Arc::new(l)),
                Err(e) => ShotState::Refused(e),
            };
            (p.to_owned(), s)
        })
        .collect();
    Snapshot {
        day: Some(Arc::new(day)),
        days_index: Arc::new(marks),
        index: Arc::new(index),
        shots: Arc::new(shots),
        rev: 1,
        ..Snapshot::default()
    }
}

/// The window on the demo vault: `date` open, the clock at `now` (Kyiv).
fn screen(theme: Theme, now: &str, date: &str, win: Option<WindowId>) -> Harness<'static, DeskApp> {
    screen_sized(theme, now, date, win, 1900.0)
}

fn screen_sized(
    theme: Theme,
    now: &str,
    date: &str,
    win: Option<WindowId>,
    height: f32,
) -> Harness<'static, DeskApp> {
    let ctx = egui::Context::default();
    let clock = Clock::Fixed(parse_now(now).unwrap());
    let mut app = DeskApp::new(&ctx, clock, None, Some(demo_vault()));
    app.set_snapshot(demo_snapshot(d(date)));
    app.textures_mut().sync = true;
    let s = app.screen_mut();
    s.go(d(date));
    s.win = win;
    Harness::builder()
        .with_size(egui::vec2(1280.0, height))
        .with_theme(theme)
        .with_context_setup(setup)
        .build_ui_state(|ui, app: &mut DeskApp| app.draw(ui), app)
}

/// The «today» of the demo vault, at the hour of the reference shots.
const TODAY: &str = "2026-10-26T09:40";

fn has(h: &Harness<'_, DeskApp>, s: &str) -> bool {
    h.query_all_by_label_contains(s).next().is_some()
}

#[test]
fn ask_first_shows_only_the_question() {
    // TEST_PLAN §4 row 1 / UI_CHECKLIST §1: 26.10 EU, review there, no read.
    let mut h = screen(Theme::Light, TODAY, "2026-10-26", Some(WindowId::Eu));
    h.run();
    for s in [
        "Сначала твой взгляд",
        "Окно Европы, 10:00–12:00: куда пойдёт?",
        "вверх",
        "вниз",
        "не знаю",
        "не торгую",
        "Показать обзор без ответа",
        "после твоего взгляда",
        "Обзор готов",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    for s in [
        "Сегодня помни",
        "Что изменилось",
        "Куда смотрит рынок",
        "Что делать",
        "Почему так",
        "Не согласен?",
        "Подробнее",
        "сильный шорт",
        "−41%",
        "из 2 верно",
    ] {
        assert!(!has(&h, s), "must be hidden: {s}");
    }
    assert!(h.query_by_label("Понедельник, 26 октября").is_some());
    // Both window tiles ask first; «Сегодня» is not offered on today.
    assert_eq!(
        h.query_all_by_label_contains("сначала твой взгляд").count(),
        2
    );
    assert!(h.query_by_label("Сегодня").is_none());
}

#[test]
fn peek_opens_the_review_and_marks_the_answer() {
    // Rows 2 and 13: «Показать обзор без ответа», then an answer.
    let mut h = screen(Theme::Light, TODAY, "2026-10-26", Some(WindowId::Us));
    h.run();
    h.get_by_label("Показать обзор без ответа").click();
    h.run();
    for s in [
        "Сначала твой взгляд",
        "Окно США, 15:30–17:30: куда пойдёт?",
        "Куда смотрит рынок",
        "Что делать",
        "Почему так",
        "Можно торговать, риск 1 %",
    ] {
        assert!(has(&h, s), "missing after peek: {s}");
    }
    assert!(!has(&h, "Показать обзор без ответа"));
    // US has no drift signals: no empty «Что изменилось» frame.
    assert!(!has(&h, "Что изменилось"));
    // The evening tile stays hidden while Europe is unanswered.
    assert!(has(&h, "после твоего взгляда"));
    h.get_by_label("вниз").click();
    h.run();
    let wrote = h.state().sent.iter().any(|c| {
        matches!(
            c,
            Command::WriteHumanRead {
                region: Region::Us,
                before_scale: false,
                ..
            }
        )
    });
    assert!(
        wrote,
        "answer after a peek is written with before_scale: false"
    );
}

#[test]
fn answer_before_the_gauge_is_before_scale() {
    let mut h = screen(Theme::Light, TODAY, "2026-10-26", Some(WindowId::Eu));
    h.run();
    h.get_by_label("вниз").click();
    h.run();
    assert!(h.state().sent.iter().any(|c| matches!(
        c,
        Command::WriteHumanRead {
            region: Region::Eu,
            before_scale: true,
            ..
        }
    )));
}

#[test]
fn said_day_shows_read_memory_and_every_factor() {
    // 14.09 seen on 14.09: the owner answered both windows (row 3).
    let mut h = screen(
        Theme::Light,
        "2026-09-14T11:00",
        "2026-09-14",
        Some(WindowId::Eu),
    );
    h.run();
    for s in [
        "Твой взгляд:",
        "Шкала:",
        "Сегодня помни",
        "Куда смотрит рынок",
        "Почему так",
        "Новости и календарь",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    assert!(!has(&h, "Сначала твой взгляд"));
    // Owner rule (TZ item 6): every factor that voted is listed in «Почему так».
    let day = demo_snapshot(d("2026-09-14")).day.unwrap();
    let scale = day.eu.scale.as_deref().unwrap();
    let split = desk_core::gauge::split_factors(&scale.factors);
    for f in split.long.iter().chain(&split.short).chain(&split.silent) {
        // The tag is the config's current name, else the file's.
        let id = f.id.as_deref().unwrap();
        let name = day.factor_name(id).or(f.name.as_deref()).unwrap();
        assert!(has(&h, name), "factor hidden: {name}");
    }
    // The argument's line is the observation, not the factor's name.
    for f in split.long.iter().chain(&split.short) {
        if let Some(why) = f.why.as_deref().filter(|w| !w.is_empty()) {
            assert!(has(&h, why), "thesis missing: {why}");
        }
    }
    if !split.silent.is_empty() {
        assert!(has(&h, "молчат:"));
    }
    // Silent factors are names only in the folded line: their `why` («… ·
    // молчит», or the old «сегодня молчит: …») and no «молчит» badge.
    for f in &split.silent {
        if let Some(why) = f.why.as_deref().filter(|w| !w.is_empty()) {
            assert!(!has(&h, why), "silent why shown: {why}");
        }
    }
    assert!(h.query_by_label("молчит").is_none());
    // Owner, 25.11: «Не согласен?» is off until settings turn it on.
    const { assert!(!crate::settings::DISCUSS) };
    assert!(!has(&h, "Не согласен?"));
    // News and the calendar are on the main screen, not in «Подробнее».
    assert!(has(&h, "Подробнее: уровни, все факторы, группы, фон дня"));
    // No similar days until desk-core writes them: no empty card.
    assert!(!has(&h, "Похожие дни"));
}

#[test]
fn renamed_factor_shows_its_new_name_on_old_days() {
    // Owner, 25.11: question-like names were renamed in the vault config;
    // the day file still has the old one.
    let mut h = screen(Theme::Light, TODAY, "2026-09-14", Some(WindowId::Eu));
    {
        let app = h.state_mut();
        let mut snap = app.snapshot().clone();
        if let Some(day) = snap.day.as_mut() {
            let d = Arc::make_mut(day);
            d.names.insert(
                "night_sweep".into(),
                serde_json::Value::String("Снятие PDH / PDL в премаркете".into()),
            );
        }
        app.set_snapshot(snap);
    }
    h.run();
    assert!(has(&h, "Снятие PDH / PDL в премаркете"));
    assert!(!has(&h, "Что снял премаркет"));
}

/// Put the hand-written `similar_<D>_eu.json` into the open day.
fn with_similar(h: &mut Harness<'static, DeskApp>) {
    let app = h.state_mut();
    let mut snap = app.snapshot().clone();
    if let Some(day) = snap.day.as_mut() {
        let d = Arc::make_mut(day);
        d.eu.similar = Some(crate::screen::core_bridge::sample_similar(
            &d.date.to_string(),
        ));
    }
    app.set_snapshot(snap);
}

#[test]
fn similar_days_card_from_the_sample() {
    let mut h = screen(
        Theme::Light,
        "2026-09-14T11:00",
        "2026-09-14",
        Some(WindowId::Eu),
    );
    with_similar(&mut h);
    h.run();
    for s in [
        "Похожие дни",
        "Вторник, 8 сентября",
        "совпало: пила · ночь ≥ 0.8 ADR · стрелка −41",
        "день: вниз 0.62 ADR · от IB: вниз до IB1",
        "разбор: план дошёл до IB1",
        "от IB: вверх пробой без IB1",
        "от IB: вниз до IB2",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
}

#[test]
fn past_day_has_no_question() {
    // Row 6: 18.09 Europe seen from 26.10.
    let mut h = screen(Theme::Light, TODAY, "2026-09-18", Some(WindowId::Eu));
    h.run();
    assert!(!has(&h, "Сначала твой взгляд"));
    assert!(has(&h, "Куда смотрит рынок"));
    assert!(has(&h, "Перевеса нет · по правилам день пропускаем"));
    assert!(has(&h, "до обсуждения было"));
    assert!(
        h.query_by_label("Сегодня").is_some(),
        "another day offers «Сегодня»"
    );
}

#[test]
fn evening_asks_your_day_first() {
    let mut h = screen(
        Theme::Light,
        "2026-10-26T20:30",
        "2026-10-26",
        Some(WindowId::Review),
    );
    h.run();
    for s in [
        "Твой день",
        "Брал ли сделку?",
        "Сохранить",
        "Как прошёл день",
        "Окно пошло:",
        "Идеи на проверке",
        "Уроки дня",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    // The evening still waits for the morning reads (row 10).
    assert!(!has(&h, "Неделя в пять строк"));
}

#[test]
fn friday_evening_has_the_week() {
    let mut h = screen(Theme::Light, TODAY, "2026-09-18", Some(WindowId::Review));
    h.run();
    for s in [
        "Неделя в пять строк",
        "14.09 – 18.09",
        "О ТЕБЕ",
        "после обсуждения — пропуск",
        "Обзора не было — нечего оценивать",
    ] {
        assert!(has(&h, s), "missing: {s}");
    }
    assert!(
        !has(&h, "Твой день"),
        "a past day without answers asks nothing"
    );
}

#[test]
fn past_day_without_review_says_it_did_not_happen() {
    // UI_CHECKLIST §5 (24.09 from 26.10) with QA's Q25: no «появится в 20:00».
    let mut h = screen(Theme::Light, TODAY, "2026-09-24", Some(WindowId::Review));
    h.run();
    assert!(has(&h, "Четверг, 24 сентября"));
    assert!(has(&h, "Итога дня не было"));
    assert!(!has(&h, "появится в 20:00"));
    assert!(!has(&h, "Твой день"));
    assert!(!has(&h, "Сначала твой взгляд"));
}

#[test]
fn shots_decode_once() {
    let mut h = screen(Theme::Light, TODAY, "2026-09-14", Some(WindowId::Eu));
    h.run();
    let n = h.state_mut().textures_mut().decoded;
    assert!(n >= 1, "the M15 picture was decoded");
    h.run();
    h.run();
    assert_eq!(
        h.state_mut().textures_mut().decoded,
        n,
        "a texture is decoded once"
    );
}

// ---------------------------------------------------------------- images

#[cfg(target_os = "linux")]
fn theme_name(t: Theme) -> &'static str {
    match t {
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

#[cfg(target_os = "linux")]
fn snapshot_options() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new()
        .output_path(concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots"))
}

#[cfg(target_os = "linux")]
#[test]
fn gauge_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        for v in [-80.0, 0.0, 30.0, 100.0] {
            let mut h = gauge_harness(theme, v);
            h.run();
            let name = format!("gauge_{}_{}", theme_name(theme), v as i32);
            results.add(h.try_snapshot_options(name, &snapshot_options()));
        }
    }
}

/// One picture per screen state: a strong long day, a neutral day, an
/// evening review, and «Сначала человек» on the demo vault's «today».
#[cfg(target_os = "linux")]
#[test]
fn screen_snapshots() {
    let states = [
        ("0914_eu_similar", "2026-09-14", Some(WindowId::Eu)),
        ("0915_eu_strong_long", "2026-09-15", Some(WindowId::Eu)),
        ("0916_eu_neutral", "2026-09-16", Some(WindowId::Eu)),
        ("0914_review", "2026-09-14", Some(WindowId::Review)),
        ("1026_eu_ask", "2026-10-26", Some(WindowId::Eu)),
    ];
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        for (name, date, win) in states {
            // The similar-days state is tall: the new cards sit below the factors.
            let height = if name.ends_with("similar") {
                2900.0
            } else {
                1900.0
            };
            let mut h = screen_sized(theme, TODAY, date, win, height);
            if name.ends_with("similar") {
                with_similar(&mut h);
            }
            h.run();
            let name = format!("screen_{}_{name}", theme_name(theme));
            results.add(h.try_snapshot_options(name, &snapshot_options()));
        }
    }
}
