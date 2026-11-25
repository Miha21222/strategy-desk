//! The window: the sidebar and «Обзор дня» drawn from desk-app's latest
//! `Snapshot`. The UI never waits: commands go out through `AppHandle::send`,
//! desk-app wakes the window with `request_repaint` when `rev` moves.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use desk_app::{AppHandle, Command, Snapshot};
use eframe::egui::{self, Frame, Margin, ScrollArea, ThemePreference, Ui};

use crate::cli::WindowOpts;
use crate::clock::{parse_now, Clock};
use crate::fonts;
use crate::i18n::t;
use crate::perf::Perf;
use crate::screen::kit::{self, bold, txt};
use crate::screen::shots::ShotTextures;
use crate::screen::{self, ScreenEvent, ScreenState};
use crate::theme::{self, space, text, Palette, SIDEBAR_WIDTH};

/// The demo vault of the golden fixtures, found next to the sources: `--demo`
/// is a developer's switch, the installed app has no fixtures.
pub fn demo_vault() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden/vault")
}

/// The demo vault's «today» (rust/fixtures/golden/README.md), at the hour the
/// reference screenshots were taken.
pub const DEMO_NOW: &str = "2026-10-26T09:40";

/// Content column of the main area; wider screens get side margins.
const MAIN_MAX_WIDTH: f32 = 1100.0;

/// The clock and the window state change every 30 s even when nothing else does.
const CLOCK_TICK: Duration = Duration::from_secs(30);

pub struct DeskApp {
    handle: Option<AppHandle>,
    snap: Snapshot,
    screen: ScreenState,
    tex: ShotTextures,
    clock: Clock,
    vault: Option<PathBuf>,
    perf: Option<Perf>,
    bench: Option<SwitchBench>,
    autochat: bool,
    theme: ThemePreference,
    /// Commands sent without a handle (tests read them).
    pub sent: Vec<Command>,
}

impl DeskApp {
    /// Builds the app state. Only cheap work here: it runs before the first frame.
    pub fn new(
        ctx: &egui::Context,
        clock: Clock,
        handle: Option<AppHandle>,
        vault: Option<PathBuf>,
    ) -> Self {
        fonts::install(ctx);
        theme::apply(ctx, ThemePreference::System);
        if let Some(h) = &handle {
            let ctx = ctx.clone();
            h.set_repaint(Box::new(move || ctx.request_repaint()));
        }
        Self {
            handle,
            snap: Snapshot::default(),
            screen: ScreenState::default(),
            tex: ShotTextures::default(),
            clock,
            vault,
            perf: None,
            bench: None,
            autochat: false,
            theme: ThemePreference::System,
            sent: Vec::new(),
        }
    }

    /// For tests: draw this snapshot instead of a live one.
    pub fn set_snapshot(&mut self, snap: Snapshot) {
        self.snap = snap;
    }

    pub fn screen_mut(&mut self) -> &mut ScreenState {
        &mut self.screen
    }

    pub fn textures_mut(&mut self) -> &mut ShotTextures {
        &mut self.tex
    }

    fn send(&mut self, cmds: Vec<Command>) {
        match &self.handle {
            Some(h) => cmds.into_iter().for_each(|c| h.send(c)),
            None => self.sent.extend(cmds),
        }
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
            .show(ui, |ui| {
                if let Some(pref) = sidebar(ui, &p, self.theme) {
                    self.theme = pref;
                    ui.ctx().set_theme(pref);
                }
            });
        let mut out = Vec::new();
        let mut events = Vec::new();
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg_chat).inner_margin(Margin {
                left: space::S12 as i8,
                right: space::S6 as i8,
                top: space::S6 as i8,
                bottom: 0,
            }))
            .show(ui, |ui| {
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(MAIN_MAX_WIDTH) - space::S6);
                    ui.spacing_mut().item_spacing.y = space::S6;
                    events = screen::show(
                        ui,
                        &mut self.screen,
                        &self.snap,
                        self.clock.kyiv(),
                        &mut self.tex,
                        &mut out,
                    );
                    ui.add_space(40.0);
                });
            });
        self.send(out);
        for e in events {
            match e {
                ScreenEvent::OpenNote(rel) => {
                    if let Some(v) = &self.vault {
                        open_in_system(&desk_app::day::full_path(v, &rel));
                    }
                }
            }
        }
        if self.clock == Clock::Real {
            ui.ctx().request_repaint_after(CLOCK_TICK);
        }
        if let Some(b) = &mut self.bench {
            b.after_draw(self.screen.shown_date(), &self.snap);
        }
    }
}

impl eframe::App for DeskApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(h) = &self.handle {
            self.snap = h.snapshot();
        }
        if let Some(perf) = &mut self.perf {
            perf.on_frame(ctx, frame);
        }
        if self.autochat {
            // CONTRACTS «Хуки»: one chat message after the first frame. The chat
            // driver arrives in stage 5; until then say so instead of pretending.
            self.autochat = false;
            eprintln!("strategy-desk: DESK_BENCH_AUTOCHAT=1 ignored — the chat arrives in stage 5");
        }
        if let Some(b) = &mut self.bench {
            if let Some(go) = b.before_frame(&self.snap, self.screen.shown_date()) {
                self.screen.go(go);
            }
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

/// Opens a vault note with the system's default app. Fire and forget: the
/// spawned opener returns at once, the UI does not wait for it.
fn open_in_system(path: &Path) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(path).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(path).spawn();
    if let Err(e) = r {
        eprintln!("strategy-desk: could not open {}: {e}", path.display());
    }
}

/// Where the vault comes from, in order: `--vault`, `DESK_VAULT`, `--demo`.
pub fn resolve_vault(opts: &WindowOpts) -> Option<PathBuf> {
    opts.vault
        .clone()
        .or_else(|| {
            std::env::var_os("DESK_VAULT")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .or_else(|| opts.demo.then(demo_vault))
}

/// The clock: `--now`, `DESK_NOW`, the demo's «today», else the real one.
pub fn resolve_clock(opts: &WindowOpts) -> anyhow::Result<Clock> {
    let raw = opts
        .now
        .clone()
        .or_else(|| std::env::var("DESK_NOW").ok().filter(|v| !v.is_empty()))
        .or_else(|| opts.demo.then(|| DEMO_NOW.to_owned()));
    match raw {
        None => Ok(Clock::Real),
        Some(s) => parse_now(&s)
            .map(Clock::Fixed)
            .ok_or_else(|| anyhow::anyhow!("--now: cannot read {s:?} (try 2026-10-26T09:40)")),
    }
}

/// Opens the window and blocks until it closes.
pub fn run(started: Instant, opts: WindowOpts) -> anyhow::Result<()> {
    let frame_stats = std::env::var_os("DESK_FRAME_STATS").is_some_and(|v| v == "1");
    let autochat = std::env::var_os("DESK_BENCH_AUTOCHAT").is_some_and(|v| v == "1");
    let clock = resolve_clock(&opts)?;
    let vault = resolve_vault(&opts);
    let perf = (opts.bench_first_frame || frame_stats)
        .then(|| Perf::new(started, opts.bench_first_frame, frame_stats));
    let bench = opts.bench_switch.map(SwitchBench::new);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t("app.title"))
            .with_app_id("strategy-desk")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([820.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        t("app.title"),
        options,
        Box::new(move |cc| {
            // The background thread starts here, after the window exists: no
            // disk before the first frame. Without a vault desk-app still runs
            // and reports `no-vault`.
            let handle = match &vault {
                Some(v) => AppHandle::new(v.clone()),
                None => AppHandle::start(desk_app::AppConfig::default()),
            }
            .map_err(|e| anyhow::anyhow!("background thread: {e}"))?;
            let mut app = DeskApp::new(&cc.egui_ctx, clock, Some(handle), vault);
            app.perf = perf;
            app.bench = bench;
            app.autochat = autochat;
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("could not open the window: {e}"))
}

// ---------------------------------------------------------------- sidebar

/// The four tabs of the MVP; only «Обзор дня» is here yet.
fn sidebar(ui: &mut Ui, p: &Palette, theme_now: ThemePreference) -> Option<ThemePreference> {
    ui.spacing_mut().item_spacing.y = space::S3;
    ui.add_space(space::S2);
    ui.label(bold(t("app.title"), text::HEADING, p.fg));
    ui.add_space(space::S4);
    for (key, on) in [
        ("tabs.macro", true),
        ("tabs.copilot", false),
        ("tabs.settings", false),
        ("tabs.help", false),
    ] {
        let fill = if on {
            p.accent_soft
        } else {
            egui::Color32::TRANSPARENT
        };
        let color = if on { p.accent } else { p.fg_muted };
        let r = Frame::new()
            .fill(fill)
            .corner_radius(egui::CornerRadius::same(crate::theme::radius::SM))
            .inner_margin(Margin::symmetric(space::S5 as i8, space::S3 as i8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let label = if on {
                    bold(t(key), text::BASE, color)
                } else {
                    txt(t(key), text::BASE, color)
                };
                ui.label(label);
            })
            .response;
        if !on {
            r.on_hover_text(t("desk.soon"));
        }
    }
    ui.add_space(space::S8);
    kit::sub(ui, p, t("sidebar.simple.title"));
    kit::para(ui, txt(t("desk.soon"), text::SM, p.fg_muted));
    ui.add_space(space::S8);
    kit::sub(ui, p, t("sidebar.chats"));
    kit::para(ui, txt(t("sidebar.empty"), text::SM, p.fg_muted));

    let mut picked = None;
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let prefs = [
            (ThemePreference::System, t("desk.theme.system")),
            (ThemePreference::Light, t("desk.theme.light")),
            (ThemePreference::Dark, t("desk.theme.dark")),
        ];
        let items: Vec<(&str, kit::Tone)> =
            prefs.iter().map(|(_, l)| (*l, kit::Tone::Accent)).collect();
        let sel = prefs.iter().position(|(x, _)| *x == theme_now);
        if let Some(i) = kit::seg(ui, p, &items, sel, false) {
            picked = Some(prefs[i].0);
        }
        kit::sub(ui, p, t("desk.theme.title"));
    });
    picked
}

// ---------------------------------------------------------------- --bench-switch

/// `--bench-switch N`: switch the day N times; each time is measured from
/// the command to the start of the frame after the one that drew the new
/// day (that frame has then been painted and swapped).
struct SwitchBench {
    n: u32,
    days: Vec<NaiveDate>,
    next: usize,
    pending: Option<(NaiveDate, Instant)>,
    drawn: bool,
    results: Vec<f64>,
    started: Instant,
}

const SWITCH_TIMEOUT: Duration = Duration::from_secs(10);

impl SwitchBench {
    fn new(n: u32) -> Self {
        Self {
            n,
            days: Vec::new(),
            next: 0,
            pending: None,
            drawn: false,
            results: Vec::new(),
            started: Instant::now(),
        }
    }

    fn finish(&self, timeout: bool) -> ! {
        let list: Vec<String> = self.results.iter().map(|ms| format!("{ms:.1}")).collect();
        let extra = if timeout { ", \"timeout\": true" } else { "" };
        println!("{{\"switch_ms\": [{}]{extra}}}", list.join(", "));
        // As with --bench-first-frame: the numbers are out, leave at once.
        std::process::exit(if timeout { 1 } else { 0 });
    }

    /// Called at the top of a frame; returns a day to open.
    fn before_frame(&mut self, snap: &Snapshot, shown: Option<NaiveDate>) -> Option<NaiveDate> {
        if let Some((_, t0)) = self.pending {
            if self.drawn {
                self.results.push(t0.elapsed().as_secs_f64() * 1000.0);
                self.pending = None;
                self.drawn = false;
            } else if t0.elapsed() > SWITCH_TIMEOUT {
                self.finish(true);
            } else {
                return None;
            }
        }
        if self.results.len() as u32 >= self.n {
            self.finish(false);
        }
        if self.days.is_empty() {
            // Wait for the index and the first day.
            let ready = snap.day.as_ref().is_some_and(|d| Some(d.date) == shown)
                && snap.days_index.len() >= 2;
            if !ready {
                if self.started.elapsed() > SWITCH_TIMEOUT {
                    eprintln!("strategy-desk: --bench-switch needs a vault with at least two days");
                    self.finish(true);
                }
                return None;
            }
            self.days = snap.days_index.iter().map(|m| m.date).collect();
        }
        let mut target = self.days[self.next % self.days.len()];
        if Some(target) == shown {
            self.next += 1;
            target = self.days[self.next % self.days.len()];
        }
        self.next += 1;
        self.pending = Some((target, Instant::now()));
        Some(target)
    }

    fn after_draw(&mut self, shown: Option<NaiveDate>, snap: &Snapshot) {
        if let Some((target, _)) = self.pending {
            if shown == Some(target) && snap.day.as_ref().is_some_and(|d| d.date == target) {
                self.drawn = true;
            }
        }
    }
}
