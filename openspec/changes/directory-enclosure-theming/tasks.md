## 1. Theme: muted per-directory hue band

- [ ] 1.1 Add a new muted saturation/value band for directory frames in
      `theme.rs` (distinct from `BLOCK_SATURATION`/`BLOCK_VALUE` used by
      files and from the old flat `DIR_SATURATION`/`DIR_VALUE`), plus
      constants for the frame border stroke width and the faint fill-tint
      alpha/lerp amount.
- [ ] 1.2 Add a function that hashes a directory's name (reusing `fnv1a`)
      into a hue within that muted band, mirroring `color_for_extension`'s
      shape but with the new band — this is the directory frame's border
      color.
- [ ] 1.3 Derive the frame's faint fill tint from the same border color
      (e.g. a low-alpha or heavily-lightened-toward-`BG` variant), replacing
      `TRAY_FILL`.
- [ ] 1.4 Update `tray_header_color` (or its replacement) to use the new
      per-name hue instead of the fixed `DIR_HUE`, keeping `depth_shift`
      applied on top as before.
- [ ] 1.5 Remove `TRAY_FILL` and `tray_inset_shadow` now that the inset-
      shadow mesh under the header is dropped in favor of the border+tint
      frame (per design.md's elevation-mesh decision).
- [ ] 1.6 Update/add unit tests: same directory name always yields the same
      frame color; different directory names differ in hue; frame colors
      stay within the new muted band (analogous to
      `extension_colors_stay_inside_the_band`); the muted band is distinct
      from both the file band and the old flat directory color;
      `depth_shift` still lightens frame colors monotonically and caps.

## 2. app.rs: single-child chain collapsing

- [ ] 2.1 Implement a chain-walk helper that, given a `Node`, follows
      children while each has exactly one child that `is_dir`, returning the
      joined chain of names and a reference to the effective (terminal)
      node — the first one with zero children, more than one child, or that
      is a file.
- [ ] 2.2 Wire the chain-walk into `draw_children` before the existing
      `would_nest`/`MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` gate, so that
      gate evaluates the effective node instead of the immediate child.
- [ ] 2.3 Render a single combined header for the whole collapsed chain
      (joined names, e.g. `SteamLibrary / steamapps / common`), clipped the
      same way existing labels clip when they don't fit.
- [ ] 2.4 Ensure `depth` advances by exactly one for the entire collapsed
      chain (not once per absorbed level), so `depth_shift`/`MAX_DEPTH`
      track visual containers shown, not raw filesystem depth.
- [ ] 2.5 Set the `HitRect` for a combined header's trail to the full path
      through the effective node, so a single click drills straight past
      every collapsed level in one step; confirm hover tooltip, right-click
      context menu (Open/Reveal/Delete), and the status-bar hover readout
      all target the effective node consistently.
- [ ] 2.6 Confirm a directory with more than one child (even if one child is
      much smaller than the other) never collapses — it always gets its own
      header and frame.

## 3. app.rs: frame painting and flush packing

- [ ] 3.1 Replace `draw_tray_shell`'s body/header painting with the new
      frame: border stroke (theme's muted per-name color) plus faint fill
      tint, dropping the inset-shadow mesh call.
- [ ] 3.2 Replace the symmetric `TRAY_PAD` inset used when recursing into a
      tray's children with an inset equal to the frame's border-stroke
      width, so children pack flush against the visible frame line instead
      of floating in a separate margin.
- [ ] 3.3 Re-check `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` constants
      still make sense against the new (smaller) padding — adjust if the
      reduced inset changes what counts as "big enough to nest."

## 4. Perf spike parity

- [ ] 4.1 Update `collect_bench_blocks`/`build_elevated_shapes` (the
      `--debug-perf` synthetic spike in `app.rs`) to mirror the new frame
      painting (border + tint, no inset-shadow mesh) instead of the old
      tray+inset-shadow shapes, so the spike still reflects real rendering
      cost.
- [ ] 4.2 Re-run `--debug-perf` against both existing scenes
      (`synth_dense_tree`, `synth_all_cards_tree`) and compare triangle
      counts / median frame time against the pre-change baseline to confirm
      dropping the inset-shadow mesh doesn't regress (expected to improve
      or be neutral, per design.md — verify, don't assume).

## 5. Verification

- [ ] 5.1 `cargo test` — scanner/treemap/theme/util unit tests pass,
      including the new/updated theme tests from section 1.
- [ ] 5.2 Build via the documented Windows GNU toolchain recipe
      (`cargo +stable-x86_64-pc-windows-gnu build --release` with WinLibs
      mingw64 on PATH) and confirm it compiles clean.
- [ ] 5.3 `--debug-screenshot-drill` (or `--debug-screenshot`) against a
      single-child-chain-heavy scan (e.g. a Steam library folder, matching
      the motivating screenshot) — confirm the chain collapses into one
      header and the frame reads clearly around the first branching level.
- [ ] 5.4 `--debug-screenshot` against a branchy/dense scan (e.g. a
      Downloads-style folder with many mixed files and a few nested app
      dirs) — confirm sibling directories are visually distinguishable by
      frame color, children pack flush with no dead-space gaps, and dense
      small-file clusters still fall back to flat rendering below
      `MIN_CARD_SIDE` as before.
