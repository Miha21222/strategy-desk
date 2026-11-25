//! The window: sidebar placeholder + the main area with the gauge (stage 1 +
//! the gauge prototype of stage 3). Draws mock data until desk-app's
//! `Snapshot` is wired in.

use std::time::Instant;

use eframe::egui::{
    self, Color32, CornerRadius, Frame, Margin, RichText, ScrollArea, Shadow, Stroke,
    ThemePreference, Ui,
};

use crate::cli::WindowOpts;
use crate::gauge::{group_bar, split_bar, Gauge};
use crate::i18n::{t, tf};
use crate::mock::{self, LightColor, ScaleState};
use crate::perf::Perf;
use crate::theme::{self, radius, space, text, Palette, SIDEBAR_WIDTH};
use desk_core::zones::zone_of;

/// Content column of the main area; wider screens get side margins.
const MAIN_MAX_WIDTH: f32 = 720.0;

/// The `--demo` controls.
#[derive(Clone, Debug, PartialEq)]
pub struct Demo {
    pub needle: f32,
    pub ghost: bool,
    pub theme: ThemePreference,
}

pub struct DeskApp {
    perf: Option<Perf>,
    scale: ScaleState,
    demo: Option<Demo>,
}

impl DeskApp {
    /// Builds the app state. Only cheap work here: it runs before the first frame.
    pub fn new(ctx: &egui::Context, opts: WindowOpts, perf: Option<Perf>) -> Self {
        theme::apply(ctx, ThemePreference::System);
        let scale = mock::demo_scale();
        let demo = opts.demo.then_some(Demo {
            needle: scale.result.needle,
            ghost: false,
            theme: ThemePreference::System,
        });
        Self { perf, scale, demo }
    }

    pub fn demo_mut(&mut self) -> Option<&mut Demo> {
        self.demo.as_mut()
    }

    /// The whole window; public so the kittest harness can drive it.
    pub fn draw(&mut self, ui: &mut Ui) {
        let p = Palette::of(ui);
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(SIDEBAR_WIDTH)
            .frame(
                Frame::new()
                    .fill(p.bg_sidebar)
                    .inner_margin(Margin::same(space::S4 as i8)),
            )
            .show(ui, |ui| sidebar(ui, &p));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg_chat).inner_margin(Margin {
                left: space::S12 as i8,
                right: space::S12 as i8,
                top: space::S6 as i8,
                bottom: 0,
            }))
            .show(ui, |ui| self.main_area(ui, &p));
    }

    fn main_area(&mut self, ui: &mut Ui, p: &Palette) {
        ui.label(
            RichText::new(t("macro.title"))
                .size(text::TITLE)
                .color(p.fg)
                .strong(),
        );
        ui.add_space(space::S4);
        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            let w = ui.available_width().min(MAIN_MAX_WIDTH);
            ui.set_max_width(w);
            ui.spacing_mut().item_spacing.y = space::S6;
            let (needle, before) = match &self.demo {
                Some(d) => (d.needle, d.ghost.then_some(self.scale.result.needle)),
                None => (
                    self.scale
                        .after
                        .as_ref()
                        .map_or(self.scale.result.needle, |a| a.needle),
                    self.scale.after.as_ref().map(|_| self.scale.result.needle),
                ),
            };
            hero_card(ui, p, &self.scale, needle, before);
            groups_card(ui, p, &self.scale);
            ui.label(
                RichText::new(t("desk.mockNote"))
                    .size(text::SM)
                    .color(p.fg_muted),
            );
            if let Some(demo) = &mut self.demo {
                demo_card(ui, p, demo);
            }
            ui.add_space(40.0);
        });
    }
}

impl eframe::App for DeskApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(perf) = &mut self.perf {
            perf.on_frame(ctx, frame);
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

/// Opens the window and blocks until it closes.
pub fn run(started: Instant, opts: WindowOpts) -> anyhow::Result<()> {
    let frame_stats = std::env::var_os("DESK_FRAME_STATS").is_some_and(|v| v == "1");
    let perf = (opts.bench_first_frame || frame_stats)
        .then(|| Perf::new(started, opts.bench_first_frame, frame_stats));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t("app.title"))
            .with_app_id("strategy-desk")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        t("app.title"),
        options,
        Box::new(move |cc| Ok(Box::new(DeskApp::new(&cc.egui_ctx, opts, perf)))),
    )
    .map_err(|e| anyhow::anyhow!("could not open the window: {e}"))
}

// ---------------------------------------------------------------- pieces

fn sidebar(ui: &mut Ui, p: &Palette) {
    ui.spacing_mut().item_spacing.y = space::S3;
    ui.add_space(space::S2);
    ui.label(
        RichText::new(t("app.title"))
            .size(text::HEADING)
            .color(p.fg)
            .strong(),
    );
    ui.add_space(space::S4);
    // The one tab of the MVP, drawn as the selected nav item.
    Frame::new()
        .fill(p.accent_soft)
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S5 as i8, space::S3 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(t("tabs.macro")).color(p.accent).strong());
        });
    ui.add_space(space::S8);
    section_title(ui, p, t("sidebar.simple.title"));
    ui.label(
        RichText::new(t("desk.soon"))
            .size(text::SM)
            .color(p.fg_muted),
    );
    ui.add_space(space::S8);
    section_title(ui, p, t("sidebar.chats"));
    ui.label(
        RichText::new(t("sidebar.empty"))
            .size(text::SM)
            .color(p.fg_muted),
    );
}

/// `.macro .sub`: small caps-like muted heading.
fn section_title(ui: &mut Ui, p: &Palette, s: &str) {
    ui.label(
        RichText::new(s.to_uppercase())
            .size(text::SM)
            .color(p.fg_muted)
            .strong(),
    );
}

/// `.card` / `.card-head` / `.card-body` of cards.css.
pub(crate) fn card<R>(ui: &mut Ui, p: &Palette, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let r = radius::MD;
    Frame::new()
        .fill(p.bg_raised)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(r))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(if p.dark { 60 } else { 18 }),
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let head = Frame::new()
                .fill(p.bg_elev)
                .corner_radius(CornerRadius {
                    nw: r - 1,
                    ne: r - 1,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(Margin::symmetric(space::S6 as i8, space::S3 as i8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(title).size(text::BASE).color(p.fg).strong());
                })
                .response;
            ui.painter().hline(
                head.rect.x_range(),
                head.rect.bottom(),
                Stroke::new(1.0, p.border),
            );
            Frame::new()
                .inner_margin(Margin::symmetric(space::S6 as i8, space::S5 as i8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = space::S5;
                    body(ui)
                })
                .inner
        })
        .inner
}

fn hero_card(ui: &mut Ui, p: &Palette, s: &ScaleState, needle: f32, before: Option<f32>) {
    card(ui, p, t("macro.whereTitle"), |ui| {
        Gauge::new(needle, s.zones.clone()).before(before).show(ui);
        let zone = zone_of(f64::from(needle), &s.zones);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new(t(&format!("macro.head.{}", zone.as_str())))
                    .size(text::LG)
                    .color(p.fg)
                    .strong(),
            );
        });
        let r = &s.result;
        split_bar(ui, r.long, r.short, r.undecided);
        light_box(ui, p, s);
    });
}

/// `.mlight`: the trade light under the split bar.
fn light_box(ui: &mut Ui, p: &Palette, s: &ScaleState) {
    let (fill, line) = match s.light.color {
        LightColor::Green => (
            p.ok_soft,
            tf("macro.lightGreen", &[("r", &s.light.risk_pct)]),
        ),
        LightColor::Yellow => (
            p.warn_soft,
            tf("macro.lightYellow", &[("r", &s.light.risk_pct)]),
        ),
        LightColor::Red => (p.err_soft, t("macro.lightRed").to_owned()),
    };
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius::SM))
        .inner_margin(Margin::symmetric(space::S5 as i8, space::S4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(line).color(p.fg).strong());
        });
}

fn groups_card(ui: &mut Ui, p: &Palette, s: &ScaleState) {
    let groups = &s.result.groups;
    if groups.is_empty() {
        return; // a card with nothing to show is not rendered at all
    }
    let scale = groups.iter().map(|g| g.share).fold(1.0, f32::max);
    card(ui, p, t("macro.groups"), |ui| {
        for g in groups {
            group_bar(ui, &g.name, g.share, g.long, g.short, scale);
        }
        ui.label(
            RichText::new(t("macro.groupsNote"))
                .size(text::SM)
                .color(p.fg_muted),
        );
    });
}

fn demo_card(ui: &mut Ui, p: &Palette, demo: &mut Demo) {
    card(ui, p, t("desk.demo.title"), |ui| {
        ui.horizontal(|ui| {
            let label = ui.label(t("desk.demo.needle"));
            ui.add(egui::Slider::new(&mut demo.needle, -100.0..=100.0).step_by(1.0))
                .labelled_by(label.id);
        });
        ui.checkbox(&mut demo.ghost, t("desk.demo.ghost"));
        ui.horizontal(|ui| {
            ui.label(t("desk.demo.theme"));
            let before = demo.theme;
            ui.selectable_value(
                &mut demo.theme,
                ThemePreference::System,
                t("desk.theme.system"),
            );
            ui.selectable_value(
                &mut demo.theme,
                ThemePreference::Light,
                t("desk.theme.light"),
            );
            ui.selectable_value(&mut demo.theme, ThemePreference::Dark, t("desk.theme.dark"));
            if demo.theme != before {
                ui.ctx().set_theme(demo.theme);
            }
        });
    });
}
