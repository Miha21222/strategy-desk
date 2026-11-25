//! Chart screenshot helpers: `legendColor`, `mainShot`, `shotTabLabel`,
//! `shotsErrorText` of `src/shared/macro.ts`, and the pure half of
//! `readShot` (macro-state.ts) as [`shot_ref`].

use crate::js;
use crate::names::is_shot_path;
use crate::vault::{MacroShot, ShotLegend, ShotRef};

/// `SHOT_KIND_COLORS`: line colour per kind when the file's own colour is
/// missing or not a plain hex.
pub fn kind_color(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "manip" => "#d32f2f",
        "target" => "#1e8e3e",
        "level" => "#37474f",
        "open" => "#9e9e9e",
        "zone" => "#5c6bc0",
        "fact" => "#ef6c00",
        _ => return None,
    })
}

/// `legendColor`: `#rgb`, `#rrggbb` or `#rrggbbaa` from the file, else the
/// kind's default (else the `level` one).
pub fn legend_color(item: &ShotLegend) -> String {
    let c = js::trim(&item.color);
    let hex = c
        .strip_prefix('#')
        .filter(|h| matches!(h.len(), 3 | 6 | 8) && h.bytes().all(|b| b.is_ascii_hexdigit()));
    if hex.is_some() {
        return c.to_owned();
    }
    kind_color(&item.kind).unwrap_or("#37474f").to_owned()
}

/// `mainShot`: the picture for the main view — the M15 one when there is one, else the first.
pub fn main_shot(shots: &[MacroShot]) -> Option<&MacroShot> {
    shots
        .iter()
        .find(|s| s.name.to_lowercase() == "m15" || s.tf.to_uppercase() == "M15")
        .or_else(|| shots.first())
}

/// `shotTabLabel`: «M15 · ночь и уровни у открытия» from tf «M15» and title
/// «M15 — ночь и уровни у открытия».
pub fn shot_tab_label(tf: &str, title: &str, name: &str) -> String {
    let tf = match js::trim(tf) {
        "" => js::trim(name).to_uppercase(),
        t => t.to_owned(),
    };
    let mut rest = js::trim(title).to_owned();
    if !tf.is_empty() && rest.to_uppercase().starts_with(&tf.to_uppercase()) {
        let cut = js::slice16_from(&rest, js::utf16_len(&tf));
        rest = cut
            .trim_start_matches(|c: char| {
                js::is_space(c) || matches!(c, '—' | '–' | '-' | ':' | '·')
            })
            .to_owned();
    }
    match (rest.is_empty(), tf.is_empty()) {
        (false, false) => format!("{tf} · {rest}"),
        (false, true) => rest,
        (true, _) => tf,
    }
}

/// What `shotsErrorText` knows about a missing picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotsErrorKind {
    /// A dry run: the screen words this itself.
    DryRun,
    /// No error at all.
    None,
}

/// `shotsErrorText`: why there are no pictures, ready for «Снимков графика
/// нет: …». `known` is `Some` for a reason the screen words itself; anything
/// else is shown as written, on one line, cut to `max` characters with «…».
pub fn shots_error_text(error: Option<&str>, max: usize) -> (Option<ShotsErrorKind>, String) {
    let e = js::collapse_spaces(error.unwrap_or(""));
    let e = js::trim(&e);
    if e.is_empty() {
        return (Some(ShotsErrorKind::None), String::new());
    }
    if is_dry_run(e) {
        return (Some(ShotsErrorKind::DryRun), String::new());
    }
    if js::utf16_len(e) > max {
        let cut = js::trim_end(js::slice16(e, max.saturating_sub(1)));
        return (None, format!("{cut}…"));
    }
    (None, e.to_owned())
}

/// The default `max` of `shotsErrorText`.
pub const SHOTS_ERROR_MAX: usize = 90;

/// `/^dry[-_ ]?run\b/i`.
fn is_dry_run(e: &str) -> bool {
    let b = e.as_bytes();
    if b.len() < 6 || !b[..3].eq_ignore_ascii_case(b"dry") {
        return false;
    }
    let mut i = 3;
    if matches!(b[i], b'-' | b'_' | b' ') {
        i += 1;
    }
    if b.len() < i + 3 || !b[i..i + 3].eq_ignore_ascii_case(b"run") {
        return false;
    }
    // \b: the next character is not an ASCII word character (or there is none).
    b.get(i + 3)
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
}

/// A screenshot is a few hundred KB; anything far bigger is not one of ours.
pub const SHOT_MAX_BYTES: u64 = 12 * 1024 * 1024;
const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// The pure half of `readShot`: given a vault-relative path and the first
/// bytes of the file plus its size, is it a picture the UI may show? Where
/// the TS answered with a base64 `data:` URL, this answers with a
/// [`ShotRef`] (path + size); the UI decodes the file itself. The I/O half
/// (realpath inside `indices-kb/shots/`, `stat`, reading) is desk-app's.
pub fn shot_ref(rel: &str, head: &[u8], size: u64) -> Option<ShotRef> {
    if !is_shot_path(rel) || size > SHOT_MAX_BYTES || size < PNG_MAGIC.len() as u64 {
        return None;
    }
    if head.get(..PNG_MAGIC.len()) != Some(&PNG_MAGIC[..]) {
        return None;
    }
    Some(ShotRef {
        path: rel.replace('\\', "/"),
        bytes: size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legend(color: &str, kind: &str) -> ShotLegend {
        ShotLegend {
            key: String::new(),
            label: String::new(),
            kind: kind.into(),
            color: color.into(),
            style: None,
        }
    }

    fn shot(name: &str, tf: &str) -> MacroShot {
        MacroShot {
            name: name.into(),
            tf: tf.into(),
            title: String::new(),
            path: String::new(),
            verify_ok: None,
            legend: vec![],
        }
    }

    #[test]
    fn keeps_only_plain_hex_colours_else_the_kind_default() {
        assert_eq!(legend_color(&legend("#1e8e3e", "target")), "#1e8e3e");
        assert_eq!(
            legend_color(&legend("red; background:url(x)", "manip")),
            "#d32f2f"
        );
        assert_eq!(legend_color(&legend("", "weird")), "#37474f");
        assert_eq!(legend_color(&legend(" #ABC ", "x")), "#ABC");
        assert_eq!(legend_color(&legend("#abcd", "fact")), "#ef6c00");
    }

    #[test]
    fn puts_m15_in_the_main_view_whatever_the_order() {
        let s = [shot("h1", "H1"), shot("m15", "M15")];
        assert_eq!(main_shot(&s).unwrap().name, "m15");
        assert_eq!(main_shot(&s[..1]).unwrap().name, "h1");
        assert_eq!(main_shot(&[]), None);
        assert_eq!(
            main_shot(&[shot("x", "H1"), shot("y", "m15")])
                .unwrap()
                .name,
            "y"
        );
    }

    #[test]
    fn names_tabs_from_the_timeframe_and_the_title() {
        assert_eq!(
            shot_tab_label("M15", "M15 — ночь и уровни у открытия", "m15"),
            "M15 · ночь и уровни у открытия"
        );
        assert_eq!(shot_tab_label("H1", "зоны дня", "h1"), "H1 · зоны дня");
        assert_eq!(shot_tab_label("", "", "h4"), "H4");
        assert_eq!(
            shot_tab_label("", "просто заголовок", ""),
            "просто заголовок"
        );
        assert_eq!(shot_tab_label("H1", "h1: зоны", ""), "H1 · зоны");
        assert_eq!(shot_tab_label("H1", "H1", ""), "H1");
    }

    #[test]
    fn words_the_reason_in_plain_language() {
        assert_eq!(
            shots_error_text(Some("dry-run"), 90),
            (Some(ShotsErrorKind::DryRun), String::new())
        );
        assert_eq!(
            shots_error_text(Some("DRY RUN: no TV"), 90).0,
            Some(ShotsErrorKind::DryRun)
        );
        assert_eq!(shots_error_text(Some("dryrunner"), 90).0, None);
        assert_eq!(shots_error_text(None, 90).0, Some(ShotsErrorKind::None));
        assert_eq!(
            shots_error_text(Some("TradingView не запущен"), 90),
            (None, "TradingView не запущен".into())
        );
        assert_eq!(shots_error_text(Some("  два\n  слова "), 90).1, "два слова");
        let long = shots_error_text(
            Some(&format!("нет шкалы с планом: {}", "x".repeat(200))),
            90,
        )
        .1;
        assert!(js::utf16_len(&long) <= 90);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn gives_out_only_pngs_inside_indices_kb_shots() {
        let png = [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, b'r', b'e', b's', b't',
        ];
        let rel = "indices-kb/shots/2026-10-08/macro_eu/m15.png";
        assert_eq!(
            shot_ref(rel, &png, 12),
            Some(ShotRef {
                path: rel.into(),
                bytes: 12
            })
        );
        assert_eq!(shot_ref(rel, b"not a png at all", 16), None);
        assert_eq!(shot_ref("indices-kb/state/secret.png", &png, 12), None);
        assert_eq!(
            shot_ref("indices-kb/shots/../state/secret.png", &png, 12),
            None
        );
        assert_eq!(shot_ref(rel, &png, SHOT_MAX_BYTES + 1), None);
        assert_eq!(shot_ref(rel, &png[..4], 4), None);
    }
}
