## 1. Tooltip text truncation

- [ ] 1.1 Add a middle-ellipsis truncation helper that bounds a joined trail string to a fixed character budget, eliding the middle rather than the end (e.g. `SteamLibrary/steamapps/…/pakchunk70-WindowsNoEditor.pak`)
- [ ] 1.2 Apply the helper to the trail string in `treemap_panel`'s tooltip content before rendering, and render it as a single non-wrapping line
- [ ] 1.3 Verify via `--debug-screenshot` on a deeply-nested path hovered near a viewport edge that the tooltip renders as one legible line instead of wrapping into a narrow column

## 2. Tooltip responsiveness on dense scans

- [ ] 2.1 Add a cached "entries in play under the current focus" count, recomputed only when the focus path or `tree_rev` changes — not every frame
- [ ] 2.2 Pick a density threshold, and decide (fully flat vs. `blur: 0` hard-edged shadow) the fallback tier used above it, by visual comparison against a dense scene
- [ ] 2.3 Wire the threshold check into `draw_children`'s card-eligible branch so the whole frame renders with the cheaper tier once the cached count crosses the threshold, alongside the existing `MIN_CARD_SIDE` per-block gate
- [ ] 2.4 Re-run `--debug-perf` against a scene shaped like a dense `C:\` scan (extending the existing synthetic bench scenes if needed) to confirm the fallback measurably cuts per-frame tessellation cost above threshold
- [ ] 2.5 Manually verify on a real dense `C:\` scan that the tooltip visibly tracks the pointer without perceptible lag

## 3. Chrome shadow scaling

- [ ] 3.1 Add a second, smaller shadow preset in `theme.rs`, tuned by eye for chrome's ~26-34px element scale, alongside the existing `card_shadow()`; cross-reference the two in doc comments so they don't drift independently
- [ ] 3.2 Update `paint_surface` to use the new chrome shadow preset instead of `card_shadow()`, leaving `paint_card` (treemap blocks) unchanged
- [ ] 3.3 Visually verify (via `--debug-screenshot` and live interaction) that toolbar buttons, the path field, and breadcrumb chips read as a subtle lift rather than a doubled/offset shape

## 4. Verification

- [ ] 4.1 Run `cargo test` to confirm no regressions in the scanner/treemap/theme/util unit tests
- [ ] 4.2 Walk through all three fixes live in the running app (per `CLAUDE.md`'s guidance to test UI changes in-browser/in-app, not just via tests) before considering the change done
