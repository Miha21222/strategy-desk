//! The window: the sidebar and «Обзор дня» drawn from desk-app's latest
//! `Snapshot`. The UI never waits: commands go out through `AppHandle::send`,
//! desk-app wakes the window with `request_repaint` when `rev` moves.

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use desk_app::{AppHandle, Command, Snapshot};
use eframe::egui::{self, Frame, Margin, ScrollArea, ThemePreference, Ui};

use crate::chat::{self, view as chat_view, AutoChat, ChatAction, ChatUi, ChatsView, Page};
use crate::cli::WindowOpts;
use crate::clock::{parse_now, Clock};
use crate::connect::{
    self, lights::LightsEvent, settings::SettingsEvent, wizard::WizardEvent, ConnUi, ConnView,
    SettingsTab,
};
use crate::fonts;
use crate::i18n::t;
use crate::opts::Opts;
use crate::perf::Perf;
use crate::screen::kit::{bold, txt};
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

/// The largest texture side the desk asks for (`DESK_OPT=atlas_full` lifts it).
pub const MAX_TEXTURE_SIDE: usize = 4096;

/// Reading column of «Настройки» and the wizard.
const SETTINGS_MAX_WIDTH: f32 = 860.0;

/// Content column of the main area; wider screens get side margins.
const MAIN_MAX_WIDTH: f32 = 1100.0;

/// Time to the next wall-clock minute (+ a margin so the frame lands after it).
pub fn until_next_minute(now: chrono::DateTime<chrono::Utc>) -> Duration {
    use chrono::Timelike;
    let into = u64::from(now.second()) * 1000 + u64::from(now.timestamp_subsec_millis().min(999));
    Duration::from_millis(60_000 - into + 50)
}

pub struct DeskApp {
    handle: Option<AppHandle>,
    snap: Snapshot,
    screen: ScreenState,
    tex: ShotTextures,
    clock: Clock,
    vault: Option<PathBuf>,
    perf: Option<Perf>,
    bench: Option<SwitchBench>,
    chat: ChatUi,
    /// The chats on screen: desk-app's, or a test's ([`DeskApp::set_chats`]).
    chats: Arc<ChatsView>,
    chats_override: Option<Arc<ChatsView>>,
    theme: ThemePreference,
    /// UI zoom (1.0 = 100 %).
    scale: f32,
    /// Theme and zoom last handed to egui (only a change is applied, so a
    /// test harness keeps the theme it was built with).
    applied_look: (ThemePreference, f32),
    /// The connection screens: desk-app's view (or a test's) and the UI's memory.
    conn: ConnView,
    conn_override: Option<ConnView>,
    /// Theme and scale as the settings view last said them (a change there
    /// is applied; a local pick wins until the next change).
    seen_look: Option<(connect::Theme, f32)>,
    cui: ConnUi,
    /// Last «window focused» probe (s of egui time), to debounce.
    focus_probe: Option<f64>,
    /// See `raw_input_hook`; `None` = the driver's limit.
    max_texture_side: Option<usize>,
    /// desk-app starting on a helper thread (`DESK_OPT` default): the
    /// handle arrives here; commands wait in `sent` until it does.
    pending: Option<mpsc::Receiver<std::io::Result<AppHandle>>>,
    /// `DESK_OPT=app_late`: start desk-app once the first frame is out.
    late_start: Option<Option<PathBuf>>,
    frames: u64,
    day_seen: bool,
    /// Commands sent without a handle (tests read them; a starting app
    /// gets them when its handle arrives).
    pub sent: Vec<Command>,
}

/// Start desk-app on its own thread so the window's creator returns at once.
fn start_app_async(vault: Option<PathBuf>) -> mpsc::Receiver<std::io::Result<AppHandle>> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("desk-app-start".into())
        .spawn(move || {
            let _ = tx.send(start_app(vault));
        });
    if let Err(e) = spawned {
        eprintln!("strategy-desk: could not start desk-app: {e}");
    }
    rx
}

fn start_app(vault: Option<PathBuf>) -> std::io::Result<AppHandle> {
    let h = match vault {
        Some(v) => AppHandle::new(v),
        None => AppHandle::start(desk_app::AppConfig::default()),
    };
    crate::perf::trace::mark("app_handle_started");
    h
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
        Self::with_parts(clock, handle, vault)
    }

    fn with_parts(clock: Clock, handle: Option<AppHandle>, vault: Option<PathBuf>) -> Self {
        Self {
            handle,
            snap: Snapshot::default(),
            screen: ScreenState::default(),
            tex: ShotTextures::default(),
            clock,
            vault,
            perf: None,
            bench: None,
            chat: ChatUi::default(),
            chats: Arc::default(),
            chats_override: None,
            theme: ThemePreference::System,
            scale: 1.0,
            applied_look: (ThemePreference::System, 1.0),
            conn: ConnView::default(),
            conn_override: None,
            seen_look: None,
            cui: ConnUi::default(),
            focus_probe: None,
            max_texture_side: Some(MAX_TEXTURE_SIDE),
            pending: None,
            late_start: None,
            frames: 0,
            day_seen: false,
            sent: Vec::new(),
        }
    }

    /// The handle arrived from the helper thread: wire the repaint and hand
    /// over the commands the first frames queued.
    fn adopt(&mut self, ctx: &egui::Context, h: AppHandle) {
        let c = ctx.clone();
        h.set_repaint(Box::new(move || c.request_repaint()));
        for cmd in self.sent.drain(..) {
            h.send(cmd);
        }
        self.handle = Some(h);
        ctx.request_repaint();
    }

    /// The snapshot on screen.
    pub fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    /// For tests: draw this snapshot instead of a live one.
    pub fn set_snapshot(&mut self, snap: Snapshot) {
        self.snap = snap;
    }

    /// For tests: draw these chats instead of the snapshot's.
    pub fn set_chats(&mut self, chats: ChatsView) {
        let chats = Arc::new(chats);
        self.chats = chats.clone();
        self.chats_override = Some(chats);
    }

    /// For tests: draw this connection state instead of the snapshot's.
    pub fn set_conn(&mut self, conn: ConnView) {
        self.conn_override = Some(conn);
        self.refresh_conn();
    }

    pub fn conn_ui_mut(&mut self) -> &mut ConnUi {
        &mut self.cui
    }

    pub fn chat_mut(&mut self) -> &mut ChatUi {
        &mut self.chat
    }

    pub fn screen_mut(&mut self) -> &mut ScreenState {
        &mut self.screen
    }

    pub fn textures_mut(&mut self) -> &mut ShotTextures {
        &mut self.tex
    }

    fn send_chat(&mut self, actions: Vec<ChatAction>) {
        let cmds = actions.into_iter().map(chat::to_command).collect();
        self.send(cmds);
    }

    fn refresh_conn(&mut self) {
        let today = desk_core::windows::kyiv_now(self.clock.now()).date;
        let mut c = match &self.conn_override {
            Some(c) => c.clone(),
            None => connect::conn_of(&self.snap, today),
        };
        let look = (c.settings.theme, c.settings.scale);
        if self.seen_look != Some(look) {
            self.seen_look = Some(look);
            self.theme = theme_pref(look.0);
            self.scale = if look.1 > 0.0 { look.1 } else { 1.0 };
        }
        // What is applied is what the page shows (a pick shows before the
        // snapshot brings it back).
        let theme = match self.theme {
            ThemePreference::Light => connect::Theme::Light,
            ThemePreference::Dark => connect::Theme::Dark,
            ThemePreference::System => connect::Theme::System,
        };
        if c.settings.theme != theme || c.settings.scale != self.scale {
            let s = std::sync::Arc::make_mut(&mut c.settings);
            s.theme = theme;
            s.scale = self.scale;
        }
        self.conn = c;
    }

    /// Apply what is the UI's own (theme, scale), then hand the rest to desk-app.
    fn send_conn(&mut self, cmds: Vec<Command>) {
        // Theme and scale take effect here and now; desk-app stores them.
        for c in &cmds {
            if let Command::SaveSettings(next) = c {
                self.theme = theme_pref(next.theme);
                self.scale = next.scale;
            }
        }
        self.send(cmds);
        self.refresh_conn();
    }

    fn refresh_chats(&mut self) {
        self.chats = match &self.chats_override {
            Some(c) => c.clone(),
            None => chat::chats_of(&self.snap),
        };
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
        let mut actions = Vec::new();
        let mut conn_actions = Vec::new();
        self.chat.sync(&self.chats, &mut actions);
        let today = desk_core::windows::kyiv_now(self.clock.now()).date;
        if self.applied_look != (self.theme, self.scale) {
            self.applied_look = (self.theme, self.scale);
            ui.ctx().set_theme(self.theme);
            ui.ctx().set_zoom_factor(self.scale);
        }
        // The first run opens the wizard, once.
        if self.conn.first_run && !self.cui.wizard_shown {
            self.cui.wizard_shown = true;
            self.chat.page = Page::Wizard;
        }
        // A window brought to the front asks for fresh lights (at most every 30 s).
        let focused = ui.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::WindowFocused(true)))
        });
        let now_s = ui.input(|i| i.time);
        if focused && self.focus_probe.is_none_or(|t0| now_s - t0 > 30.0) {
            self.focus_probe = Some(now_s);
            conn_actions.push(Command::CheckStatus);
        }
        egui::Panel::left("sidebar")
            .resizable(false)
            .exact_size(SIDEBAR_WIDTH)
            .frame(
                Frame::new()
                    .fill(p.bg_sidebar)
                    .inner_margin(Margin::same(space::S4 as i8)),
            )
            .show(ui, |ui| {
                let ev = sidebar(
                    ui,
                    &p,
                    &mut self.chat,
                    &self.chats,
                    &self.conn,
                    today,
                    &mut actions,
                    &mut conn_actions,
                );
                if ev == Some(LightsEvent::OpenDiagnostics) {
                    self.chat.page = Page::Settings;
                    self.cui.tab = SettingsTab::Diagnostics;
                    if self.conn.checks_at == 0 {
                        conn_actions.push(Command::RunPreflight);
                    }
                }
            });
        let mut out = Vec::new();
        let mut events = Vec::new();
        let page = self.chat.page.clone();
        egui::CentralPanel::default()
            .frame(Frame::new().fill(p.bg_chat).inner_margin(Margin {
                left: space::S12 as i8,
                right: space::S6 as i8,
                top: space::S6 as i8,
                bottom: 0,
            }))
            .show(ui, |ui| {
                match page {
                    Page::Day => {}
                    Page::Chat(_) => {
                        actions.extend(chat_view::page(ui, &p, &mut self.chat, &self.chats));
                        return;
                    }
                    Page::Settings | Page::Wizard => {
                        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(SETTINGS_MAX_WIDTH));
                            ui.spacing_mut().item_spacing.y = space::S6;
                            if page == Page::Settings {
                                let ev = connect::settings::page(
                                    ui,
                                    &p,
                                    &mut self.cui,
                                    &self.conn,
                                    &mut conn_actions,
                                );
                                if ev == Some(SettingsEvent::OpenWizard) {
                                    self.cui.wizard.step = 0;
                                    self.chat.page = Page::Wizard;
                                }
                            } else {
                                let ev = connect::wizard::page(
                                    ui,
                                    &p,
                                    &mut self.cui,
                                    &self.conn,
                                    &mut conn_actions,
                                );
                                if ev == Some(WizardEvent::Close) {
                                    self.chat.page = Page::Day;
                                }
                            }
                            ui.add_space(40.0);
                        });
                        return;
                    }
                }
                ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(MAIN_MAX_WIDTH) - space::S6);
                    ui.spacing_mut().item_spacing.y = space::S6;
                    events = screen::show(
                        ui,
                        &mut self.screen,
                        &self.snap,
                        self.clock.now(),
                        &mut self.tex,
                        screen::ChatCtx {
                            view: &self.chats,
                            md: &mut self.chat.md,
                        },
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
                ScreenEvent::Chat(a) => actions.push(a),
                ScreenEvent::OpenChat(id) => self.chat.open(&id, &mut actions),
            }
        }
        self.send_chat(actions);
        self.send_conn(conn_actions);
        if self.applied_look != (self.theme, self.scale) {
            ui.ctx().request_repaint();
        }
        if self.clock == Clock::Real {
            // Window states and «сегодня» move with the minute; nothing else
            // on screen depends on the clock.
            ui.ctx()
                .request_repaint_after(until_next_minute(self.clock.now()));
        }
        if self.screen.drew_day && !self.day_seen {
            self.day_seen = true;
            crate::perf::trace::mark("day_drawn");
            if let Some(perf) = &mut self.perf {
                perf.day_drawn(ui.ctx());
            }
        }
        if let Some(b) = &mut self.bench {
            b.after_draw(self.screen.shown_date(), &self.snap);
        }
    }
}

impl eframe::App for DeskApp {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 {
            if let Some(v) = self.late_start.take() {
                self.pending = Some(start_app_async(v));
            }
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(Ok(h)) => {
                    self.pending = None;
                    self.adopt(ctx, h);
                }
                Ok(Err(e)) => {
                    self.pending = None;
                    eprintln!("strategy-desk: background thread: {e}");
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Not there yet: look again soon (the helper thread
                    // cannot wake us before the handle exists).
                    ctx.request_repaint_after(Duration::from_millis(5));
                }
                Err(mpsc::TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if let Some(h) = &self.handle {
            self.snap = h.snapshot();
        }
        self.refresh_chats();
        self.refresh_conn();
        if let Some(perf) = &mut self.perf {
            perf.on_frame(ctx, frame);
        }
        if self.frames >= 2 && self.chat.autochat.is_some() {
            // CONTRACTS «Хуки»: one chat message once the first frame is out.
            let mut actions = Vec::new();
            let waiting = self.chat.autochat_step(&mut actions);
            self.send_chat(actions);
            if waiting {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        if let Some(b) = &mut self.bench {
            if let Some(go) = b.before_frame(&self.snap, self.screen.shown_date()) {
                self.screen.go(go);
            }
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let first = self.frames == 1;
        if first {
            crate::perf::trace::mark("first_update_begin");
        }
        self.draw(ui);
        if first {
            crate::perf::trace::mark("first_update_end");
        }
    }

    /// Textures, and with them egui's font atlas (as wide as the largest
    /// texture allowed), are kept to [`MAX_TEXTURE_SIDE`]: the driver allows
    /// 16384, i.e. a 16384-px-wide atlas that grows 64 KB a row on the CPU
    /// and again in the driver.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(cap) = self.max_texture_side {
            raw.max_texture_side = Some(raw.max_texture_side.map_or(cap, |m| m.min(cap)));
        }
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
    let o = Opts::from_env();
    let clock = resolve_clock(&opts)?;
    let vault = resolve_vault(&opts);
    let benching = opts.bench_first_frame || opts.bench_data_ready;
    let perf = (benching || frame_stats || crate::perf::trace::enabled()).then(|| {
        Perf::new(
            started,
            opts.bench_first_frame,
            opts.bench_data_ready,
            frame_stats,
        )
    });
    let bench = opts.bench_switch.map(SwitchBench::new);
    #[allow(unused_mut)] // only the glow build has a vsync switch here
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(t("app.title"))
            .with_app_id("strategy-desk")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([820.0, 560.0]),
        // Defaults kept on purpose (checked 26.11): no MSAA, no depth or
        // stencil buffer, the window created hidden and shown after the
        // first painted frame (eframe/glow), no persistence feature.
        ..Default::default()
    };
    #[cfg(feature = "glow")]
    {
        options.glow_options.vsync = o.vsync;
    }
    crate::perf::trace::mark_with(
        "before_run_native",
        &format!(", \"opts\": \"{}\"", o.label()),
    );
    eframe::run_native(
        t("app.title"),
        options,
        Box::new(move |cc| {
            crate::perf::trace::mark("creator_entered");
            if o.inter {
                fonts::install(&cc.egui_ctx);
            }
            theme::apply(&cc.egui_ctx, ThemePreference::System);
            crate::perf::trace::mark("fonts_installed");
            let mut app = DeskApp::with_parts(clock, None, vault.clone());
            // desk-app (tokio runtime, watcher, the first index read) never
            // starts on this thread by default: the creator returns at once.
            if o.app_late {
                app.late_start = Some(vault);
            } else if o.app_async {
                app.pending = Some(start_app_async(vault));
            } else {
                let h = start_app(vault).map_err(|e| anyhow::anyhow!("background thread: {e}"))?;
                app.adopt(&cc.egui_ctx, h);
            }
            if o.atlas_full {
                app.max_texture_side = None;
            }
            app.tex.full_everywhere = o.shots_full;
            app.tex.preview_mips = o.shot_mips;
            app.tex.disabled = !o.shots;
            if o.shot_small {
                app.tex.preview_max_w = 640;
            }
            app.perf = perf;
            app.bench = bench;
            if autochat {
                let text = std::env::var("DESK_BENCH_AUTOCHAT_TEXT")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| t("home.askToday").to_owned());
                app.chat.autochat = Some(AutoChat::new(text));
            }
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("could not open the window: {e}"))
}

// ---------------------------------------------------------------- sidebar

/// The tabs, the lights, the chats.
#[allow(clippy::too_many_arguments)]
fn sidebar(
    ui: &mut Ui,
    p: &Palette,
    chat: &mut ChatUi,
    chats: &ChatsView,
    conn: &ConnView,
    today: NaiveDate,
    actions: &mut Vec<ChatAction>,
    conn_actions: &mut Vec<Command>,
) -> Option<LightsEvent> {
    ui.spacing_mut().item_spacing.y = space::S3;
    ui.add_space(space::S2);
    ui.label(bold(t("app.title"), text::HEADING, p.fg));
    ui.add_space(space::S4);
    for (key, target) in [
        ("tabs.macro", Some(Page::Day)),
        ("tabs.settings", Some(Page::Settings)),
        ("tabs.help", None),
    ] {
        let on = target.as_ref() == Some(&chat.page);
        let fill = if on {
            p.accent_soft
        } else {
            egui::Color32::TRANSPARENT
        };
        let color = if on {
            p.accent
        } else if target.is_some() {
            p.fg
        } else {
            p.fg_muted
        };
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
        match target {
            Some(page) => {
                let r = ui
                    .interact(r.rect, ui.scope_id().with(key), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let label = t(key);
                r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, label));
                if r.clicked() {
                    chat.page = page;
                }
            }
            None => {
                r.on_hover_text(t("desk.soon"));
            }
        }
    }
    ui.add_space(space::S6);
    let ev = connect::lights::show(ui, p, conn, conn_actions);
    ui.add_space(space::S6);
    let list_h = (ui.available_height() - space::S4).max(80.0);
    actions.extend(chat_view::sidebar_list(ui, p, chat, chats, today, list_h));
    ev
}

fn theme_pref(th: connect::Theme) -> ThemePreference {
    match th {
        connect::Theme::System => ThemePreference::System,
        connect::Theme::Light => ThemePreference::Light,
        connect::Theme::Dark => ThemePreference::Dark,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn repaint_lands_just_after_the_minute() {
        let t = chrono::Utc
            .with_ymd_and_hms(2026, 11, 26, 9, 15, 42)
            .unwrap();
        assert_eq!(until_next_minute(t), Duration::from_millis(18_050));
        let t = chrono::Utc
            .with_ymd_and_hms(2026, 11, 26, 9, 15, 0)
            .unwrap();
        assert_eq!(until_next_minute(t), Duration::from_millis(60_050));
    }
}
