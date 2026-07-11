## Why

Bytewhiffer's current treemap and chrome read as flat: blocks are pure flat-fill rectangles with a 1px near-black seam, and the per-level depth cue (`theme::depth_shift`) lifts lightness by only 3.5% per level, capped at 18% — invisible past one or two nesting levels in practice. The `theming` spec's own "Depth communicated via elevation shift" requirement asks for depth to read as elevation; the current implementation satisfies its letter (a lightness shift exists) but not its spirit (nothing actually looks raised or layered). This was always meant to be a placeholder — the MVP's own design doc sequenced "theming polish" as deliberately last — and it's now visibly the weakest part of the app compared to the reference points (Rerun, Linear, GitHub dark) the original design direction named.

## What Changes

- Treemap blocks gain real elevation: a native `epaint::Shadow` drop shadow, a hand-built two-stop gradient fill (lighter top, darker bottom), and a larger corner radius, replacing the flat-fill + hairline-border look.
- Directories render as a recessed tray (inset shadow) with a real header strip carrying their name; their children float above that tray as raised cards — elevation communicates container-vs-content structure, not uniform decoration.
- The toolbar and breadcrumb chrome adopt the same elevation language (shadowed/gradient buttons and bars) so the whole app reads as one consistent visual system, not just the map.
- **New exception**: below a minimum on-screen size, blocks and chrome elements fall back to flat fill — no shadow, gradient, radius, or gap — so dense clusters of tiny blocks (common in real scans) don't turn to visual mush. This threshold follows the same pattern as the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE` constants in `app.rs`.
- Deterministic per-extension hue selection, the fixed saturation/value band, and the single reserved accent color are **unchanged** — this change is about shading/elevation mechanism only, not the color-selection algorithm. (A separate "raise the saturation band for more vibrancy" direction was explored and explicitly rejected in favor of this one.)

## Capabilities

### New Capabilities
(none)

### Modified Capabilities
- `theming`: the "Depth communicated via elevation shift" requirement is rewritten so its scenario demands actual shadow/gradient elevation with a directory-tray/floating-content hierarchy, not just a lightness shift. A new requirement is added for the minimum-size flat-fallback exception. The dark-base-theme, deterministic-color-from-extension, and reserved-accent-color requirements are unaffected.

## Impact

- `src/theme.rs`: depth-shift constants and function are replaced/extended with elevation-styled fill helpers (shadow params, gradient stops, the size-fallback threshold).
- `src/app.rs`: `draw_children` gains shadow/gradient/radius rendering and the directory-tray/header-strip layout; `toolbar()`/`breadcrumb()` adopt matching chrome styling; a new `MIN_CARD_SIDE`-style constant joins `MIN_NEST_AREA`/`MIN_NEST_SIDE`.
- `openspec/specs/theming/spec.md`: requirement rewrite plus one new requirement, via this change's delta spec.
- Performance: blurred shadows tessellate more triangles per block than a flat rect; a de-risking spike (frame-time measurement on a dense scan) is required before this is considered done, per this repo's existing convention of spiking risky/novel rendering work early.
