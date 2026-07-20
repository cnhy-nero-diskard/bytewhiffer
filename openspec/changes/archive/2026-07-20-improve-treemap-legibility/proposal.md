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

A fourth issue surfaced in a follow-up exploration pass and is folded in
here at the user's explicit direction: bytewhiffer briefly freezes the UI
while scanning large, file-dense directories. Two distinct causes were
found in `app.rs`'s scan-orchestration code, both the same underlying
pattern — unpaced bulk work dumped into a single frame — surfacing at two
different moments. Mid-scan, `drain_scan` drains the *entire* currently-
queued backlog of `ScanEvent`s synchronously once per frame; a dense
directory's rayon-parallel walker can queue backlog faster than the
~100ms repaint cadence drains it, so whichever frame finally gets a turn
processes the whole pile-up in one go. At scan completion, the
authoritative `Entry` tree is converted into the UI's `Node` tree via one
uninterrupted recursive rebuild (`Node::from_entry`), discarding the
already-built live tree and redoing the same insertion work for every
entry in a single frame — likely the larger of the two freezes on a truly
huge tree. Neither is a thread-count problem: pacing the UI's own
consumption of already-produced scan data, not the scan thread's
parallelism, is what determines freeze length. (A rayon-pool-size angle
was considered and explicitly set aside — it would only soften the
diffuse "everything's a bit sluggish while scanning" feel, not these sharp
stutters, and risks slowing the scan itself, which cuts against
bytewhiffer's whole reason for existing over SpaceSniffer.)

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
- Bound `drain_scan`'s per-frame event processing to a wall-clock time
  slice rather than draining the whole backlog every call, so a discovery
  burst from a dense directory spreads its tree-insertion cost across
  multiple frames instead of stalling one.
- Replace the scan-completion `Node::from_entry` full synchronous rebuild
  with a paced, resumable assembly that runs within that same per-frame
  time budget across multiple frames, keeping the existing live tree on
  screen (and interactive) until the authoritative tree finishes
  assembling, then swapping it in atomically.

## Capabilities

### New Capabilities
- `block-labels`: what text is painted on a rendered treemap block or
  directory tray header (name, and now size), the size-based gating that
  decides whether each label — and the combination of both labels sharing
  the top edge — is shown, including the directory-tray-specific gating
  that accounts for a collapsed chain's label width.
- `scan-responsiveness`: how the UI paces its consumption of in-flight
  scan data — both live discovery events and the final authoritative-tree
  handoff — so that no single frame's processing work is unbounded,
  keeping the app responsive while scanning (and immediately after
  completing a scan of) large, file-dense directories.

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
  `swatch` helper), `drain_scan` (time-boxed event processing), the scan-
  completion path (paced `Node` assembly from the authoritative `Entry`
  tree, replacing the one-shot `Node::from_entry` call), `ActiveScan` and
  related state (new fields tracking resumable-rebuild progress across
  frames).
- `src/theme.rs`: a new proportional-darken label-text-color rule, a new
  neutral bar-fill color constant.
- No changes to `scanner/`, to `treemap::squarify` geometry, or to any
  `ScanEngine` — the responsiveness fix is entirely in how the UI consumes
  already-existing scan output, not in how scanning itself works.
