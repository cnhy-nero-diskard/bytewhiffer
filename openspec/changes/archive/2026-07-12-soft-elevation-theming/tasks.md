## 1. De-risk: blurred-shadow performance spike

- [x] 1.1 Add a minimal `epaint::Shadow` behind every treemap block in `draw_children` (flat fill unchanged otherwise), using placeholder offset/blur/spread values, gated behind nothing yet — just to measure cost.
  - Done as a dedicated headless bench (`bytewhiffer --debug-perf`, `app::run_perf_bench`) rather than mutating `draw_children`: it lays out synthetic trees shaped like the motivating screenshot and tessellates flat-baseline vs shadow+gradient shape sets directly via `epaint::Tessellator`, so cost is measured with no GUI/display and no throwaway edits to the real render path.
- [x] 1.2 Measure frame time with shadows on vs. today's baseline on a scan shaped like the motivating screenshot (a nested DLL-heavy directory, an installers directory, and a dense 20+ file mosaic) — reuse `--debug-screenshot-live` timing or add a temporary frame-time readout.
  - Measured CPU tessellation time (the stated risk is "tessellate more triangles per block", i.e. CPU cost; a few hundred extra triangles is negligible on the GPU), median of 200 passes, `TessellationOptions::default()` (parallel, as eframe uses). Numbers below.
- [x] 1.3 Decide and record: keep the blurred shadow as planned, or fall back to a hard-edged (blur=0) shadow / tighten the size floor — capture the outcome and numbers before proceeding to section 3.
  - **Decision: keep the blurred shadow as planned. No hard-edged fallback, no tighter floor.**
  - Numbers (release, GNU toolchain, 1280×760 layout, `MIN_CARD_SIDE = 22`):
    - Dense motivating scene — 315 blocks (41 cards, 274 flat): baseline 35 838 tris / 0.54 ms → elevated 18 244 tris / 0.34 ms = **0.51× tris, 0.63× time (elevation is *cheaper*)**. Today every block pays for a rounded fill + feathered rounded stroke; the size-floor flattens the 274 tiny blocks (square, no shadow) and only 41 large blocks carry the shadow/gradient.
    - Adversarial all-cards scene — 400 near-equal ~49 px blocks (0 flat): baseline 71 200 tris / 1.15 ms → elevated 98 400 tris / 1.63 ms = **1.38× tris, 1.42× time**. Absolute cost 1.63 ms median — ~15 ms of headroom under a 16.6 ms/60 fps budget, before eframe's parallel tessellation.
  - Conclusion: the size-based flat fallback is the correct primary lever; it makes the common dense case *faster* than today and keeps the worst case comfortably within frame budget. The blurred (not hard-edged) look is retained per the design's aesthetic decision.

## 2. Elevation primitives in `theme.rs`

- [x] 2.1 Add a gradient-stop color helper (lighten/darken a base color for the top/bottom mesh vertices). — `theme::gradient_stops(base) -> (top, bottom)` via `Color32::lerp_to_gamma` toward white/black; hue untouched.
- [x] 2.2 Replace/extend `depth_shift` with elevation constants (corner radius, shadow offset/blur/spread, gradient stop amounts), using the shadow parameters validated in section 1. — Added `CARD_CORNER_RADIUS`, `TRAY_CORNER_RADIUS`, `GRAD_LIGHTEN/DARKEN`, `SHADOW_*`, `TRAY_FILL`, `card_shadow()`, `tray_inset_shadow()`, `tray_header_color()`. `depth_shift` is *kept* as a secondary cue layered under elevation (spec asks for elevation "rather than a lightness shift alone", not its removal).
- [x] 2.3 Add the minimum-card-size constant (e.g. `MIN_CARD_SIDE`) alongside the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE` in `app.rs`. — Added `MIN_CARD_SIDE = 22.0` and `CARD_GAP = 1.5`.

## 3. Treemap block rendering (`draw_children`)

- [x] 3.1 Build a 4-vertex gradient mesh helper (top-lighter/bottom-darker `Mesh::colored_vertex` quad) and use it in place of `rect_filled` for card-eligible blocks. — Implemented as `gradient_mesh`: a center-fan over epaint's rounded-rect perimeter (`path::rounded_rectangle`) with per-vertex colour by height, so the gradient gets *real rounded corners* rather than a sharp quad. Used via `paint_card`.
- [x] 3.2 Paint the validated `Shadow` shape beneath each card-eligible block, before its fill. — `paint_card` adds `theme::card_shadow().as_shape(...)` before the gradient fill.
- [x] 3.3 Apply the size-floor fallback: blocks below `MIN_CARD_SIDE` keep today's flat-fill/hairline-border path unchanged. — Sub-threshold blocks use flat `rect_filled` (radius 0) + hairline stroke, and no gap; card-eligible blocks get a `CARD_GAP` inset so neighbours' shadows show.
- [x] 3.4 Render directories as a recessed tray (inset shadow + header strip carrying the name) drawn before recursing into children; children render as raised cards within the inset content region. — `draw_tray_shell`: dark `TRAY_FILL` well + header strip with the name + a short dark→transparent inset shadow under the header; children recurse into the well inset by `TRAY_PAD`.
- [x] 3.5 Use `--debug-screenshot-drill` to visually confirm nested directories don't stack trays into visual mud at typical on-screen depths. — Verified: `target › debug` renders `deps`/`build` as clean trays with card children; the size floor stops nesting before trays stack into mud.

## 4. Chrome (toolbar / breadcrumb)

- [x] 4.1 Apply matching gradient/shadow/radius styling to toolbar buttons and the path field. — Custom `chrome_button` (shadow + gradient + rounded, hover→accent, press→darker) for Pick folder / Scan / Cancel; the path field is a frameless `TextEdit` over a recessed (darkened) `paint_surface` card. `theme::apply` also rounds remaining stock widgets/windows.
- [x] 4.2 Apply matching styling to breadcrumb links and the current-crumb indicator. — Crumbs are `chrome_chip`s; the current level (and any hovered crumb) wears the accent, others the muted chrome base; the back arrow is a `chrome_button`.
- [x] 4.3 Apply the same size-floor fallback rule to chrome elements for consistency, even though they're unlikely to trigger it in practice. — `paint_surface` (used by both `chrome_button` and `chrome_chip`) elevates at ≥ `MIN_CARD_SIDE` and falls back to flat fill + hairline stroke below it, identical to the treemap floor.

## 5. Spec verification

- [x] 5.1 Manually verify each scenario in the updated `theming` spec via `--debug-screenshot`/`--debug-screenshot-drill`: nested depth reads as elevation, directories read as trays with floating children, chrome matches the treemap's language, dense small-file clusters stay flat. — Verified on a dense `target/` scan (final + drill modes): child cards (shadow/gradient/rounded) sit above parent trays; `debug`/`build`/`deps`/`release` render as header-stripped recessed wells; toolbar/breadcrumb use the same language; the sub-threshold mosaic (e.g. `.fingerprint`) stays flat. Threshold crossing is a discrete `card_eligible` branch (flat ↔ elevated, no intermediate state).
- [x] 5.2 Run `cargo test` and update any `theme.rs` unit tests affected by the constant/function changes from section 2. — All 24 tests pass (22 existing + 2 new). `depth_shift` was kept, so no existing test needed changing; added `gradient_stops_bracket_the_base_in_lightness` and `card_shadow_is_soft_not_hard_edged`.
- [x] 5.3 Capture before/after `--debug-screenshot` output against a scan shaped like the original motivating screenshot as a final visual sanity check. — Captured before (git-stashed flat baseline) vs after on the same dense `target/` scan; the contrast is unmistakable (hard-bordered flat rects + stock widgets → gradient/shadow cards, recessed directory trays, and matching elevated chrome).
