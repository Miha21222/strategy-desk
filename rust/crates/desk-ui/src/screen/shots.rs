//! Chart screenshots (`Shots.tsx`, TZ item 5): the main picture under the
//! plan, all of them under «Подробнее», and the magnifier.
//!
//! Bytes come from desk-app (`Command::LoadShot` → `Snapshot::shots`); the PNG
//! is decoded on a worker thread, once, and kept as a texture with mipmaps,
//! so a picture shown again (another window, back to the day) costs nothing.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc;
use std::sync::Arc;

use desk_app::{Command, ShotState, Snapshot};
use desk_core::shots::{
    legend_color, main_shot, shot_tab_label, shots_error_text, ShotsErrorKind, SHOTS_ERROR_MAX,
};
use desk_core::vault::{MacroShot, ShotLegend, Shots};
use eframe::egui::{
    self, load::SizedTexture, pos2, vec2, Align2, Color32, ColorImage, CornerRadius, FontId, Id,
    Key, Order, Rect, Sense, Stroke, TextureHandle, TextureOptions, Ui, Vec2,
};

use super::kit::{self, bold, txt, Tone};
use crate::i18n::{t, tf};
use crate::theme::{space, text, Palette};

/// What a picture looks like on this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShotView {
    Loading,
    Missing,
    Ready(SizedTexture),
}

enum Tex {
    Decoding(usize),
    Ready(usize, TextureHandle),
    Failed(usize),
}

type Decoded = (String, usize, Option<ColorImage>);

/// The widest preview kept as a texture: about twice the on-screen width of
/// a picture in a card, so it stays sharp at 150 % scaling. The magnifier
/// loads the full picture on demand and drops it when it closes.
pub const PREVIEW_MAX_W: usize = 1280;

/// Which texture of a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Downscaled to ≤ [`PREVIEW_MAX_W`], with mipmaps (cards).
    Preview,
    /// Full size, no mipmaps (the magnifier).
    Full,
}

/// The texture cache. `usize` = address of the PNG bytes' `Arc`, so a new
/// file under the same path (a re-run) is decoded again. Keys: the path for
/// a preview, `full:<path>` for a full-size picture.
pub struct ShotTextures {
    map: HashMap<String, Tex>,
    requested: HashSet<String>,
    tx: mpsc::Sender<Decoded>,
    rx: mpsc::Receiver<Decoded>,
    /// Decode on the calling thread (tests and snapshots: deterministic).
    pub sync: bool,
    /// `DESK_OPT=shots_full`: full size with mipmaps everywhere, kept for
    /// the session (the b8d06cb behaviour, for A/B runs).
    pub full_everywhere: bool,
    /// How many PNGs were decoded (QA: «декодируется один раз»).
    pub decoded: usize,
}

impl Default for ShotTextures {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            map: HashMap::new(),
            requested: HashSet::new(),
            tx,
            rx,
            sync: false,
            full_everywhere: false,
            decoded: 0,
        }
    }
}

/// PNG → RGBA. `None` for anything the decoder refuses.
pub fn decode_png(bytes: &[u8]) -> Option<ColorImage> {
    decode_png_max(bytes, None)
}

/// PNG → RGBA, box-downscaled by a whole factor until it is at most
/// `max_w` wide (the CPU image is dropped right after upload, so only the
/// smaller texture stays).
pub fn decode_png_max(bytes: &[u8], max_w: Option<usize>) -> Option<ColorImage> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[g, a]| [g, g, g, a])
            .collect(),
        // normalize_to_color8 expands palettes, so this is not reached.
        png::ColorType::Indexed => return None,
    };
    if rgba.len() != w * h * 4 {
        return None;
    }
    let k = max_w.map_or(1, |m| w.div_ceil(m.max(1)).max(1));
    if k == 1 {
        return Some(ColorImage::from_rgba_unmultiplied([w, h], &rgba));
    }
    let (sw, sh) = ((w / k).max(1), (h / k).max(1));
    Some(ColorImage::from_rgba_unmultiplied(
        [sw, sh],
        &box_down(&rgba, w, k, sw, sh),
    ))
}

/// Average k×k blocks of an RGBA image `w` wide into `sw` × `sh`.
fn box_down(rgba: &[u8], w: usize, k: usize, sw: usize, sh: usize) -> Vec<u8> {
    let mut out = vec![0u8; sw * sh * 4];
    let n = (k * k) as u32;
    for y in 0..sh {
        for x in 0..sw {
            let mut acc = [0u32; 4];
            for dy in 0..k {
                let row = ((y * k + dy) * w + x * k) * 4;
                for px in rgba[row..row + k * 4].chunks_exact(4) {
                    for c in 0..4 {
                        acc[c] += u32::from(px[c]);
                    }
                }
            }
            let o = (y * sw + x) * 4;
            for c in 0..4 {
                out[o + c] = ((acc[c] + n / 2) / n) as u8;
            }
        }
    }
    out
}

fn texture_options(mipmaps: bool) -> TextureOptions {
    TextureOptions {
        mipmap_mode: mipmaps.then_some(egui::TextureFilter::Linear),
        ..TextureOptions::LINEAR
    }
}

impl ShotTextures {
    /// The day changed: desk-app keeps only the open day's pictures, so ask
    /// again; and the old day's textures go (GPU and driver memory).
    pub fn forget_requests(&mut self) {
        self.requested.clear();
        if !self.full_everywhere {
            self.map.clear();
        }
    }

    /// The magnifier closed: drop the full-size pictures.
    pub fn release_full(&mut self) {
        self.map.retain(|k, _| !k.starts_with("full:"));
    }

    /// How many textures are alive (tests).
    pub fn live_textures(&self) -> usize {
        self.map
            .values()
            .filter(|t| matches!(t, Tex::Ready(..)))
            .count()
    }

    /// Upload what the workers finished. Call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((path, key, img)) = self.rx.try_recv() {
            self.finish(ctx, path, key, img);
        }
    }

    fn finish(
        &mut self,
        ctx: &egui::Context,
        map_key: String,
        key: usize,
        img: Option<ColorImage>,
    ) {
        // A newer file may have replaced this one meanwhile.
        if !matches!(self.map.get(&map_key), Some(Tex::Decoding(k)) if *k == key) {
            return;
        }
        let mipmaps = self.full_everywhere || !map_key.starts_with("full:");
        let tex = match img {
            Some(img) => {
                let h = ctx.load_texture(&map_key, img, texture_options(mipmaps));
                crate::perf::trace::mark("first_shot_uploaded");
                Tex::Ready(key, h)
            }
            None => Tex::Failed(key),
        };
        self.map.insert(map_key, tex);
    }

    /// The picture at `path` for a card, asking desk-app for it the first time.
    pub fn view(
        &mut self,
        ctx: &egui::Context,
        snap: &Snapshot,
        path: &str,
        out: &mut Vec<Command>,
    ) -> ShotView {
        self.view_kind(ctx, snap, path, out, Kind::Preview)
    }

    /// The picture at full size (the magnifier); the preview stands in while
    /// it decodes.
    pub fn view_full(
        &mut self,
        ctx: &egui::Context,
        snap: &Snapshot,
        path: &str,
        out: &mut Vec<Command>,
    ) -> ShotView {
        if self.full_everywhere {
            return self.view(ctx, snap, path, out);
        }
        match self.view_kind(ctx, snap, path, out, Kind::Full) {
            ShotView::Loading => match self.view(ctx, snap, path, out) {
                ShotView::Ready(t) => ShotView::Ready(t),
                _ => ShotView::Loading,
            },
            v => v,
        }
    }

    fn view_kind(
        &mut self,
        ctx: &egui::Context,
        snap: &Snapshot,
        path: &str,
        out: &mut Vec<Command>,
        kind: Kind,
    ) -> ShotView {
        let entry = snap
            .shots
            .get(path)
            .or_else(|| snap.shots.get(&path.replace('\\', "/")));
        let loaded = match entry {
            None => {
                if self.requested.insert(path.to_owned()) {
                    out.push(Command::LoadShot(path.to_owned()));
                }
                return ShotView::Loading;
            }
            Some(ShotState::Refused(_)) => return ShotView::Missing,
            Some(ShotState::Ready(l)) => l,
        };
        let map_key = match kind {
            Kind::Preview => path.to_owned(),
            Kind::Full => format!("full:{path}"),
        };
        let key = Arc::as_ptr(&loaded.png) as *const u8 as usize;
        match self.map.get(&map_key) {
            Some(Tex::Ready(k, h)) if *k == key => {
                return ShotView::Ready(SizedTexture::from_handle(h))
            }
            Some(Tex::Decoding(k)) if *k == key => return ShotView::Loading,
            Some(Tex::Failed(k)) if *k == key => return ShotView::Missing,
            _ => {}
        }
        self.map.insert(map_key.clone(), Tex::Decoding(key));
        self.decoded += 1;
        let max_w = (kind == Kind::Preview && !self.full_everywhere).then_some(PREVIEW_MAX_W);
        if self.sync {
            let img = decode_png_max(&loaded.png, max_w);
            self.finish(ctx, map_key, key, img);
            return self.view_kind(ctx, snap, path, out, kind);
        }
        let (tx, bytes, owned, repaint) = (
            self.tx.clone(),
            Arc::clone(&loaded.png),
            map_key.clone(),
            ctx.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("desk-png".into())
            .spawn(move || {
                let img = decode_png_max(&bytes, max_w);
                // The receiver lives as long as the app; a send after it is gone is moot.
                let _ = tx.send((owned, key, img));
                repaint.request_repaint();
            });
        if spawned.is_err() {
            // No thread to spare: decode here rather than show nothing.
            let img = decode_png_max(&loaded.png, max_w);
            self.finish(ctx, map_key, key, img);
        }
        ShotView::Loading
    }
}

// ---------------------------------------------------------------- pieces

fn css_hex(s: &str) -> Color32 {
    let h = s.trim().trim_start_matches('#');
    let n = |i: usize, len: usize| {
        u8::from_str_radix(h.get(i..i + len).unwrap_or("0"), 16).unwrap_or(0)
    };
    match h.len() {
        3 => Color32::from_rgb(n(0, 1) * 17, n(1, 1) * 17, n(2, 1) * 17),
        6 => Color32::from_rgb(n(0, 2), n(2, 2), n(4, 2)),
        8 => Color32::from_rgba_unmultiplied(n(0, 2), n(2, 2), n(4, 2), n(6, 2)),
        _ => Color32::GRAY,
    }
}

/// `ShotLegendList`: a line (solid / dashed) or a zone box per item. Each item
/// is measured first and allocated as one block, so the row wraps between
/// items instead of running past the card.
pub fn legend(ui: &mut Ui, p: &Palette, items: &[ShotLegend], size: f32) {
    if items.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(space::S8, space::S2);
        for l in items {
            let mut c = css_hex(&legend_color(l));
            // The skill's «level» grey is meant for a light chart; lift it on dark.
            if p.dark && l.kind == "level" {
                c = crate::theme::mix_oklab(c, Color32::WHITE, 0.55);
            }
            let dashed =
                l.style
                    .as_deref()
                    .unwrap_or(if l.kind == "open" { "dashed" } else { "solid" })
                    == "dashed";
            let galley = ui.painter().layout(
                l.label.clone(),
                FontId::proportional(size),
                p.fg,
                (ui.available_width() - 24.0).max(40.0),
            );
            let sw = 18.0;
            let gap = space::S3;
            let block = vec2(sw + gap + galley.size().x, galley.size().y.max(12.0));
            let (rect, _) = ui.allocate_exact_size(block, Sense::hover());
            let painter = ui.painter();
            let mark = Rect::from_min_size(
                pos2(
                    rect.left(),
                    rect.top() + galley.rows.first().map_or(block.y, |r| r.height()) / 2.0 - 6.0,
                ),
                vec2(sw, 12.0),
            );
            if l.kind == "zone" {
                let b = Rect::from_center_size(mark.center(), vec2(sw, 10.0));
                painter.rect(
                    b,
                    CornerRadius::same(2),
                    c.gamma_multiply(0.35),
                    Stroke::new(1.0, c),
                    egui::StrokeKind::Inside,
                );
            } else if dashed {
                painter.extend(egui::Shape::dashed_line(
                    &[mark.left_center(), mark.right_center()],
                    Stroke::new(2.0, c),
                    4.0,
                    3.0,
                ));
            } else {
                painter.line_segment(
                    [mark.left_center(), mark.right_center()],
                    Stroke::new(3.0, c),
                );
            }
            painter.galley(pos2(rect.left() + sw + gap, rect.top()), galley, p.fg);
            let _ = &l.label;
        }
    });
}

/// A picture as a button: click opens the magnifier.
fn frame(ui: &mut Ui, p: &Palette, view: ShotView, shot: &MacroShot, width: f32) -> bool {
    let label = shot_tab_label(&shot.tf, &shot.title, &shot.name);
    let size = match view {
        ShotView::Ready(t) => vec2(width, width * t.size.y / t.size.x.max(1.0)),
        _ => vec2(width, width * 7.0 / 12.0),
    };
    let sense = if matches!(view, ShotView::Ready(_)) {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let painter = ui.painter_at(rect);
    let r = CornerRadius::same(crate::theme::radius::SM);
    match view {
        ShotView::Ready(t) => {
            painter.image(
                t.id,
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            let border = if resp.hovered() {
                p.border_strong
            } else {
                p.border
            };
            painter.rect_stroke(rect, r, Stroke::new(1.0, border), egui::StrokeKind::Inside);
        }
        ShotView::Loading => {
            painter.rect_filled(rect, r, Color32::from_rgb(0x13, 0x17, 0x22));
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                t("macro.shotsLoading"),
                FontId::proportional(text::SM),
                Color32::from_rgb(0x8a, 0x93, 0xa6),
            );
        }
        ShotView::Missing => {
            painter.rect_filled(rect, r, p.bg_elev);
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                t("macro.shotsMissing"),
                FontId::proportional(text::SM),
                p.fg_muted,
            );
        }
    }
    if shot.verify_ok == Some(false) {
        let chip = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(0.0, 0.0));
        let galley = painter.layout_no_wrap(
            format!("⚠ {}", t("macro.shotsMoved")),
            FontId::proportional(text::SM),
            p.warn,
        );
        let bg = Rect::from_min_size(chip.min, galley.size() + vec2(16.0, 4.0));
        painter.rect_filled(bg, CornerRadius::same(255), p.warn_soft);
        painter.galley(bg.min + vec2(8.0, 2.0), galley, p.warn);
    }
    let resp = resp.on_hover_text(t("macro.shotsOpen"));
    resp.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::Role::Image,
            true,
            format!("{label}. {}", t("macro.shotsOpen")),
        )
    });
    resp.clicked()
}

fn shots_why(error: Option<&str>) -> String {
    match shots_error_text(error, SHOTS_ERROR_MAX) {
        (Some(ShotsErrorKind::DryRun), _) => t("macro.shotsDryRun").to_owned(),
        (Some(ShotsErrorKind::None), _) => t("macro.shotsNoReason").to_owned(),
        (None, s) => s,
    }
}

/// `ShotsInline`: one picture (M15) with its legend, or one quiet line.
/// Returns the index to open in the magnifier.
pub fn inline(
    ui: &mut Ui,
    p: &Palette,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    shots: Option<&Shots>,
    out: &mut Vec<Command>,
) -> Option<usize> {
    let shots = shots?;
    if !shots.ok {
        kit::para(
            ui,
            txt(
                tf(
                    "macro.shotsNone",
                    &[("why", &shots_why(shots.error.as_deref()))],
                ),
                text::SM,
                p.fg_muted,
            ),
        );
        return None;
    }
    let main = main_shot(&shots.shots)?;
    let view = tex.view(ui.ctx(), snap, &main.path, out);
    let open = frame(ui, p, view, main, ui.available_width());
    legend(ui, p, &main.legend, text::SM);
    open.then(|| {
        shots
            .shots
            .iter()
            .position(|s| std::ptr::eq(s, main))
            .unwrap_or(0)
    })
}

/// `ShotsCard`: every picture of the window, two per row when there is room.
pub fn card(
    ui: &mut Ui,
    p: &Palette,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    shots: Option<&Shots>,
    out: &mut Vec<Command>,
) -> Option<usize> {
    let shots = shots.filter(|s| s.ok && !s.shots.is_empty())?;
    let mut open = None;
    kit::card(
        ui,
        p,
        Some(t("macro.shotsTitle")),
        kit::CardKind::Plain,
        |ui| {
            let per_row = if ui.available_width() >= 2.0 * 320.0 + space::S8 {
                2
            } else {
                1
            };
            for (row, chunk) in shots.shots.chunks(per_row).enumerate() {
                kit::columns(ui, per_row, |k, ui| {
                    if let Some(s) = chunk.get(k) {
                        ui.label(bold(
                            shot_tab_label(&s.tf, &s.title, &s.name),
                            text::BASE,
                            p.fg,
                        ));
                        let view = tex.view(ui.ctx(), snap, &s.path, out);
                        if frame(ui, p, view, s, ui.available_width()) {
                            open = Some(row * per_row + k);
                        }
                        legend(ui, p, &s.legend, text::SM);
                    }
                });
            }
        },
    );
    open
}

// ---------------------------------------------------------------- magnifier

/// The magnifier's state: which picture, zoom and pan.
#[derive(Clone, Debug, PartialEq)]
pub struct Lightbox {
    /// Which list: the region (`eu`/`us`) or the evening's region.
    pub key: String,
    pub index: usize,
    pub zoom: f32,
    pub pan: Vec2,
}

impl Lightbox {
    pub fn new(key: impl Into<String>, index: usize) -> Self {
        Self {
            key: key.into(),
            index,
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }

    fn reset_view(&mut self) {
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    pub fn step(&mut self, len: usize, forward: bool) {
        if len > 1 {
            self.index = if forward {
                (self.index + 1) % len
            } else {
                (self.index + len - 1) % len
            };
            self.reset_view();
        }
    }
}

pub const ZOOM_MAX: f32 = 8.0;

/// The full-screen magnifier: tabs, the picture (wheel = zoom at the
/// pointer, drag = pan), legend, ←/→ between pictures, Esc / ✕ / a click on
/// the backdrop closes. Returns `false` when it should close.
pub fn lightbox(
    ctx: &egui::Context,
    p: &Palette,
    lb: &mut Lightbox,
    tex: &mut ShotTextures,
    snap: &Snapshot,
    shots: &Shots,
    out: &mut Vec<Command>,
) -> bool {
    let list = &shots.shots;
    if list.is_empty() {
        return false;
    }
    lb.index = lb.index.min(list.len() - 1);
    let (esc, left, right) = ctx.input(|i| {
        (
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
        )
    });
    if esc {
        return false;
    }
    if left || right {
        lb.step(list.len(), right);
    }
    let mut keep = true;
    let screen = ctx.content_rect();
    egui::Area::new(Id::NULL.with("desk-lightbox"))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let backdrop = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(
                screen,
                0.0,
                Color32::from_black_alpha(if p.dark { 175 } else { 120 }),
            );
            if backdrop.clicked() {
                keep = false;
            }
            let panel = screen.shrink(space::S12);
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(panel)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            egui::Frame::new()
                .fill(p.bg_raised)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(crate::theme::radius::MD))
                .inner_margin(egui::Margin::same(space::S6 as i8))
                .show(&mut child, |ui| {
                    ui.set_min_size(panel.size() - vec2(2.0 * space::S6, 2.0 * space::S6));
                    let shot = &list[lb.index];
                    ui.horizontal(|ui| {
                        if list.len() > 1 {
                            let tabs: Vec<String> = list
                                .iter()
                                .map(|s| shot_tab_label(&s.tf, &s.title, &s.name))
                                .collect();
                            let items: Vec<(&str, Tone)> =
                                tabs.iter().map(|s| (s.as_str(), Tone::Accent)).collect();
                            if let Some(i) = kit::seg(ui, p, &items, Some(lb.index), false) {
                                lb.index = i;
                                lb.reset_view();
                            }
                        } else {
                            ui.label(bold(
                                shot_tab_label(&shot.tf, &shot.title, &shot.name),
                                text::BASE,
                                p.fg,
                            ));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(txt("×", text::LG, p.fg)).frame(false))
                                .on_hover_text(t("macro.shotsClose"))
                                .clicked()
                            {
                                keep = false;
                            }
                            if (lb.zoom - 1.0).abs() > 1e-3
                                && kit::link(ui, p, t("desk.zoomReset")).clicked()
                            {
                                lb.reset_view();
                            }
                            if shot.verify_ok == Some(false) {
                                kit::chip(
                                    ui,
                                    p,
                                    &format!("⚠ {}", t("macro.shotsMoved")),
                                    Tone::Warn,
                                    text::SM,
                                );
                            }
                        });
                    });
                    let legend_h = if shot.legend.is_empty() { 0.0 } else { 44.0 };
                    let avail = vec2(
                        ui.available_width(),
                        (ui.available_height() - legend_h - 24.0).max(120.0),
                    );
                    let (area, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
                    let painter = ui.painter_at(area);
                    painter.rect_filled(
                        area,
                        CornerRadius::same(crate::theme::radius::SM),
                        Color32::from_rgb(0x13, 0x17, 0x22),
                    );
                    match tex.view_full(ui.ctx(), snap, &shot.path, out) {
                        ShotView::Ready(tx) => {
                            // Fit, then zoom around the pointer.
                            let fit = (area.width() / tx.size.x)
                                .min(area.height() / tx.size.y)
                                .min(1.0);
                            if resp.hovered() {
                                let (scroll, pinch) =
                                    ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                                let factor = pinch * (scroll * 0.0025).exp();
                                if (factor - 1.0).abs() > 1e-4 {
                                    let new = (lb.zoom * factor).clamp(1.0, ZOOM_MAX);
                                    if let Some(ptr) = resp.hover_pos() {
                                        let c = area.center() + lb.pan;
                                        let k = new / lb.zoom;
                                        lb.pan += (ptr - c) * (1.0 - k);
                                    }
                                    lb.zoom = new;
                                }
                            }
                            if resp.dragged() {
                                lb.pan += resp.drag_delta();
                            }
                            let size = tx.size * fit * lb.zoom;
                            // Keep the picture over the area: no panning into the void.
                            let slack = ((size - area.size()) / 2.0).max(Vec2::ZERO);
                            lb.pan = lb.pan.clamp(-slack, slack);
                            if lb.zoom <= 1.0 {
                                lb.pan = Vec2::ZERO;
                            }
                            let img = Rect::from_center_size(area.center() + lb.pan, size);
                            painter.image(
                                tx.id,
                                img,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        ShotView::Loading => {
                            painter.text(
                                area.center(),
                                Align2::CENTER_CENTER,
                                t("macro.shotsLoading"),
                                FontId::proportional(text::MD),
                                Color32::from_rgb(0x8a, 0x93, 0xa6),
                            );
                        }
                        ShotView::Missing => {
                            painter.text(
                                area.center(),
                                Align2::CENTER_CENTER,
                                t("macro.shotsMissing"),
                                FontId::proportional(text::MD),
                                Color32::from_rgb(0x8a, 0x93, 0xa6),
                            );
                        }
                    }
                    legend(ui, p, &shot.legend, text::MD);
                    ui.label(txt(t("desk.zoomHint"), text::SM, p.fg_muted));
                });
        });
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_colours() {
        assert_eq!(css_hex("#fff"), Color32::WHITE);
        assert_eq!(css_hex("#d32f2f"), Color32::from_rgb(0xd3, 0x2f, 0x2f));
        assert_eq!(css_hex("bad"), Color32::from_rgb(0xbb, 0xaa, 0xdd));
        assert_eq!(css_hex("#12"), Color32::GRAY);
    }

    #[test]
    fn lightbox_wraps_and_resets() {
        let mut lb = Lightbox::new("eu", 0);
        lb.zoom = 3.0;
        lb.step(2, false);
        assert_eq!((lb.index, lb.zoom), (1, 1.0));
        lb.step(2, true);
        assert_eq!(lb.index, 0);
        lb.step(1, true);
        assert_eq!(lb.index, 0, "one picture: arrows do nothing");
    }

    #[test]
    fn box_downscale_averages_blocks() {
        // 4×2 image: left half white, right half black → 2×1 at k = 2.
        let mut rgba = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let v = if x < 2 { 255 } else { 0 };
                rgba.extend([v, v, v, 255]);
            }
        }
        assert_eq!(
            box_down(&rgba, 4, 2, 2, 1),
            vec![255, 255, 255, 255, 0, 0, 0, 255]
        );
    }

    #[test]
    fn previews_are_capped() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/vault/indices-kb/shots");
        let png = walk(&dir).into_iter().next().unwrap();
        let bytes = std::fs::read(png).unwrap();
        let full = decode_png(&bytes).unwrap();
        let small = decode_png_max(&bytes, Some(full.size[0] / 2)).unwrap();
        assert_eq!(small.size, [full.size[0] / 2, full.size[1] / 2]);
        let same = decode_png_max(&bytes, Some(full.size[0])).unwrap();
        assert_eq!(same.size, full.size);
    }

    #[test]
    fn decodes_the_demo_vault_pngs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/vault/indices-kb/shots");
        let mut n = 0;
        for e in walk(&dir) {
            let bytes = std::fs::read(&e).unwrap();
            let img = decode_png(&bytes).unwrap_or_else(|| panic!("{e:?}"));
            assert!(img.size[0] > 0 && img.size[1] > 0);
            n += 1;
        }
        assert!(n > 0, "the demo vault has pictures");
        assert!(decode_png(b"not a png").is_none());
    }

    fn walk(d: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else if p.extension().is_some_and(|x| x == "png") {
                out.push(p);
            }
        }
        out
    }
}
