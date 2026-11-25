//! egui_kittest tests of the gauge and the window.
//!
//! Tree tests (accessible names, Cyrillic glyph coverage, the needle easing)
//! run everywhere. Image snapshots render through wgpu on a software adapter
//! (lavapipe on Linux) into `crates/desk-ui/snapshots/`; they are compared on
//! Linux only, because another rasteriser (WARP on windows-latest) draws
//! anti-aliased edges a few pixels differently. Update them with
//! `UPDATE_SNAPSHOTS=1 cargo test -p desk-ui`.

use eframe::egui::{self, Frame, Margin, Theme, ThemePreference, Ui};
use egui_kittest::kittest::Queryable as _;
use egui_kittest::Harness;

use crate::app::DeskApp;
use crate::cli::WindowOpts;
use crate::gauge::{Gauge, NeedleAnim, NEEDLE_TRAVEL_S};
use crate::geom::Zones;
use crate::i18n;
use crate::theme::{self, Palette};

#[cfg(target_os = "linux")] // only the image snapshots use it
const VALUES: [f32; 4] = [-80.0, 0.0, 30.0, 100.0];

#[cfg(target_os = "linux")] // only the image snapshots use it
fn theme_name(t: Theme) -> &'static str {
    match t {
        Theme::Dark => "dark",
        Theme::Light => "light",
    }
}

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
        .with_context_setup(|ctx| theme::apply(ctx, ThemePreference::System))
        .build_ui_state(|ui, v: &mut f32| gauge_ui(ui, *v), value)
}

fn window_harness(theme: Theme) -> Harness<'static, DeskApp> {
    let ctx = egui::Context::default();
    let app = DeskApp::new(&ctx, WindowOpts::default(), None);
    Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_theme(theme)
        .with_context_setup(|ctx| theme::apply(ctx, ThemePreference::System))
        .build_ui_state(|ui, app: &mut DeskApp| app.draw(ui), app)
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
    let wide = Zones {
        neutral: 35.0,
        strong: 70.0,
    };
    let mut h = Harness::builder()
        .with_size(egui::vec2(360.0, 290.0))
        .build_ui(|ui| {
            Gauge::new(30.0, wide).show(ui);
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
    // whole dictionary proves the default fonts cover our Cyrillic.
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 1600.0))
        .build_ui(|ui| {
            for (_, s) in i18n::RU {
                ui.label(*s);
                ui.label(egui::RichText::new(s.to_uppercase()).strong());
            }
            ui.label("−+%·«»—…ё Ё");
        });
    h.run();
}

#[test]
fn window_shows_sidebar_and_gauge() {
    let mut h = window_harness(Theme::Dark);
    h.run();
    h.get_by_label(i18n::t("app.title"));
    h.get_by_label(i18n::t("macro.whereTitle"));
    h.get_by_label(i18n::t("macro.groups"));
    h.get_by_label_contains("+30%, лонг");
}

// ---------------------------------------------------------------- images

#[cfg(target_os = "linux")] // only the image snapshots use it
fn snapshot_options() -> egui_kittest::SnapshotOptions {
    egui_kittest::SnapshotOptions::new()
        .output_path(concat!(env!("CARGO_MANIFEST_DIR"), "/snapshots"))
}

#[cfg(target_os = "linux")]
#[test]
fn gauge_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        for v in VALUES {
            let mut h = gauge_harness(theme, v);
            h.run();
            let name = format!("gauge_{}_{}", theme_name(theme), v as i32);
            results.add(h.try_snapshot_options(name, &snapshot_options()));
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn window_snapshots() {
    let mut results = egui_kittest::SnapshotResults::new();
    for theme in [Theme::Dark, Theme::Light] {
        let mut h = window_harness(theme);
        h.run();
        let name = format!("window_{}", theme_name(theme));
        results.add(h.try_snapshot_options(name, &snapshot_options()));
    }
}
