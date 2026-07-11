## 1. De-risk: blurred-shadow performance spike

- [ ] 1.1 Add a minimal `epaint::Shadow` behind every treemap block in `draw_children` (flat fill unchanged otherwise), using placeholder offset/blur/spread values, gated behind nothing yet — just to measure cost.
- [ ] 1.2 Measure frame time with shadows on vs. today's baseline on a scan shaped like the motivating screenshot (a nested DLL-heavy directory, an installers directory, and a dense 20+ file mosaic) — reuse `--debug-screenshot-live` timing or add a temporary frame-time readout.
- [ ] 1.3 Decide and record: keep the blurred shadow as planned, or fall back to a hard-edged (blur=0) shadow / tighten the size floor — capture the outcome and numbers before proceeding to section 3.

## 2. Elevation primitives in `theme.rs`

- [ ] 2.1 Add a gradient-stop color helper (lighten/darken a base color for the top/bottom mesh vertices).
- [ ] 2.2 Replace/extend `depth_shift` with elevation constants (corner radius, shadow offset/blur/spread, gradient stop amounts), using the shadow parameters validated in section 1.
- [ ] 2.3 Add the minimum-card-size constant (e.g. `MIN_CARD_SIDE`) alongside the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE` in `app.rs`.

## 3. Treemap block rendering (`draw_children`)

- [ ] 3.1 Build a 4-vertex gradient mesh helper (top-lighter/bottom-darker `Mesh::colored_vertex` quad) and use it in place of `rect_filled` for card-eligible blocks.
- [ ] 3.2 Paint the validated `Shadow` shape beneath each card-eligible block, before its fill.
- [ ] 3.3 Apply the size-floor fallback: blocks below `MIN_CARD_SIDE` keep today's flat-fill/hairline-border path unchanged.
- [ ] 3.4 Render directories as a recessed tray (inset shadow + header strip carrying the name) drawn before recursing into children; children render as raised cards within the inset content region.
- [ ] 3.5 Use `--debug-screenshot-drill` to visually confirm nested directories don't stack trays into visual mud at typical on-screen depths.

## 4. Chrome (toolbar / breadcrumb)

- [ ] 4.1 Apply matching gradient/shadow/radius styling to toolbar buttons and the path field.
- [ ] 4.2 Apply matching styling to breadcrumb links and the current-crumb indicator.
- [ ] 4.3 Apply the same size-floor fallback rule to chrome elements for consistency, even though they're unlikely to trigger it in practice.

## 5. Spec verification

- [ ] 5.1 Manually verify each scenario in the updated `theming` spec via `--debug-screenshot`/`--debug-screenshot-drill`: nested depth reads as elevation, directories read as trays with floating children, chrome matches the treemap's language, dense small-file clusters stay flat.
- [ ] 5.2 Run `cargo test` and update any `theme.rs` unit tests affected by the constant/function changes from section 2.
- [ ] 5.3 Capture before/after `--debug-screenshot` output against a scan shaped like the original motivating screenshot as a final visual sanity check.
