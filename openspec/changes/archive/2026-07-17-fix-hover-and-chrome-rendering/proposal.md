## Why

Real-use testing on a dense `C:\` scan surfaced three concrete rendering defects: the block hover tooltip wraps into an illegible single-character-per-line column near screen edges, that same tooltip visibly lags far behind a smoothly-moving cursor on dense directories, and toolbar buttons render a drop shadow that reads as a doubled, offset rectangle rather than a subtle lift. All three trace back to visual/render code that was tuned or written for one context (block-scale cards, sparse trees, screen-center placement) and never adapted to the contexts it also runs in (viewport edges, dense trees, chrome-scale elements) — worth fixing together since they share the same rendering code paths.

## What Changes

- Give the hover tooltip's path text an explicit wrap/width policy (or middle-elision, matching how Explorer shortens long paths) so it never degrades into a single-character-per-line column when the popup is repositioned near a viewport edge.
- Stop re-sorting and re-tessellating every card-eligible block from scratch on every frame regardless of whether the tree changed; gate that work on the existing `tree_rev` change counter so a frame driven purely by pointer movement doesn't pay full shadow/gradient tessellation cost for the entire visible subtree. Apply the already-documented (but never implemented) hard-edged-shadow fallback for dense views if caching alone doesn't close the gap, verified with the existing `--debug-perf` harness.
- Scale chrome shadow parameters (toolbar buttons, path field, breadcrumb chips) to their actual element size instead of reusing the same absolute-pixel shadow constants used for much larger treemap cards, fulfilling the theming spec's existing "scaled appropriately" clause for chrome elevation, which was never actually implemented.

## Capabilities

### New Capabilities
- `hover-tooltip`: the floating path/size tooltip that follows the pointer while hovering a treemap block — its text formatting (wrap/elision behavior for long paths) and its responsiveness to pointer movement regardless of how many blocks are currently visible.

### Modified Capabilities
- `theming`: tighten the existing "Chrome shares the same elevation language as the treemap" scenario, which already calls for chrome elevation to be "scaled appropriately" but leaves that untestable, into a concrete requirement that chrome shadow/gradient parameters scale down relative to the block-scale values rather than reusing them unscaled.

## Impact

- `src/app.rs`: `draw_children`, `paint_card`, `paint_surface`, `chrome_button`, and the `treemap_panel` tooltip block (`egui::Tooltip::always_open` call and its content).
- `src/theme.rs`: `card_shadow` and its constants (`SHADOW_OFFSET`/`SHADOW_BLUR`/`SHADOW_SPREAD`/`SHADOW_ALPHA`), which currently have no size-aware variant.
- No changes to `src/scanner/` or the pure geometry in `src/treemap.rs` — both stay free of rendering concerns per the existing module boundary.
- Verification: the existing `--debug-perf` hidden flag (tessellation benchmark) for the tessellation-cost side of the tooltip-lag fix, plus manual `--debug-screenshot*` checks and live interaction for the wrap and shadow-proportion fixes, since those are visual/perceptual and not amenable to a headless assertion.
