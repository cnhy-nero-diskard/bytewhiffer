//! Color palette and deterministic color-from-extension logic.
//!
//! Direction (from the planning doc): dark charcoal-navy base, hash-derived
//! hue per file extension constrained to a fixed saturation/value band so
//! the palette reads as curated rather than random RGB noise, nesting depth
//! shown as a subtle lightness shift, and one reserved accent color that
//! only ever means "interactive": hover, the active breadcrumb, selection.

use eframe::egui::{self, Color32};

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

/// Saturation/value band every extension-derived block color lives in.
/// Keeping these fixed (only hue varies) is what makes the palette cohere.
pub const BLOCK_SATURATION: f32 = 0.42;
pub const BLOCK_VALUE: f32 = 0.46;

/// Directories are deliberately *not* hue-coded: a muted slate so the
/// hue-coded files inside them carry the color signal.
const DIR_HUE: f32 = 0.60; // blue-slate
const DIR_SATURATION: f32 = 0.25;
const DIR_VALUE: f32 = 0.30;

/// Per-level lightness lift that communicates nesting depth. Subtle on
/// purpose; capped so deep trees don't wash out to white.
const DEPTH_LIFT: f32 = 0.035;
const DEPTH_LIFT_MAX: f32 = 0.18;

/// Installs the dark theme on the egui context. Called once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.linear_multiply(0.4);
    visuals.hyperlink_color = ACCENT;
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
