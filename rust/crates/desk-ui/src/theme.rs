//! The Desk theme: the colour tokens of `src/renderer/styles.css` (light and
//! dark) and `macro.css`, plus the spacing / radius / text scale.
//!
//! Widgets read colours through [`Palette::of`], never from egui's built-in
//! visuals, so a screen looks the same as the Electron desk it replaces.

use eframe::egui::{
    self, Color32, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme,
    ThemePreference, Visuals,
};

/// `--space-*` in pixels.
pub mod space {
    pub const S1: f32 = 2.0;
    pub const S2: f32 = 4.0;
    pub const S3: f32 = 6.0;
    pub const S4: f32 = 8.0;
    pub const S5: f32 = 10.0;
    pub const S6: f32 = 12.0;
    pub const S8: f32 = 16.0;
    pub const S12: f32 = 24.0;
}

/// `--radius-*` in pixels.
pub mod radius {
    pub const XS: u8 = 4;
    pub const SM: u8 = 6;
    pub const MD: u8 = 12;
}

/// `--text-*` in pixels.
pub mod text {
    pub const XS: f32 = 11.0;
    pub const SM: f32 = 12.0;
    pub const MD: f32 = 13.0;
    pub const BASE: f32 = 14.0;
    pub const LG: f32 = 15.0;
    pub const HEADING: f32 = 18.0;
    pub const TITLE: f32 = 22.0;
    /// `.gauge-value`.
    pub const GAUGE_VALUE: f32 = 34.0;
}

/// The sidebar column of `.app { grid-template-columns: 300px 1fr }`.
pub const SIDEBAR_WIDTH: f32 = 300.0;

/// Every colour token the desk paints with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub bg_panel: Color32,
    pub bg_elev: Color32,
    pub bg_input: Color32,
    pub bg_hover: Color32,
    pub bg_sidebar: Color32,
    pub bg_chat: Color32,
    pub bg_raised: Color32,
    pub fg: Color32,
    pub fg_muted: Color32,
    pub border: Color32,
    pub border_control: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub accent_solid: Color32,
    pub accent_fg: Color32,
    pub ok: Color32,
    pub err: Color32,
    pub warn: Color32,
    pub ok_soft: Color32,
    pub err_soft: Color32,
    pub warn_soft: Color32,
    pub code_bg: Color32,
    /// `--track` = `--bg-elev`.
    pub track: Color32,
    // macro.css: the diverging gauge scale.
    pub short_strong: Color32,
    pub short_soft: Color32,
    pub long_strong: Color32,
    pub long_soft: Color32,
    pub neutral_zone: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

/// Light tokens (`:root`).
struct Base {
    bg: u32,
    bg_panel: u32,
    bg_elev: u32,
    bg_input: u32,
    bg_hover: u32,
    bg_sidebar: u32,
    bg_chat: u32,
    bg_raised: u32,
    fg: u32,
    fg_muted: u32,
    border: u32,
    border_control: u32,
    border_strong: u32,
    accent: u32,
    accent_soft: u32,
    accent_solid: u32,
    accent_fg: u32,
    ok: u32,
    err: u32,
    warn: u32,
    ok_soft: u32,
    err_soft: u32,
    warn_soft: u32,
    code_bg: u32,
}

const LIGHT: Base = Base {
    bg: 0xf3f5f9,
    bg_panel: 0xffffff,
    bg_elev: 0xedf1f6,
    bg_input: 0xffffff,
    bg_hover: 0xe3e9f1,
    bg_sidebar: 0xe6ecf5,
    bg_chat: 0xffffff,
    bg_raised: 0xffffff,
    fg: 0x192538,
    fg_muted: 0x526176,
    border: 0xd5dce5,
    border_control: 0xafb9c6,
    border_strong: 0x778599,
    accent: 0x4338ca,
    accent_soft: 0xeeedff,
    accent_solid: 0x5146db,
    accent_fg: 0xffffff,
    ok: 0x087465,
    err: 0xb52d43,
    warn: 0x885600,
    ok_soft: 0xe3f2ec,
    err_soft: 0xfcebef,
    warn_soft: 0xfbf0da,
    code_bg: 0xedf1f6,
};

const DARK: Base = Base {
    bg: 0x101620,
    bg_panel: 0x18212f,
    bg_elev: 0x202c3e,
    bg_input: 0x121b28,
    bg_hover: 0x29354a,
    bg_sidebar: 0x0b1220,
    bg_chat: 0x161f2e,
    bg_raised: 0x202b3c,
    fg: 0xe8edf5,
    fg_muted: 0xa6b2c6,
    border: 0x2e3b51,
    border_control: 0x4b596f,
    border_strong: 0x7b8ca5,
    accent: 0xaca8ff,
    accent_soft: 0x302f56,
    accent_solid: 0x6155ec,
    accent_fg: 0xffffff,
    ok: 0x6cdbc0,
    err: 0xff98a5,
    warn: 0xf1c16d,
    ok_soft: 0x1b3935,
    err_soft: 0x402933,
    warn_soft: 0x3a3224,
    code_bg: 0x121b28,
};

impl Palette {
    pub fn light() -> Self {
        Self::from_base(&LIGHT, false)
    }

    pub fn dark() -> Self {
        Self::from_base(&DARK, true)
    }

    /// The palette of the theme `ui` is drawn in.
    pub fn of(ui: &egui::Ui) -> Self {
        if ui.visuals().dark_mode {
            Self::dark()
        } else {
            Self::light()
        }
    }

    fn from_base(b: &Base, dark: bool) -> Self {
        let raised = hex(b.bg_raised);
        Self {
            dark,
            bg: hex(b.bg),
            bg_panel: hex(b.bg_panel),
            bg_elev: hex(b.bg_elev),
            bg_input: hex(b.bg_input),
            bg_hover: hex(b.bg_hover),
            bg_sidebar: hex(b.bg_sidebar),
            bg_chat: hex(b.bg_chat),
            bg_raised: raised,
            fg: hex(b.fg),
            fg_muted: hex(b.fg_muted),
            border: hex(b.border),
            border_control: hex(b.border_control),
            border_strong: hex(b.border_strong),
            accent: hex(b.accent),
            accent_soft: hex(b.accent_soft),
            accent_solid: hex(b.accent_solid),
            accent_fg: hex(b.accent_fg),
            ok: hex(b.ok),
            err: hex(b.err),
            warn: hex(b.warn),
            ok_soft: hex(b.ok_soft),
            err_soft: hex(b.err_soft),
            warn_soft: hex(b.warn_soft),
            code_bg: hex(b.code_bg),
            track: hex(b.bg_elev),
            short_strong: hex(b.err),
            short_soft: mix_oklab(hex(b.err), raised, 0.45),
            long_strong: hex(b.ok),
            long_soft: mix_oklab(hex(b.ok), raised, 0.45),
            neutral_zone: mix_oklab(hex(b.border_strong), raised, 0.45),
        }
    }
}

// ---------------------------------------------------------------- oklab mix

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0).round() as u8
}

fn to_oklab(c: Color32) -> [f32; 3] {
    let (r, g, b) = (
        srgb_to_linear(c.r()),
        srgb_to_linear(c.g()),
        srgb_to_linear(c.b()),
    );
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> Color32 {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    Color32::from_rgb(
        linear_to_srgb(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_),
        linear_to_srgb(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_),
        linear_to_srgb(-0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    )
}

/// CSS `color-mix(in oklab, a <share>, b)`.
pub fn mix_oklab(a: Color32, b: Color32, share: f32) -> Color32 {
    let (x, y) = (to_oklab(a), to_oklab(b));
    from_oklab([
        x[0] * share + y[0] * (1.0 - share),
        x[1] * share + y[1] * (1.0 - share),
        x[2] * share + y[2] * (1.0 - share),
    ])
}

// ---------------------------------------------------------------- egui style

fn visuals(p: &Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let r = CornerRadius::same(radius::SM);
    v.panel_fill = p.bg_chat;
    v.window_fill = p.bg_raised;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(radius::MD);
    v.menu_corner_radius = r;
    v.faint_bg_color = p.bg_elev;
    v.extreme_bg_color = p.bg_input;
    v.text_edit_bg_color = Some(p.bg_input);
    v.code_bg_color = p.code_bg;
    v.hyperlink_color = p.accent;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.err;
    v.weak_text_color = Some(p.fg_muted);
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.slider_trailing_fill = true;
    let shadow_alpha = if p.dark { 70 } else { 28 };
    v.window_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    v.popup_shadow = v.window_shadow;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.bg_raised;
    w.noninteractive.weak_bg_fill = p.bg_raised;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.fg);
    w.noninteractive.corner_radius = r;
    // `button`: panel fill, control border. `bg_fill` is what inputs use
    // (checkbox box, slider rail): `input { background: var(--bg-input) }`.
    w.inactive.bg_fill = p.bg_input;
    w.inactive.weak_bg_fill = p.bg_panel;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border_control);
    w.inactive.fg_stroke = Stroke::new(1.0, p.fg);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = p.bg_hover;
    w.hovered.weak_bg_fill = p.bg_hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border_control);
    w.hovered.fg_stroke = Stroke::new(1.5, p.fg);
    w.hovered.corner_radius = r;
    w.active.bg_fill = p.accent_solid;
    w.active.weak_bg_fill = p.bg_hover;
    w.active.bg_stroke = Stroke::new(1.0, p.border_primary());
    w.active.fg_stroke = Stroke::new(1.5, p.fg);
    w.active.corner_radius = r;
    w.open.bg_fill = p.bg_hover;
    w.open.weak_bg_fill = p.bg_hover;
    w.open.bg_stroke = Stroke::new(1.0, p.border_control);
    w.open.fg_stroke = Stroke::new(1.0, p.fg);
    w.open.corner_radius = r;
    v
}

impl Palette {
    fn border_primary(&self) -> Color32 {
        if self.dark {
            hex(0x6669a6)
        } else {
            hex(0x8e8cd9)
        }
    }
}

fn style_common(style: &mut egui::Style) {
    use FontFamily::{Monospace, Proportional};
    style.text_styles = [
        (TextStyle::Small, FontId::new(text::SM, Proportional)),
        (TextStyle::Body, FontId::new(text::BASE, Proportional)),
        (TextStyle::Button, FontId::new(text::BASE, Proportional)),
        (TextStyle::Heading, FontId::new(text::HEADING, Proportional)),
        (TextStyle::Monospace, FontId::new(text::MD, Monospace)),
    ]
    .into();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(space::S4, space::S3);
    s.button_padding = egui::vec2(space::S5, 5.0);
    s.interact_size.y = 26.0;
    s.slider_width = 240.0;
}

/// Installs the Desk style for both themes and the theme preference.
/// Cheap (no I/O, no font loading), so it runs in the app creator.
pub fn apply(ctx: &egui::Context, pref: ThemePreference) {
    ctx.set_visuals_of(Theme::Light, visuals(&Palette::light()));
    ctx.set_visuals_of(Theme::Dark, visuals(&Palette::dark()));
    ctx.all_styles_mut(style_common);
    ctx.set_theme(pref);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_reads_css_order() {
        assert_eq!(hex(0x4338ca), Color32::from_rgb(0x43, 0x38, 0xca));
    }

    #[test]
    fn oklab_round_trips() {
        for c in [hex(0xb52d43), hex(0x6cdbc0), hex(0x202b3c), Color32::WHITE] {
            assert_eq!(mix_oklab(c, c, 0.45), c);
        }
    }

    #[test]
    fn soft_zones_sit_between_pole_and_card() {
        let p = Palette::light();
        // Mixed with white, the soft red is lighter than the strong red but still reddish.
        let (s, w) = (p.short_soft, p.short_strong);
        assert!(s.r() > w.r() && s.g() > w.g());
        assert!(s.r() > s.g());
        let d = Palette::dark();
        assert!(d.long_soft.g() < d.long_strong.g());
    }

    #[test]
    fn palettes_differ_by_theme() {
        assert!(Palette::dark().dark);
        assert!(!Palette::light().dark);
        assert_ne!(Palette::dark().bg, Palette::light().bg);
    }
}
