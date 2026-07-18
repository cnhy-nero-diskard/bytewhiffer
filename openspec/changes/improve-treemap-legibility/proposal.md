## Why

The treemap's on-block labels and the Insights drawer's numeric breakdowns
both under-communicate scale today: a block shows only its name, never its
size, and the label text (a fixed near-white) reads poorly against the
elevation gradient's lightened top edge; the Insights drawer's "File types"
and "Biggest items" sections require reading numbers rather than glancing at
a WizTree-style proportional bar. These three gaps were scoped together in
one exploration pass and touch the same small rendering surface (block/tray
label painting in `app.rs`, row painting in the Insights drawer), so they're
bundled into one change.

## What Changes

- Add a size label to the top-right corner of every rendered box — both
  file-card blocks and directory tray headers — using the existing
  auto-scaling `format_size` (not a fixed unit), gated by a dedicated
  "room for both labels" check so small boxes never show a size label that
  would clip or overlap the name label.
- Give directory tray headers their own version of that gate: a collapsed
  single-child chain's joined name (e.g. `SteamLibrary / steamapps /
  common`) can already consume most of a header's width, so the size-label
  fit check accounts for that header's actual label width rather than
  reusing the file-card threshold as-is.
- Replace the on-block label text color — the file-card name label, the
  directory tray header label, and the new size label — from the fixed
  near-white `theme::TEXT` to a proportional darken (an alpha-blended black
  overlay) that darkens whatever color and lightness is underneath, rather
  than picking one fixed gray for every block.
- Add a horizontal proportional fill bar to each row of the Insights
  drawer's "File types" and "Biggest items" sections, its length scaled to
  that row's share of the currently focused subtree's total size (so bars
  rescale correctly as the user drills into a subdirectory), rendered in a
  flat neutral color distinct from the row's existing per-extension swatch.

## Capabilities

### New Capabilities
- `block-labels`: what text is painted on a rendered treemap block or
  directory tray header (name, and now size), the size-based gating that
  decides whether each label — and the combination of both labels sharing
  the top edge — is shown, including the directory-tray-specific gating
  that accounts for a collapsed chain's label width.

### Modified Capabilities
- `theming`: on-block label text (governed by `block-labels`) SHALL remain
  legible against the block's own fill color via a proportional darken,
  replacing the fixed light label color used today.
- `insights-drawer`: the "Top-extensions-by-size breakdown" and "Biggest
  files/folders leaderboard" requirements gain a proportional fill-bar
  visualization per row, scaled to the focused subtree's total size.

## Impact

- `src/app.rs`: `draw_children` (block name label, new size label, fit
  gating), `draw_tray_shell` (tray header label, new size label, tray-
  specific fit gating), `insights_panel` (row rendering for the File types
  and Biggest items sections, a new bar-paint helper alongside the existing
  `swatch` helper).
- `src/theme.rs`: a new proportional-darken label-text-color rule, a new
  neutral bar-fill color constant.
- No changes to `scanner/`, to `treemap::squarify` geometry, or to any
  `ScanEngine`.
