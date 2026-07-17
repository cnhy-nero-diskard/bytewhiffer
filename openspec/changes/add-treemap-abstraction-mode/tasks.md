## 1. Resolve open design questions

- [x] 1.1 Decide the abstraction mechanism — a manual slider scaling the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE` gate; top-K+aggregate-remainder deferred to a future change
- [x] 1.2 Decide whether the render posture is a manual toggle, auto-triggered by density (like `render_dense`), or both — manual only, no auto-trigger
- [x] 1.3 Decide whether the posture control replaces or layers on top of the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` gate — layers on top; the slider scales the existing constants directly
- [x] 1.4 Decide the preview's visual treatment — inset within the collapsed block's own rect, never a floating overlay
- [x] 1.5 Update `design.md` and the specs in this change to reflect the resolved decisions before continuing — design.md updated; delta specs already sit at the requirement level and need no changes for these implementation-level decisions

## 2. Render posture state and control

- [ ] 2.1 Add a render posture slider field to `BytewhifferApp` (scales `MIN_NEST_AREA`/`MIN_NEST_SIDE`) alongside the existing `render_dense`/`density_key` fields
- [ ] 2.2 Add a chrome slider control for setting the posture
- [ ] 2.3 Wire the slider's scale factor into `draw_children`'s collapse/recurse decision, multiplying the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE` pixel-size gate

## 3. Hover preview

- [ ] 3.1 Add a cached "currently previewed" layout keyed on `(hovered path, tree_rev)`, mirroring the `refresh_density`/`refresh_insights` caching pattern, so preview layout isn't recomputed every frame
- [ ] 3.2 Render the preview per the visual treatment decided in 1.4, using the existing `hits`/`hovered_path` plumbing from `fix-hover-and-chrome-rendering` to detect which collapsed block is hovered
- [ ] 3.3 Ensure the preview never mutates `self.focus` or the breadcrumb trail, and is discarded on pointer-out
- [ ] 3.4 Verify click-to-drill on a collapsed block still behaves exactly as it does today, regardless of an active preview

## 4. Verification

- [ ] 4.1 Add/extend unit tests for the abstraction mechanism's block-count behavior (detail vs. abstract posture on the same tree), following the existing scanner/treemap unit-test pattern that needs no display
- [ ] 4.2 Manually verify hover-preview and click-to-drill interaction live (per `run` skill / `--debug-screenshot*` flags), since preview visuals are not amenable to a headless assertion
- [ ] 4.3 Re-run `--debug-perf` if the abstraction mechanism changes how often/what `draw_children` tessellates, to confirm no regression versus the `fix-hover-and-chrome-rendering` baseline
