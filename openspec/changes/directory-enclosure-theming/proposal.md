## Why

The soft-elevation-theming change (archived same day as this proposal) made
directories render as a recessed tray — a near-black fill nearly
indistinguishable from the app background — with a header strip per level.
On real-world dense trees (e.g. a Steam library) this reads as jarring and
dead-space-heavy: directories with a single dominant child stack full-width
header bars several levels deep before any real branching appears, and a
directory's recessed body gives no visual hint that the blocks inside it
belong to it — it just looks like void around floating cards. This is
exactly the risk that change's own design doc flagged ("nested directories
several levels deep could stack inset-shadow trays inside inset-shadow
trays, reading as muddy") plus a related gap it didn't anticipate: no
per-folder color identity to make containment legible.

## What Changes

- Replace the directory's near-black recessed-tray fill (`theme::TRAY_FILL`)
  with a border/frame in a muted, desaturated hash-derived hue unique to
  that directory's name, plus a faint tint of the same hue as the fill —
  reusing the existing FNV-1a hash machinery files use for color, but pinned
  to a distinct, much less saturated band so directory identity never
  competes with vivid file colors.
- Reduce nested-child padding from a symmetric inset on all four sides down
  to essentially the frame's border-stroke width, so children pack flush
  against their parent's frame instead of floating in a margin that (via
  compounding with the existing gap-between-cards spacing) currently reads
  as unexplained dead space.
- Collapse consecutive directories that each have exactly one child into a
  single combined header (e.g. `SteamLibrary / steamapps / common`) instead
  of one stacked full-width header bar per level, rendering the frame around
  the first node that actually branches (more than one child, or a leaf
  file). Clicking anywhere on a combined header drills directly to that
  first-branching node's view in one step.
- File/leaf block rendering (raised card: gradient, drop shadow, rounded
  corners, minimum-size flat fallback) is unchanged — this only changes how
  a directory's own container renders.

## Capabilities

### New Capabilities
(none)

### Modified Capabilities
- `theming`: the "Depth communicated via elevation shift" requirement's
  directory-tray behavior changes from a fixed near-black recessed body to a
  per-directory muted-hue frame with flush child packing, and a new
  requirement is added for collapsing single-child directory chains into one
  combined header.

## Impact

- `src/theme.rs`: `TRAY_FILL`, `tray_header_color`, and the directory color
  path (`base_block_color`'s `is_dir` branch, `DIR_HUE`/`DIR_SATURATION`/
  `DIR_VALUE`) change; a new muted-hue-band function is added alongside the
  existing file `color_for_extension` band.
- `src/app.rs`: `draw_children` and `draw_tray_shell` change — chain-walking
  logic to detect and collapse single-child runs, revised padding constants
  (`TRAY_PAD` and how it's applied), and the tray fill/border painting.
  `HitRect` trail handling needs to account for a combined header's click
  target spanning multiple trail segments at once.
- `--debug-screenshot*` and `--debug-perf` (dense-tree tessellation spike)
  should be re-run against the new rendering to confirm the size-floor
  flat-fallback behavior and frame-stroke cost are still acceptable.
- No changes to the scanner, treemap layout algorithm, or navigation/
  breadcrumb state model.
