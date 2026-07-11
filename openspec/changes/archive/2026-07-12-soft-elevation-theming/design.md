## Context

The current renderer (`theme.rs` + `app.rs::draw_children`) paints every treemap block as a flat-filled rectangle with a 1px near-black seam (`theme::BLOCK_BORDER`) and communicates nesting depth via `theme::depth_shift`, a lightness lift of 3.5% per level capped at 18% — too subtle to read past one or two levels. The toolbar/breadcrumb chrome (`app.rs::toolbar`/`breadcrumb`) is unstyled `egui::Visuals::dark()`. The `theming` spec's existing "Depth communicated via elevation shift" requirement already asks for elevation; this design fulfills that literally instead of via a lightness-only proxy.

This surfaced from an `/opsx:explore` session that built a live HTML comparison (four candidate treatments, identical geometry, reconstructed from the user's own dense-Downloads-folder screenshot) — "soft elevation" (gradient + shadow + rounded directory trays) was the chosen direction, with two conditions attached: it applies uniformly across blocks *and* chrome, and it must degrade gracefully for undersized elements rather than applying unconditionally.

Verified against the vendored `epaint 0.35.0` source (not assumed):
- `epaint::Shadow { offset: [i8;2], blur: u8, spread: u8, color: Color32 }` is native and already used internally for window/popup shadows. `Shadow::as_shape(rect, corner_radius)` returns a paintable `RectShape` — no custom tessellation needed.
- `RectShape::corner_radius` (`CornerRadius`) is already used today (`draw_children` passes `2.0`); a larger radius is a constant change, not new plumbing.
- There is **no** built-in gradient-fill rectangle. A top-lighter/bottom-darker fill requires a hand-built 4-vertex `epaint::Mesh` using `Mesh::colored_vertex` per corner, relying on the renderer's per-vertex color interpolation.

## Goals / Non-Goals

**Goals:**
- Give treemap blocks and app chrome a consistent raised-card look: gradient fill, drop shadow, rounded corners.
- Make directories read as recessed containers (inset shadow + header strip) with their children floating above them, so elevation encodes container-vs-content structure.
- Preserve legibility and frame time on dense scans by falling back to flat fill below a size threshold.
- Leave the color-selection algorithm (hue-from-extension hash, fixed saturation/value band, single reserved accent) untouched.

**Non-Goals:**
- Changing `BLOCK_SATURATION`/`BLOCK_VALUE` or any hue-selection logic (a "raise saturation for vibrancy" direction was considered and rejected separately).
- A full custom `egui::Style`/widget-visuals rewrite beyond the toolbar/breadcrumb chrome named here.
- Building the NTFS MFT engine or any non-theming feature.

## Decisions

**Shadow: use `epaint::Shadow` with blur, not a hard-edged offset rect.** A blurred shadow costs more to tessellate per block than a flat rect or a hard-edged (blur=0) shadow, but a hard-edged "neo-brutalist" shadow was explicitly considered and rejected in favor of the softer, blurred look during exploration. This makes the perf spike below non-optional rather than a nice-to-have.

**Gradient: hand-built 2-stop mesh, not a texture or shader.** A 4-vertex quad (`Mesh::colored_vertex`, top two vertices lightened via `Color32`/HSV lerp, bottom two darkened) is cheap, keeps the dependency footprint at zero, and matches how egui itself expects custom shading to be done — there is no simpler native primitive to defer to.

**Directories are recessed, children float.** A directory's own fill sits *behind* an inset shadow (reads as a shallow tray) with a header strip carrying its name; its children are drawn as normal raised cards on top. This was chosen over uniform elevation (every block raised equally) specifically because uniform elevation was judged to communicate nothing — if everything floats, nothing does. Concretely: `draw_children` renders the directory's tray + header first, then recurses into children within an inset content region, mirroring the existing `DIR_LABEL_H` reserved-strip pattern already in the code, just with real depth instead of a text-only label.

**Size-based flat fallback, modeled on existing constants.** A new threshold (name TBD at implementation time, e.g. `MIN_CARD_SIDE`) sits alongside `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` in `app.rs`: below it, a block/chrome element renders with flat fill, no shadow, no gradient, no radius, and no inter-block gap — identical to today's rendering. This directly targets the dense small-file mosaic that motivated this change: without a floor, hundreds of sub-20px blocks would each carry a blurred shadow and rounded corners, reading as noise rather than polish, and multiplying tessellation cost exactly where block count is highest.

**Chrome gets the same treatment, not a separate visual language.** Toolbar buttons, the path field, and breadcrumb links adopt the same gradient/shadow/radius tokens as blocks (scaled appropriately) so the app reads as one system. Chrome elements are unlikely to ever hit the size floor in practice, but the same fallback rule applies to them for consistency rather than as a special case.

## Risks / Trade-offs

- **[Risk] Blurred shadows may materially increase per-frame tessellation cost on dense scans (hundreds of simultaneously visible blocks), directly undercutting Bytewhiffer's speed pitch vs. SpaceSniffer** → **Mitigation**: a dedicated perf-spike task, before this is considered done, measuring frame time on a scan shaped like the motivating screenshot (a nested dir with many small DLLs, an installers directory, and a dense ~20+ file mosaic) with shadows enabled vs. today's flat baseline. The size-based fallback is the primary lever if the spike shows a problem; a secondary lever (dropping blur in favor of a hard-edged shadow) is documented as a fallback plan, not implemented speculatively.
- **[Risk] Nested directories several levels deep could stack inset-shadow trays inside inset-shadow trays, reading as muddy** → **Mitigation**: the same size floor that flattens tiny leaf blocks also flattens tiny nested directories in practice, since `MIN_NEST_AREA`/`MIN_NEST_SIDE` already stop nesting before rects get pathologically small; no separate depth cap is expected to be needed, but this should be visually checked during implementation (e.g. via the existing `--debug-screenshot-drill` mode) rather than assumed.
- **[Trade-off] This touches render code in `draw_children`, not just constants in `theme.rs`**, and is a larger diff than a typical theming tweak — accepted, since the exploration concluded a lightness-only shift can't be fixed without touching how blocks are painted.

## Open Questions

- Exact `Shadow` offset/blur/spread values and the size-floor threshold in pixels are implementation-time tuning, not architectural decisions — expect these to be adjusted against the real running app (via `--debug-screenshot*`) rather than fixed here.
- Whether the hard-edged-shadow fallback plan (if the perf spike is unfavorable) should be a build-time choice, a runtime toggle, or simply a smaller size-floor is left for the spike's outcome to decide.
