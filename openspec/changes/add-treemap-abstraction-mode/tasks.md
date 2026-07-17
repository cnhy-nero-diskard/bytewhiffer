## 1. Resolve open design questions

- [x] 1.1 Decide the abstraction mechanism — a manual slider driving a combined depth-cap + size-scale gate (revised from a pure size-threshold scale after real-use testing showed that never reached a "top-level only" view); top-K+aggregate-remainder deferred to a future change
- [x] 1.2 Decide whether the render posture is a manual toggle, auto-triggered by density (like `render_dense`), or both — manual only, no auto-trigger
- [x] 1.3 Decide whether the posture control replaces or layers on top of the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` gate — layers on top; the slider tightens the existing `MAX_DEPTH` cap and `MIN_NEST_*` thresholds
- [x] 1.4 Decide the preview's visual treatment — inset within the collapsed block's own rect, never a floating overlay
- [x] 1.5 Update `design.md` and the specs in this change to reflect the resolved decisions before continuing — design.md updated; delta specs already sit at the requirement level and need no changes for these implementation-level decisions

## 2. Render posture state and control

- [x] 2.1 Add a render posture slider field to `BytewhifferApp` (`abstraction: f32`, 0.0 = detail via `derive(Default)`) plus a `nest_gate()` helper returning `NestGate { max_depth, min_side, min_area }` and an `ABSTRACTION_SIDE_GAIN` constant, alongside the existing `render_dense`/`density_key` fields
- [x] 2.2 Add a chrome slider control (Detail ↔ Abstract) to `toolbar`, after the Insights button
- [x] 2.3 Wire the resolved `NestGate` into `draw_children` (new `gate` param) — the `would_nest` check uses `gate.max_depth`/`gate.min_side`/`gate.min_area`; threaded through both call sites (`treemap_panel` + the recursive call)

## 3. Hover preview

- [x] 3.1 Add a cached preview overlay (`PreviewOverlay`/`self.preview`) keyed on `(path, tree_rev, block rect)`, mirroring the `refresh_density`/`refresh_insights` caching pattern — pre-tessellated `egui::Shape`s rebuilt only when the key changes, re-painted every frame via `painter.extend`
- [x] 3.2 Render the preview inset within the collapsed block (per 1.4) via `build_preview_shapes`, gated on a new `HitRect.collapsed` flag + `self.abstraction > 0.0`, using the existing `hits`/`hovered` plumbing to find the hovered block and `Node::find(&hit.trail)` to resolve its subtree
- [x] 3.3 Preview is presentation-only — the paint path never touches `self.focus`/breadcrumb; cleared on pointer-out (the `else` arm) and on any non-eligible hover
- [x] 3.4 Click-to-drill left byte-for-byte unchanged (the `response.clicked() && hit.is_dir` arm), so a collapsed block still drills regardless of an active preview — code-level guarantee; live confirmation is 4.2

## 4. Verification

- [ ] 4.1 Add/extend unit tests for the abstraction mechanism's block-count behavior (detail vs. abstract posture on the same tree), following the existing scanner/treemap unit-test pattern that needs no display
- [ ] 4.2 Manually verify hover-preview and click-to-drill interaction live (per `run` skill / `--debug-screenshot*` flags), since preview visuals are not amenable to a headless assertion
- [ ] 4.3 Re-run `--debug-perf` if the abstraction mechanism changes how often/what `draw_children` tessellates, to confirm no regression versus the `fix-hover-and-chrome-rendering` baseline
