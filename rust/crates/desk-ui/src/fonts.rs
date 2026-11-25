//! Inter (Regular + SemiBold), subset to Latin, Latin-1, Cyrillic, general
//! punctuation and the few symbols the screen uses (✓ ✗ ⚠ ≈): ~150 KB
//! together, `assets/fonts/OFL.txt`. Inter is the face the TZ asks for;
//! egui's Ubuntu-Light stays behind it as the fallback, and the monospace face
//! is egui's Hack. egui 0.37 ships no emoji font by default, so the screen
//! uses none (the glyph test in `snapshots.rs` would catch one).

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular-subset.otf");
const SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold-subset.otf");

/// The family name of the semibold face (headings, numbers, buttons that are on).
pub const BOLD: &str = "bold";

/// The semibold family, for `RichText::family`.
pub fn bold() -> FontFamily {
    FontFamily::Name(BOLD.into())
}

/// The desk's font set. Pure data: parsing happens lazily inside egui.
pub fn definitions() -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    defs.font_data
        .insert("inter".into(), Arc::new(FontData::from_static(REGULAR)));
    defs.font_data.insert(
        "inter-semibold".into(),
        Arc::new(FontData::from_static(SEMIBOLD)),
    );
    let fallback = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut proportional = vec!["inter".to_owned()];
    proportional.extend(fallback.iter().cloned());
    let mut bold = vec!["inter-semibold".to_owned()];
    bold.extend(fallback);
    defs.families.insert(FontFamily::Proportional, proportional);
    defs.families.insert(bold_family(), bold);
    defs
}

fn bold_family() -> FontFamily {
    bold()
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_leads_both_families_with_egui_fallback() {
        let d = definitions();
        let p = &d.families[&FontFamily::Proportional];
        assert_eq!(p[0], "inter");
        assert!(p.len() > 1, "egui's fonts stay as fallback");
        assert_eq!(d.families[&bold()][0], "inter-semibold");
    }

    #[test]
    fn subsets_stay_small() {
        assert!(REGULAR.len() + SEMIBOLD.len() < 400 * 1024);
    }
}
