## 1. Resolve open design questions

- [ ] 1.1 Decide the abstraction mechanism (depth cutoff vs. size threshold vs. top-K+aggregate-remainder) based on which scenario is actually motivating this — a deeply nested tree, or a single flat directory with hundreds of same-level files
- [ ] 1.2 Decide whether the render posture is a manual toggle, auto-triggered by density (like `render_dense`), or both
- [ ] 1.3 Decide whether the posture control replaces or layers on top of the existing `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` gate
- [ ] 1.4 Decide the preview's visual treatment (inset within the block vs. floating overlay that can exceed block bounds)
- [ ] 1.5 Update `design.md` and the specs in this change to reflect the resolved decisions before continuing

## 2. Render posture state and control

- [ ] 2.1 Add render posture state to `BytewhifferApp` (detail/abstract, or a continuous slider — per 1.2/1.3) alongside the existing `render_dense`/`density_key` fields
- [ ] 2.2 Add the UI control for setting the posture (chrome toggle or slider) per the decision in 1.2
- [ ] 2.3 Wire the chosen abstraction mechanism (1.1) into `draw_children`'s collapse/recurse decision, composed with the existing pixel-size gate per the decision in 1.3

## 3. Hover preview

- [ ] 3.1 Add a cached "currently previewed" layout keyed on `(hovered path, tree_rev)`, mirroring the `refresh_density`/`refresh_insights` caching pattern, so preview layout isn't recomputed every frame
- [ ] 3.2 Render the preview per the visual treatment decided in 1.4, using the existing `hits`/`hovered_path` plumbing from `fix-hover-and-chrome-rendering` to detect which collapsed block is hovered
- [ ] 3.3 Ensure the preview never mutates `self.focus` or the breadcrumb trail, and is discarded on pointer-out
- [ ] 3.4 Verify click-to-drill on a collapsed block still behaves exactly as it does today, regardless of an active preview

## 4. Verification

- [ ] 4.1 Add/extend unit tests for the abstraction mechanism's block-count behavior (detail vs. abstract posture on the same tree), following the existing scanner/treemap unit-test pattern that needs no display
- [ ] 4.2 Manually verify hover-preview and click-to-drill interaction live (per `run` skill / `--debug-screenshot*` flags), since preview visuals are not amenable to a headless assertion
- [ ] 4.3 Re-run `--debug-perf` if the abstraction mechanism changes how often/what `draw_children` tessellates, to confirm no regression versus the `fix-hover-and-chrome-rendering` baseline
