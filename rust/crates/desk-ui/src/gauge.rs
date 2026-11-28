//! The LONG / SHORT gauge (`src/renderer/macro/Gauge.tsx`): a half dial from
//! strong short (left) to strong long (right), five zones from the scale's
//! thresholds, a needle at the balance (long % − short %), plus the split bar
//! and the per-group bars that sit under it.
//!
//! Geometry is the SVG's: a 320 × 200 canvas, centre (160, 172), ring radii
//! 104..142, scaled to the width the widget gets (at most 380 px, like the CSS).

use desk_core::gauge::needle_angle;
use desk_core::zones::{clamp_needle, zone_of, zone_segments, ZoneKey, Zones};
use eframe::egui::{
    self, emath::easing, epaint::Mesh, pos2, vec2, Align2, AsIdSalt, Color32, CornerRadius, FontId,
    Id, IdSalt, Pos2, Rect, Response, RichText, Role, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo,
};

use crate::i18n::{t, tf};
use crate::theme::{mix_oklab, radius, space, text, Palette};

const W: f32 = 320.0;
const H: f32 = 200.0;
const CX: f32 = W / 2.0;
const CY: f32 = 172.0;
const R0: f32 = 104.0;
const R1: f32 = 142.0;
const MAX_WIDTH: f32 = 380.0;

/// How long the needle travels to a new value, seconds (TZ item 2).
pub const NEEDLE_TRAVEL_S: f32 = 0.3;

/// Longest chord of the ring's outer rim, in physical pixels: below this the
/// polygon reads as a circle at any scale.
const MAX_CHORD_PX: f32 = 1.5;

/// The opaque fill of a zone on the dial. One family: each colour is its pole
/// (`--err` / `--ok`) or the neutral grey mixed toward the card in oklab, so
/// the five steps share a lightness ramp; zones the needle is not in sit
/// further toward the card (the CSS did that with `opacity: .55`, which let
/// the seams show through).
pub fn dial_fill(p: &Palette, z: ZoneKey, active: bool) -> Color32 {
    let base = match z {
        ZoneKey::StrongShort => p.short_strong,
        ZoneKey::Short => mix_oklab(p.short_strong, p.bg_raised, 0.55),
        ZoneKey::Neutral => p.neutral_zone,
        ZoneKey::Long => mix_oklab(p.long_strong, p.bg_raised, 0.55),
        ZoneKey::StrongLong => p.long_strong,
    };
    if active {
        base
    } else {
        mix_oklab(base, p.bg_raised, 0.6)
    }
}

/// `signed()` of Gauge.tsx: rounded, real minus sign, `%` glued on.
pub fn signed(n: f32) -> String {
    let r = n.round();
    let sign = if r > 0.0 {
        "+"
    } else if r < 0.0 {
        "\u{2212}"
    } else {
        ""
    };
    format!("{sign}{}%", r.abs() as i64)
}

/// The colour of the zone's caption (`.zone-* .gauge-zone`).
pub fn zone_text(p: &Palette, z: ZoneKey) -> Color32 {
    match z {
        ZoneKey::StrongShort | ZoneKey::Short => p.err,
        ZoneKey::Neutral => p.fg,
        ZoneKey::Long | ZoneKey::StrongLong => p.ok,
    }
}

pub fn zone_label(z: ZoneKey) -> &'static str {
    match z {
        ZoneKey::StrongShort => t("macro.zone.strong-short"),
        ZoneKey::Short => t("macro.zone.short"),
        ZoneKey::Neutral => t("macro.zone.neutral"),
        ZoneKey::Long => t("macro.zone.long"),
        ZoneKey::StrongLong => t("macro.zone.strong-long"),
    }
}

// ---------------------------------------------------------------- animation

/// Per-gauge animation state kept in egui's temp memory.
///
/// `ctx.animate_value_with_time` gives a value that moves *linearly* to the
/// target; this maps its progress through `cubic_out` so the needle eases in
/// like the CSS `--ease` did, and a retarget mid-flight starts from where the
/// needle is drawn, not from where it was headed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NeedleAnim {
    /// Value drawn when the current travel started.
    from: f32,
    /// Where the needle is going.
    to: f32,
    /// The linear animation's value when the travel started.
    lin_from: f32,
    /// The linear value of the previous frame.
    last_lin: f32,
    /// What was drawn last frame.
    pub(crate) shown: f32,
}

impl NeedleAnim {
    fn settled(v: f32) -> Self {
        Self {
            from: v,
            to: v,
            lin_from: v,
            last_lin: v,
            shown: v,
        }
    }

    /// One frame: `lin` is this frame's linear value toward `target`.
    pub(crate) fn step(&mut self, target: f32, lin: f32) -> f32 {
        if target != self.to {
            self.from = self.shown;
            self.to = target;
            self.lin_from = self.last_lin;
        }
        let span = self.to - self.lin_from;
        let p = if span.abs() < f32::EPSILON {
            1.0
        } else {
            ((lin - self.lin_from) / span).clamp(0.0, 1.0)
        };
        // Land exactly on the target: `from + (to - from) * 1.0` can miss it
        // by a rounding error, and a settled needle must equal the file value.
        self.shown = if p >= 1.0 {
            self.to
        } else {
            self.from + (self.to - self.from) * easing::cubic_out(p)
        };
        self.last_lin = lin;
        self.shown
    }
}

/// The value to draw this frame for a needle headed to `target`.
pub(crate) fn animated_needle(ctx: &egui::Context, id: Id, target: f32) -> f32 {
    let lin = ctx.animate_value_with_time(id, target, NEEDLE_TRAVEL_S);
    let mut anim = ctx
        .data(|d| d.get_temp::<NeedleAnim>(id))
        .unwrap_or_else(|| NeedleAnim::settled(target));
    let shown = anim.step(target, lin);
    ctx.data_mut(|d| d.insert_temp(id, anim));
    shown
}

// ---------------------------------------------------------------- the dial

/// The Fear & Greed dial.
pub struct Gauge<'a> {
    value: f32,
    before: Option<f32>,
    zones: Zones,
    mini: bool,
    label: Option<&'a str>,
    id_salt: IdSalt,
}

impl<'a> Gauge<'a> {
    /// `value` is the needle, −100..100 (clamped); `zones` come from the scale config.
    pub fn new(value: f32, zones: Zones) -> Self {
        Self {
            value,
            before: None,
            zones,
            mini: false,
            label: None,
            id_salt: IdSalt::new("gauge"),
        }
    }

    /// The needle before the discussion; drawn as a dashed ghost when it differs.
    pub fn before(mut self, before: Option<f32>) -> Self {
        self.before = before;
        self
    }

    /// Small variant for the window strip: no end labels, no caption.
    pub fn mini(mut self, mini: bool) -> Self {
        self.mini = mini;
        self
    }

    /// Accessible name; defaults to «Перевес лонг / шорт».
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Needed when one screen shows several gauges (each animates on its own).
    pub fn id_salt(mut self, salt: impl AsIdSalt) -> Self {
        self.id_salt = IdSalt::new(salt);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let p = Palette::of(ui);
        let target = clamp_needle(f64::from(self.value)) as f32;
        let zone = zone_of(f64::from(target), &self.zones);
        let ghost = self
            .before
            .map(|b| clamp_needle(f64::from(b)) as f32)
            .filter(|b| b.round() != target.round());

        let canvas_h = if self.mini { CY + 8.0 } else { H };
        let width = ui.available_width().min(MAX_WIDTH);
        let s = width / W;
        let id = ui.make_persistent_id(self.id_salt);
        let shown = animated_needle(ui.ctx(), id, target);

        ui.vertical_centered(|ui| {
            let (rect, response) =
                ui.allocate_exact_size(vec2(width, canvas_h * s), Sense::hover());
            if ui.is_rect_visible(rect) {
                paint_dial(ui, &p, rect, s, &self, zone, shown, ghost);
            }
            if !self.mini {
                // `figcaption` sits 6 px into the canvas' bottom margin.
                ui.add_space(-6.0 * s);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(
                    RichText::new(signed(target))
                        .size(text::GAUGE_VALUE)
                        .color(p.fg)
                        .strong(),
                );
                ui.label(
                    RichText::new(zone_label(zone))
                        .size(text::LG)
                        .color(zone_text(&p, zone)),
                );
                if let Some(b) = ghost {
                    ui.label(
                        RichText::new(tf("macro.beforeTalk", &[("v", &signed(b))]))
                            .size(text::SM)
                            .color(p.fg_muted),
                    );
                }
            }
            let name = format!(
                "{}: {}, {}",
                self.label.unwrap_or(t("macro.gauge")),
                signed(target),
                zone_label(zone)
            );
            response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &name));
            response
        })
        .inner
    }
}

/// One sample along the ring: an angle (degrees, mathematical) and the zone
/// whose colour the vertices there carry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RingSample {
    pub angle: f64,
    pub key: ZoneKey,
}

/// The ring's angles from 180° (strong short) to 0° (strong long), at most
/// `max_step_deg` apart. Every zone boundary is sampled twice at the very
/// same angle, once per zone, so the colour changes with a hard edge and
/// no gap. A zone of zero width (or inverted thresholds) adds nothing.
pub(crate) fn ring_samples(zones: &Zones, max_step_deg: f64) -> Vec<RingSample> {
    let step = max_step_deg.max(0.05);
    let mut out = Vec::new();
    for seg in zone_segments(zones) {
        let (a0, a1) = (needle_angle(seg.from), needle_angle(seg.to));
        if a0 <= a1 {
            continue;
        }
        let n = ((a0 - a1) / step).ceil().max(1.0) as usize;
        out.extend((0..=n).map(|i| RingSample {
            angle: a0 + (a1 - a0) * i as f64 / n as f64,
            key: seg.key,
        }));
    }
    out
}

/// The ring in screen points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    pub center: Pos2,
    pub r0: f32,
    pub r1: f32,
    /// Width of the anti-aliasing fringe: one physical pixel.
    pub feather: f32,
}

impl Ring {
    fn at(&self, r: f32, angle_deg: f64) -> Pos2 {
        let a = angle_deg.to_radians() as f32;
        pos2(self.center.x + r * a.cos(), self.center.y - r * a.sin())
    }

    /// The largest angular step that keeps the outer chord under `MAX_CHORD_PX`.
    pub fn max_step_deg(&self, pixels_per_point: f32) -> f64 {
        let r_px = (self.r1 * pixels_per_point).max(1.0);
        f64::from(MAX_CHORD_PX / r_px).to_degrees()
    }
}

/// The whole dial as ONE mesh: per sample four vertices across the ring —
/// transparent fringe, solid inner rim, solid outer rim, transparent fringe —
/// so only the outer and inner rims (and the two end caps) are feathered,
/// by exactly one physical pixel, and nothing inside the ring is.
pub(crate) fn ring_mesh(
    ring: &Ring,
    samples: &[RingSample],
    fill: impl Fn(ZoneKey) -> Color32,
) -> Mesh {
    let mut mesh = Mesh::default();
    if samples.len() < 2 {
        return mesh;
    }
    let f = ring.feather;
    let radii = [
        ring.r0 - f / 2.0,
        ring.r0 + f / 2.0,
        ring.r1 - f / 2.0,
        ring.r1 + f / 2.0,
    ];
    let clear = Color32::TRANSPARENT;

    // A column of four vertices; returns the index of its first one.
    let column = |mesh: &mut Mesh, pos: [Pos2; 4], color: Color32| -> u32 {
        let base = mesh.vertices.len() as u32;
        let colors = [clear, color, color, clear];
        for (p, c) in pos.into_iter().zip(colors) {
            mesh.colored_vertex(p, c);
        }
        base
    };
    // Three quads between two columns.
    let strip = |mesh: &mut Mesh, a: u32, b: u32| {
        for k in 0..3 {
            mesh.add_triangle(a + k, a + k + 1, b + k);
            mesh.add_triangle(a + k + 1, b + k + 1, b + k);
        }
    };
    let ring_col = |angle: f64| radii.map(|r| ring.at(r, angle));
    // End caps: the end column pushed one fringe width along the tangent,
    // away from the arc, all transparent.
    let cap = |angle: f64, outward: f32| {
        let a = angle.to_radians() as f32;
        // d/dangle of (cos a, −sin a) on a y-down screen.
        let tangent = vec2(-a.sin(), -a.cos()) * outward;
        ring_col(angle).map(|p| p + tangent * f)
    };

    let first = samples[0];
    let last = samples[samples.len() - 1];
    let mut prev = column(&mut mesh, cap(first.angle, 1.0), clear);
    let mut prev_angle = f64::NAN;
    for s in samples {
        let col = column(&mut mesh, ring_col(s.angle), fill(s.key));
        // A boundary pair shares its angle: only the colour changes there.
        if s.angle != prev_angle {
            strip(&mut mesh, prev, col);
        }
        prev = col;
        prev_angle = s.angle;
    }
    let end = column(&mut mesh, cap(last.angle, -1.0), clear);
    strip(&mut mesh, prev, end);
    mesh
}

#[allow(clippy::too_many_arguments)] // one private painter, the arguments are its frame
fn paint_dial(
    ui: &Ui,
    p: &Palette,
    rect: Rect,
    s: f32,
    g: &Gauge<'_>,
    zone: ZoneKey,
    shown: f32,
    ghost: Option<f32>,
) {
    let painter = ui.painter_at(rect.expand(2.0));
    let at = |q: Pos2| rect.min + vec2(q.x, q.y) * s;
    let ppp = ui.ctx().pixels_per_point();

    let ring = Ring {
        center: at(pos2(CX, CY)),
        r0: R0 * s,
        r1: R1 * s,
        feather: 1.0 / ppp,
    };
    let samples = ring_samples(&g.zones, ring.max_step_deg(ppp));
    painter.add(Shape::mesh(ring_mesh(&ring, &samples, |k| {
        dial_fill(p, k, k == zone)
    })));

    if !g.mini {
        let font = FontId::proportional(12.0 * s.max(0.75));
        let y = CY + 16.0;
        painter.text(
            at(pos2(CX - R1 + 6.0, y)),
            Align2::LEFT_CENTER,
            t("macro.short").to_uppercase(),
            font.clone(),
            p.fg_muted,
        );
        painter.text(
            at(pos2(CX + R1 - 6.0, y)),
            Align2::RIGHT_CENTER,
            t("macro.long").to_uppercase(),
            font,
            p.fg_muted,
        );
    }

    if let Some(b) = ghost {
        let mut outline: Vec<Pos2> = needle_points(b).into_iter().map(at).collect();
        outline.push(outline[0]);
        painter.extend(Shape::dashed_line(
            &outline,
            Stroke::new(1.5, p.fg_muted),
            4.0,
            3.0,
        ));
    }
    let needle: Vec<Pos2> = needle_points(shown).into_iter().map(at).collect();
    painter.add(Shape::convex_polygon(needle, p.fg, Stroke::NONE));
    // The hub goes on top, so the needle's base disappears under it.
    let hub_r = if g.mini { 10.0 } else { 9.0 };
    painter.circle(
        at(pos2(CX, CY)),
        hub_r * s,
        p.fg,
        Stroke::new(3.0 * s, p.bg_raised),
    );
}

/// Half-width of the needle at the hub and at the tip, canvas units.
const NEEDLE_BASE: f32 = 5.0;
const NEEDLE_TIP: f32 = 1.2;
/// The tip stops this far inside the ring's inner rim.
const NEEDLE_GAP: f32 = 6.0;

/// The needle's outline in canvas units: one tapered quad whose base is
/// centred on the hub and whose tip stops just short of the inner rim,
/// rotated to the value's angle.
fn needle_points(value: f32) -> Vec<Pos2> {
    let a = needle_angle(f64::from(value)).to_radians() as f32;
    let (sin, cos) = a.sin_cos();
    // Drawn pointing right, then rotated counter-clockwise on a y-down screen.
    let rot = |dx: f32, dy: f32| pos2(CX + dx * cos + dy * sin, CY - dx * sin + dy * cos);
    let tip = R0 - NEEDLE_GAP;
    vec![
        rot(0.0, -NEEDLE_BASE),
        rot(tip, -NEEDLE_TIP),
        rot(tip, NEEDLE_TIP),
        rot(0.0, NEEDLE_BASE),
    ]
}

// ---------------------------------------------------------------- split bar

/// Long / undecided / short as one stacked bar, with a legend under it.
pub fn split_bar(ui: &mut Ui, long: f32, short: f32, undecided: f32) -> Response {
    let p = Palette::of(ui);
    let (long, short, undecided) = (long.max(0.0), short.max(0.0), undecided.max(0.0));
    let total = (long + short + undecided).max(1.0);
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let r = CornerRadius::same(6);
    painter.rect_filled(rect, r, p.track);

    let parts = [
        (long, p.long_strong),
        (undecided, p.neutral_zone),
        (short, p.short_strong),
    ];
    let visible: Vec<_> = parts.iter().filter(|(v, _)| *v > 0.0).collect();
    let gap = 2.0;
    let usable = rect.width() - gap * (visible.len().saturating_sub(1)) as f32;
    let mut x = rect.left();
    for (i, (v, color)) in visible.iter().enumerate() {
        let w = usable * *v / total;
        let seg = Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height()));
        let corners = CornerRadius {
            nw: if i == 0 { 6 } else { 0 },
            sw: if i == 0 { 6 } else { 0 },
            ne: if i + 1 == visible.len() { 6 } else { 0 },
            se: if i + 1 == visible.len() { 6 } else { 0 },
        };
        painter.rect_filled(seg, corners, *color);
        x += w + gap;
    }

    let aria = tf(
        "macro.splitAria",
        &[
            ("l", &long.round()),
            ("u", &undecided.round()),
            ("s", &short.round()),
        ],
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &aria));

    ui.add_space(space::S3 - ui.spacing().item_spacing.y);
    legend_row(
        ui,
        &p,
        [
            (p.long_strong, t("macro.forLong"), long),
            (p.neutral_zone, t("macro.undecided"), undecided),
            (p.short_strong, t("macro.forShort"), short),
        ],
    );
    response
}

/// `.split-legend`: three «dot label N %» items, spread like
/// `justify-content: space-between`. Painted by hand so the right item reads
/// left to right inside a right-aligned slot.
fn legend_row(ui: &mut Ui, p: &Palette, items: [(Color32, &str, f32); 3]) {
    let dot = 8.0;
    let galleys: Vec<_> = items
        .iter()
        .map(|(_, label, v)| {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                label,
                0.0,
                egui::TextFormat::simple(FontId::proportional(text::SM), p.fg_muted),
            );
            job.append(
                &format!("{} %", v.round()),
                space::S2,
                egui::TextFormat::simple(FontId::proportional(text::SM), p.fg),
            );
            ui.fonts_mut(|f| f.layout_job(job))
        })
        .collect();
    let h = galleys.iter().map(|g| g.size().y).fold(dot, f32::max);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let painter = ui.painter_at(rect);
    for (i, ((color, _, _), galley)) in items.iter().zip(galleys).enumerate() {
        let w = dot + space::S2 + galley.size().x;
        let x = match i {
            0 => rect.left(),
            1 => rect.center().x - w / 2.0,
            _ => rect.right() - w,
        };
        let dot_rect = Rect::from_center_size(pos2(x + dot / 2.0, rect.center().y), vec2(dot, dot));
        painter.rect_filled(dot_rect, CornerRadius::same(2), *color);
        painter.galley(
            pos2(x + dot + space::S2, rect.center().y - galley.size().y / 2.0),
            galley,
            p.fg,
        );
    }
}

// ---------------------------------------------------------------- group bars

/// One diverging bar per group: short to the left of the centre line, long
/// to the right, both on the same scale (`scale` = the largest group share).
pub fn group_bar(
    ui: &mut Ui,
    name: &str,
    share: f32,
    long: f32,
    short: f32,
    scale: f32,
) -> Response {
    let p = Palette::of(ui);
    // `.gbar { grid-template-columns: 150px 1fr 56px; gap: 10px }`
    let (name_w, net_w, gap) = (150.0, 56.0, space::S5);
    let row_h = 20.0;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::hover());
    let painter = ui.painter_at(rect);

    let mut job = egui::text::LayoutJob::default();
    job.append(
        name,
        0.0,
        egui::TextFormat::simple(FontId::proportional(text::MD), p.fg),
    );
    job.append(
        &format!("{} %", share.round()),
        space::S2,
        egui::TextFormat::simple(FontId::proportional(text::SM), p.fg_muted),
    );
    job.wrap.max_width = name_w;
    job.wrap.max_rows = 1;
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    painter.galley(
        pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
        galley,
        p.fg,
    );

    let track = Rect::from_min_max(
        pos2(rect.left() + name_w + gap, rect.center().y - 6.0),
        pos2(rect.right() - net_w - gap, rect.center().y + 6.0),
    );
    painter.rect_filled(track, CornerRadius::same(radius::XS), p.track);
    let half = track.width() / 2.0;
    let frac = |n: f32| (n.max(0.0) / scale.max(1.0)).min(1.0) * half;
    let cx = track.center().x;
    let r = radius::XS;
    if short > 0.0 {
        let bar = Rect::from_min_max(
            pos2(cx - frac(short), track.top()),
            pos2(cx, track.bottom()),
        );
        painter.rect_filled(
            bar,
            CornerRadius {
                nw: r,
                sw: r,
                ne: 0,
                se: 0,
            },
            p.short_strong,
        );
    }
    if long > 0.0 {
        let bar = Rect::from_min_max(pos2(cx, track.top()), pos2(cx + frac(long), track.bottom()));
        painter.rect_filled(
            bar,
            CornerRadius {
                nw: 0,
                sw: 0,
                ne: r,
                se: r,
            },
            p.long_strong,
        );
    }
    painter.line_segment(
        [pos2(cx, track.top() - 3.0), pos2(cx, track.bottom() + 3.0)],
        Stroke::new(1.0, p.fg_muted),
    );

    let net = long - short;
    let net_color = if net.round() > 0.0 {
        p.ok
    } else if net.round() < 0.0 {
        p.err
    } else {
        p.fg
    };
    painter.text(
        pos2(rect.right(), rect.center().y),
        Align2::RIGHT_CENTER,
        signed(net),
        FontId::proportional(text::MD),
        net_color,
    );

    let tip = format!(
        "{name}: {} {} % · {} {} · {} {}",
        t("macro.share"),
        share.round(),
        t("macro.forLong"),
        long,
        t("macro.forShort"),
        short
    );
    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, &tip));
    response.on_hover_text(tip)
}

/// Size the dial takes for a given width (tests and layout code).
pub fn dial_size(width: f32, mini: bool) -> Vec2 {
    let w = width.min(MAX_WIDTH);
    vec2(w, (if mini { CY + 8.0 } else { H }) * w / W)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_is_settled() {
        let mut a = NeedleAnim::settled(30.0);
        assert_eq!(a.step(30.0, 30.0), 30.0);
    }

    #[test]
    fn needle_eases_out() {
        let mut a = NeedleAnim::settled(0.0);
        // The linear driver goes 0 → 100; the eased value leads it.
        let mid = a.step(100.0, 50.0);
        assert!(mid > 50.0 && mid < 100.0, "{mid}");
        assert_eq!(a.step(100.0, 100.0), 100.0);
    }

    #[test]
    fn retarget_starts_from_drawn_value() {
        let mut a = NeedleAnim::settled(0.0);
        let drawn = a.step(100.0, 40.0);
        // New target arrives; the linear driver restarts from its own value.
        let next = a.step(-50.0, 40.0);
        assert!((next - drawn).abs() < 1e-3, "no jump: {drawn} → {next}");
        assert_eq!(a.step(-50.0, -50.0), -50.0);
    }

    #[test]
    fn needle_points_follow_angle() {
        // +100 → needle points right: the tip is right of the hub, level with it.
        let pts = needle_points(100.0);
        assert!(pts[1].x > CX + 90.0 && (pts[1].y - (CY - NEEDLE_TIP)).abs() < 1e-3);
        // 0 → straight up.
        let pts = needle_points(0.0);
        assert!(pts[1].y < CY - 90.0);
        // −100 → left.
        assert!(needle_points(-100.0)[1].x < CX - 90.0);
    }

    #[test]
    fn needle_base_is_centred_and_tip_inside_the_ring() {
        for v in [-80.0, 0.0, 30.0, 100.0] {
            let pts = needle_points(v);
            let base_mid = pts[0].lerp(pts[3], 0.5);
            assert!(
                base_mid.distance(pos2(CX, CY)) < 1e-3,
                "{v}: base off the hub"
            );
            let tip_mid = pts[1].lerp(pts[2], 0.5);
            let r = tip_mid.distance(pos2(CX, CY));
            assert!(r < R0 && r > R0 - 10.0, "{v}: tip at r={r}");
        }
    }

    #[test]
    fn signed_matches_ts() {
        assert_eq!(signed(30.4), "+30%");
        assert_eq!(signed(-80.0), "\u{2212}80%");
        assert_eq!(signed(0.2), "0%");
        assert_eq!(signed(-0.2), "0%");
    }

    fn zones() -> Zones {
        Zones::new(15.0, 40.0)
    }

    #[test]
    fn ring_boundaries_match_desk_core_zones() {
        let samples = ring_samples(&zones(), 2.0);
        let segs = zone_segments(&zones());
        assert_eq!(samples.first().map(|s| s.angle), Some(180.0));
        assert_eq!(samples.last().map(|s| s.angle), Some(0.0));
        for w in segs.windows(2) {
            let a = needle_angle(w[0].to);
            // The boundary is sampled twice at the same angle: last of the
            // left zone, first of the right one.
            let i = samples
                .iter()
                .position(|s| s.key == w[1].key)
                .expect("each zone is sampled");
            assert_eq!(samples[i - 1].angle, a);
            assert_eq!(samples[i].angle, a);
            assert_eq!(samples[i - 1].key, w[0].key);
        }
    }

    #[test]
    fn ring_has_no_gaps_and_small_steps() {
        let samples = ring_samples(&zones(), 1.0);
        for w in samples.windows(2) {
            let step = w[0].angle - w[1].angle;
            assert!((0.0..=1.0 + 1e-9).contains(&step), "step {step}");
            if step == 0.0 {
                assert_ne!(w[0].key, w[1].key, "only boundaries repeat an angle");
            }
        }
    }

    #[test]
    fn ring_skips_empty_zones() {
        // neutral == strong: the plain long/short zones vanish, no duplicates of them.
        let s = ring_samples(&Zones::new(40.0, 40.0), 2.0);
        assert!(!s
            .iter()
            .any(|x| x.key == ZoneKey::Short || x.key == ZoneKey::Long));
    }

    #[test]
    fn mesh_columns_share_positions_at_boundaries() {
        let ring = Ring {
            center: pos2(200.0, 200.0),
            r0: 104.0,
            r1: 142.0,
            feather: 0.5,
        };
        let samples = ring_samples(&zones(), ring.max_step_deg(2.0));
        let mesh = ring_mesh(&ring, &samples, |k| match k {
            ZoneKey::StrongShort => Color32::RED,
            ZoneKey::Short => Color32::LIGHT_RED,
            ZoneKey::Neutral => Color32::GRAY,
            ZoneKey::Long => Color32::LIGHT_GREEN,
            ZoneKey::StrongLong => Color32::GREEN,
        });
        // cap + one column per sample + cap, four vertices each.
        assert_eq!(mesh.vertices.len(), (samples.len() + 2) * 4);
        let col = |i: usize| &mesh.vertices[(i + 1) * 4..(i + 2) * 4];
        let mut boundaries = 0;
        for i in 1..samples.len() {
            if samples[i].angle == samples[i - 1].angle {
                boundaries += 1;
                let (a, b) = (col(i - 1), col(i));
                for k in 0..4 {
                    assert_eq!(a[k].pos, b[k].pos, "boundary column moved");
                }
                assert_ne!(a[1].color, b[1].color, "hard colour edge");
            } else {
                // Neighbouring columns are less than 1.5 px apart on the outer rim at 2x.
                let d = col(i - 1)[2].pos.distance(col(i)[2].pos) * 2.0;
                assert!(d <= MAX_CHORD_PX + 1e-3, "chord {d}px");
            }
        }
        assert_eq!(boundaries, 4);
        // Only the fringe vertices are transparent; the rims are solid.
        for c in (0..samples.len()).map(col) {
            assert_eq!(c[0].color, Color32::TRANSPARENT);
            assert_eq!(c[3].color, Color32::TRANSPARENT);
            assert_ne!(c[1].color, Color32::TRANSPARENT);
            assert_ne!(c[2].color, Color32::TRANSPARENT);
        }
        // Every triangle index is in range.
        assert!(mesh
            .indices
            .iter()
            .all(|&i| (i as usize) < mesh.vertices.len()));
    }

    #[test]
    fn zone_fills_form_one_ramp() {
        for p in [Palette::light(), Palette::dark()] {
            let card = crate::theme::oklab_l(p.bg_raised);
            let l = |k| crate::theme::oklab_l(dial_fill(&p, k, true));
            // Strong zones are furthest from the card, soft zones in between.
            for (strong, soft) in [
                (ZoneKey::StrongShort, ZoneKey::Short),
                (ZoneKey::StrongLong, ZoneKey::Long),
            ] {
                let (ds, dm) = ((l(strong) - card).abs(), (l(soft) - card).abs());
                assert!(ds > dm, "strong {ds} vs soft {dm}");
            }
            // The neutral zone stays close to the card.
            assert!((l(ZoneKey::Neutral) - card).abs() < 0.2);
            if p.dark {
                assert!(l(ZoneKey::Neutral) > card, "neutral lighter than the card");
            }
            // Inactive zones sit toward the card.
            let idle = crate::theme::oklab_l(dial_fill(&p, ZoneKey::StrongLong, false));
            assert!((idle - card).abs() < (l(ZoneKey::StrongLong) - card).abs());
        }
    }

    #[test]
    fn dial_is_capped_at_css_width() {
        assert_eq!(dial_size(1000.0, false), vec2(380.0, 380.0 * H / W));
        assert_eq!(dial_size(320.0, true), vec2(320.0, CY + 8.0));
    }
}
