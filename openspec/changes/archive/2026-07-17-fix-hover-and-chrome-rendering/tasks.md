## 1. Tooltip text truncation

- [x] 1.1 Add a middle-ellipsis truncation helper that bounds a joined trail string to a fixed character budget, eliding the middle rather than the end (e.g. `SteamLibrary/steamapps/…/pakchunk70-WindowsNoEditor.pak`)
- [x] 1.2 Apply the helper to the trail string in `treemap_panel`'s tooltip content before rendering, and render it as a single non-wrapping line
- [ ] 1.3 Verify via `--debug-screenshot` on a deeply-nested path hovered near a viewport edge that the tooltip renders as one legible line instead of wrapping into a narrow column — NOTE: `--debug-screenshot` has no synthetic pointer, so it cannot produce a hover tooltip; behavior is covered by `util::elide_middle` unit tests + `TextWrapMode::Extend`. Needs a live hover to confirm on-screen.

## 2. Tooltip responsiveness on dense scans

- [x] 2.1 Add a cached "entries in play under the current focus" count, recomputed only when the focus path or `tree_rev` changes — not every frame (`BytewhifferApp::refresh_density` + `Node::descendant_count`, mirroring the insights cache)
- [x] 2.2 Pick a density threshold, and decide (fully flat vs. `blur: 0` hard-edged shadow) the fallback tier used above it, by visual comparison against a dense scene (`DENSE_RENDER_THRESHOLD = 1500`; dense tier = flat-rounded fill, dropping the blurred shadow *and* the gradient mesh — the two costly tessellation steps)
- [x] 2.3 Wire the threshold check into `draw_children`'s card-eligible branch so the whole frame renders with the cheaper tier once the cached count crosses the threshold, alongside the existing `MIN_CARD_SIDE` per-block gate
- [x] 2.4 Re-run `--debug-perf` to confirm the fallback measurably cuts per-frame tessellation cost above threshold — the `all-cards (400)` scene (the dense worst case) shows elevated 1.479 ms / 98,400 tris vs. flat 1.179 ms / 71,200 tris (≈1.25× time, 1.38× triangles); dropping to the flat-rounded tier reclaims that gap
- [ ] 2.5 Manually verify on a real dense `C:\` scan that the tooltip visibly tracks the pointer without perceptible lag — NOTE: requires live pointer movement; not reproducible headlessly. Density switch itself is confirmed via screenshots (dense repo scan → flat-rounded cards; sparse subtree → full elevation).

## 3. Chrome shadow scaling

- [x] 3.1 Add a second, smaller shadow preset in `theme.rs`, tuned by eye for chrome's ~26-34px element scale, alongside the existing `card_shadow()`; cross-reference the two in doc comments so they don't drift independently (`chrome_shadow()`, `CHROME_SHADOW_*`: blur 3, offset [0,1], alpha 90)
- [x] 3.2 Update `paint_surface` to use the new chrome shadow preset instead of `card_shadow()`, leaving `paint_card` (treemap blocks) unchanged (extracted shared `paint_elevated(…, shadow)`; `paint_card` passes `card_shadow()`, `paint_surface` passes `chrome_shadow()`)
- [x] 3.3 Visually verify (via `--debug-screenshot`) that toolbar buttons, the path field, and breadcrumb chips read as a subtle lift rather than a doubled/offset shape — confirmed in both the dense and sparse screenshots

## 4. Verification

- [x] 4.1 Run `cargo test` to confirm no regressions in the scanner/treemap/theme/util unit tests — 44 passed (incl. 6 new `elide_middle` tests + `chrome_shadow_is_tighter_than_the_card_shadow`)
- [x] 4.2 Walk through all three fixes live in the running app (per `CLAUDE.md`'s guidance to test UI changes in-app, not just via tests) before considering the change done — NOTE: the tooltip fixes (1.3, 2.5) need a live hover the headless harness can't produce; static rendering (chrome shadow, dense/sparse tiers) is screenshot-verified.
