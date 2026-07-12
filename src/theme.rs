//! Color palette and deterministic color-from-extension logic.
//!
//! Direction (from the planning doc): dark charcoal-navy base, hash-derived
//! hue per file extension constrained to a fixed saturation/value band so
//! the palette reads as curated rather than random RGB noise, nesting depth
//! shown as a subtle lightness shift, and one reserved accent color that
//! only ever means "interactive": hover, the active breadcrumb, selection.

use eframe::egui::{self, epaint::Shadow, Color32};

/// App/treemap background. `#0d1117` — same family as GitHub dark.
pub const BG: Color32 = Color32::from_rgb(0x0d, 0x11, 0x17);
/// Slightly elevated panel background for toolbars and bars. `#131923`
pub const PANEL: Color32 = Color32::from_rgb(0x13, 0x19, 0x23);
/// Primary text. `#e6edf3`
pub const TEXT: Color32 = Color32::from_rgb(0xe6, 0xed, 0xf3);
/// Secondary/subdued text. `#8b949e`
pub const TEXT_SUBTLE: Color32 = Color32::from_rgb(0x8b, 0x94, 0x9e);
/// The one vivid accent — electric blue `#58a6ff`. Reserved for hover,
/// the active breadcrumb entry, and selection; nothing else.
pub const ACCENT: Color32 = Color32::from_rgb(0x58, 0xa6, 0xff);
/// Hairline borders between treemap blocks. `#010409`-ish, darker than BG.
pub const BLOCK_BORDER: Color32 = Color32::from_rgb(0x01, 0x04, 0x09);
/// Idle fill base for chrome surfaces (toolbar buttons, path field,
/// breadcrumb chips) — a muted slate that takes the same gradient/shadow
/// treatment as treemap blocks so the chrome reads as one system with the map.
pub const CHROME_BASE: Color32 = Color32::from_rgb(0x1b, 0x22, 0x2d);

/// Saturation/value band every extension-derived block color lives in.
/// Keeping these fixed (only hue varies) is what makes the palette cohere.
pub const BLOCK_SATURATION: f32 = 0.42;
pub const BLOCK_VALUE: f32 = 0.46;

/// Directories are deliberately *not* hue-coded: a muted slate so the
/// hue-coded files inside them carry the color signal.
const DIR_HUE: f32 = 0.60; // blue-slate
const DIR_SATURATION: f32 = 0.28;
// Kept below BLOCK_VALUE so directories still read as duller than files, but
// not so low that a small dir tile (bordered in near-black BLOCK_BORDER)
// blends into BG and reads as a hole in the mosaic.
const DIR_VALUE: f32 = 0.40;

/// Per-level lightness lift that communicates nesting depth. Subtle on
/// purpose; capped so deep trees don't wash out to white. Retained as a
/// *secondary* cue layered under the elevation treatment below — the spec
/// asks for depth via elevation "rather than a lightness shift alone", not
/// for the lightness shift to be removed.
const DEPTH_LIFT: f32 = 0.035;
const DEPTH_LIFT_MAX: f32 = 0.18;

// --- Soft-elevation treatment -------------------------------------------
// Blocks and chrome read as raised cards: a top-lighter/bottom-darker
// gradient fill, a soft drop shadow, and a modest corner radius. These
// values are implementation-time tuning (per the change's design doc), set
// against the running app via `--debug-screenshot*` and validated for
// tessellation cost by the `--debug-perf` spike.

/// Corner radius for raised cards and chrome elements.
pub const CARD_CORNER_RADIUS: f32 = 4.0;
/// Corner radius for a directory's recessed tray (slightly softer).
pub const TRAY_CORNER_RADIUS: f32 = 5.0;

/// How far the top of a card's gradient is lifted toward white, and its
/// bottom pushed toward black. Small enough to read as a sheen, not a
/// second color.
const GRAD_LIGHTEN: f32 = 0.13;
const GRAD_DARKEN: f32 = 0.16;

/// Soft drop shadow cast by a raised card. Blurred (not hard-edged) per the
/// design decision; kept small so the penumbra stays tight on small cards.
const SHADOW_OFFSET: [i8; 2] = [0, 2];
const SHADOW_BLUR: u8 = 7;
const SHADOW_SPREAD: u8 = 0;
const SHADOW_ALPHA: u8 = 110;

/// A directory renders as a shallow recessed tray. Its body is darker than
/// the surrounding surface so raised child cards (and their shadows) read as
/// floating above it.
pub const TRAY_FILL: Color32 = Color32::from_rgb(0x08, 0x0b, 0x10);
/// Colour of the short inner top shadow that sells the "recessed" look.
pub fn tray_inset_shadow() -> Color32 {
    Color32::from_black_alpha(90)
}

/// Top/bottom gradient stops for a card of the given base colour: lighter at
/// the top, darker at the bottom. Hue is untouched — this shades the fixed
/// per-extension colour, it does not pick a new one.
pub fn gradient_stops(base: Color32) -> (Color32, Color32) {
    let top = base.lerp_to_gamma(Color32::WHITE, GRAD_LIGHTEN);
    let bottom = base.lerp_to_gamma(Color32::BLACK, GRAD_DARKEN);
    (top, bottom)
}

/// The soft drop shadow cast by a raised card.
pub fn card_shadow() -> Shadow {
    Shadow {
        offset: SHADOW_OFFSET,
        blur: SHADOW_BLUR,
        spread: SHADOW_SPREAD,
        color: Color32::from_black_alpha(SHADOW_ALPHA),
    }
}

/// The header strip colour for a directory tray: its muted-slate base,
/// lifted by depth like any block, then brightened a touch so the name
/// strip reads as the tray's "label bar" above the darker body.
pub fn tray_header_color(name: &str, depth: usize) -> Color32 {
    depth_shift(base_block_color(name, true), depth).lerp_to_gamma(Color32::WHITE, 0.06)
}

/// Installs the dark theme on the egui context. Called once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.4);
    visuals.hyperlink_color = ACCENT;
    // Round any remaining stock widgets (tooltips, the error window, the
    // scan spinner's frame) to match the card corner radius, so nothing in
    // the app carries the old hard-cornered Win32 look.
    let radius = egui::CornerRadius::from(CARD_CORNER_RADIUS as u8);
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    visuals.window_corner_radius = egui::CornerRadius::from(TRAY_CORNER_RADIUS as u8);
    ctx.set_visuals(visuals);
}

/// Base block color for a treemap entry, before any depth shift: muted
/// slate for directories, hash-derived hue for files.
pub fn base_block_color(name: &str, is_dir: bool) -> Color32 {
    if is_dir {
        hsv(DIR_HUE, DIR_SATURATION, DIR_VALUE)
    } else {
        color_for_extension(extension_of(name))
    }
}

/// Deterministic color for a file extension: FNV-1a hash of the lowercased
/// extension picks the hue; saturation and value are fixed to the band.
/// The same extension always maps to the same color.
pub fn color_for_extension(ext: &str) -> Color32 {
    let hash = fnv1a(ext.to_ascii_lowercase().as_bytes());
    let hue = (hash % 360) as f32 / 360.0;
    hsv(hue, BLOCK_SATURATION, BLOCK_VALUE)
}

/// Lightens a base color by nesting depth so structure reads without
/// introducing new hues.
pub fn depth_shift(color: Color32, depth: usize) -> Color32 {
    let lift = (depth as f32 * DEPTH_LIFT).min(DEPTH_LIFT_MAX);
    let lerp = |c: u8| -> u8 { (c as f32 + (255.0 - c as f32) * lift) as u8 };
    Color32::from_rgb(lerp(color.r()), lerp(color.g()), lerp(color.b()))
}

/// Extension of a file name, or "" for extensionless files — which then
/// hash to their own stable fallback color like any other extension.
fn extension_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext,
        _ => "",
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    egui::ecolor::Hsva::new(h, s, v, 1.0).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::ecolor::Hsva;

    #[test]
    fn same_extension_same_color() {
        assert_eq!(color_for_extension("rs"), color_for_extension("rs"));
        assert_eq!(color_for_extension("RS"), color_for_extension("rs"));
        assert_eq!(
            base_block_color("a.txt", false),
            base_block_color("b.txt", false)
        );
    }

    #[test]
    fn different_extensions_differ_in_hue() {
        let a = color_for_extension("rs");
        let b = color_for_extension("txt");
        assert_ne!(a, b);
    }

    #[test]
    fn extension_colors_stay_inside_the_band() {
        for ext in ["rs", "txt", "png", "exe", "zip", "mp4", "dll", ""] {
            let hsva = Hsva::from(color_for_extension(ext));
            assert!(
                (hsva.s - BLOCK_SATURATION).abs() < 0.05,
                "{ext}: saturation {} escaped the band",
                hsva.s
            );
            assert!(
                (hsva.v - BLOCK_VALUE).abs() < 0.05,
                "{ext}: value {} escaped the band",
                hsva.v
            );
        }
    }

    #[test]
    fn depth_shift_lightens_monotonically_and_caps() {
        let base = base_block_color("dir", true);
        let d1 = depth_shift(base, 1);
        let d3 = depth_shift(base, 3);
        assert!(d1.r() >= base.r() && d1.g() >= base.g() && d1.b() >= base.b());
        assert!(d3.r() >= d1.r());
        // Deep nesting stops getting lighter at the cap.
        assert_eq!(depth_shift(base, 50), depth_shift(base, 100));
    }

    #[test]
    fn gradient_stops_bracket_the_base_in_lightness() {
        // The elevation treatment shades a block's fixed colour into a
        // top-lighter/bottom-darker pair; it must not invert or flatten.
        let lum = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
        for ext in ["rs", "png", "exe", "zip", ""] {
            let base = color_for_extension(ext);
            let (top, bottom) = gradient_stops(base);
            assert!(lum(top) > lum(base), "{ext}: top should be lighter than base");
            assert!(
                lum(bottom) < lum(base),
                "{ext}: bottom should be darker than base"
            );
        }
    }

    #[test]
    fn card_shadow_is_soft_not_hard_edged() {
        // The design chose a blurred penumbra over a hard-edged offset rect;
        // the perf spike confirmed the cost is acceptable. Lock that in.
        assert!(
            card_shadow().blur > 0,
            "elevation uses a soft blurred shadow, not a hard-edged one"
        );
    }

    #[test]
    fn extensionless_files_get_a_stable_fallback() {
        assert_eq!(
            base_block_color("Makefile", false),
            base_block_color("LICENSE", false)
        );
        // A leading dot alone is not an extension (dotfiles).
        assert_eq!(
            base_block_color(".gitignore", false),
            base_block_color("README", false)
        );
    }
}
