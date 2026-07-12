## Context

Today (`src/theme.rs`, `src/app.rs::draw_children`/`draw_tray_shell`), a
directory large enough to nest into renders as a "tray": a flat near-black
fill (`TRAY_FILL`, `#080b10`) one shade off the app background (`BG`,
`#0d1117`), a header strip carrying its name in a fixed non-hash-coded slate
(`DIR_HUE`/`DIR_SATURATION`/`DIR_VALUE`, identical for every directory
regardless of name), and a short inset-shadow gradient under the header.
Children are recursed into an inner rect shrunk by `TRAY_PAD` (3px) on all
four sides on top of `DIR_LABEL_H` (16px) for the header.

Two compounding problems came out of an `/opsx:explore` session grounded in
a real Steam-library scan screenshot:

1. Because `TRAY_FILL` and `BG` are nearly identical, any part of a tray not
   covered by a child card reads as void, not as "this is folder X's
   territory" — there is no positive visual signal of containment, only a
   thin header strip, and that strip is the same color for every directory
   so siblings can't even be told apart.
2. A chain of directories that each have exactly one child (a common real-
   world shape — e.g. `SteamLibrary → steamapps → common`) renders one
   full-width header bar per level before any real 2-D branching appears,
   burning vertical space on redundant, non-interesting levels.

This directly follows the `soft-elevation-theming` change (archived same
day), whose design doc explicitly flagged risk #2 above as a possibility
("nested directories several levels deep could stack inset-shadow trays
inside inset-shadow trays, reading as muddy") without a concrete mitigation
beyond the existing size floor — which doesn't help here, since each level
in the motivating example is well above that floor, just degenerate in
branching factor. Risk #1 (no containment/identity cue) wasn't anticipated
by that design at all.

Constraint: the file/leaf card treatment from `soft-elevation-theming`
(gradient, drop shadow, rounded corners, `MIN_CARD_SIDE` flat fallback) is
validated and should be left alone. This design only changes how a
directory's own container renders — its fill, border, padding, and header —
not how individual blocks floating inside it look.

## Goals / Non-Goals

**Goals:**
- Give each directory a visible, per-name-distinct container (frame + tint)
  so "these blocks belong to this folder" is legible without relying on a
  near-invisible fill color.
- Eliminate padding that reads as unexplained dead space by packing children
  flush against their parent's frame.
- Collapse redundant single-child directory chains into one header instead
  of one bar per level.
- Reuse the existing hash-based color infrastructure (`fnv1a`,
  `color_for_extension`'s approach) rather than inventing a second coloring
  scheme.

**Non-Goals:**
- Changing file/leaf card rendering (gradient, shadow, radius, size floor).
- Changing the squarify layout algorithm or its area-conservation guarantees.
- Changing breadcrumb/back navigation state (`BytewhifferApp::focus`) — only
  how many clicks it takes to reach a given trail via in-map interaction.
- Moving directory names to a corner-label overlay (no reserved header row
  at all) — noted as a possible follow-up, not part of this change.

## Decisions

**Muted per-directory hue, border + faint tint, not a full-saturation fill.**
Directories reuse the same FNV-1a-hash-of-name → hue mapping files use
(`color_for_extension`'s hash step), but map to a new, much lower
saturation/value band — distinct from both `BLOCK_SATURATION`/`BLOCK_VALUE`
(files) and the old flat `DIR_SATURATION`/`DIR_VALUE` — used for a border
stroke plus a very faint fill tint of the same hue. Considered and rejected:
(a) full file-strength saturation per directory — strongest identity signal,
but two competing vivid palettes (folders vs. files) undermines the
"curated, not noisy" palette goal from the original theming direction; (b)
a single neutral structural color for all directories (brighter/thicker
border, no hue) — solves the void/dead-space problem but not the "which
sibling is which" identity problem the exploration specifically asked for.

**Flush child packing.** The symmetric `TRAY_PAD` inset on all four sides is
replaced by an inset equal to the frame's own border-stroke width — children
begin immediately inside the visible frame line, not in a separate margin
on top of it. The header still reserves its own height at the top. This is
the direct fix for the "content looks centered/floating in a margin"
perception confirmed during exploration.

**Single-child chain collapsing happens before the size-based nest gate.**
Walking down from a node being drawn as a tray candidate: while the current
node has exactly one child and that child `is_dir`, append its name to an
accumulated label and advance to it (strict per-level check — a directory
with two children, even if one is tiny, does not collapse, matching the
exploration's chosen answer). This walk produces an *effective node* (the
first directory with zero or more-than-one children, or a file) before the
existing `would_nest`/`MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` gates run
against it — those gates are unchanged, they just now evaluate the
collapsed target instead of the immediate child. The rendered header shows
the full joined path (`SteamLibrary / steamapps / common`), clipped the same
way existing labels clip when they don't fit. The frame's color is derived
from the *effective* (terminal) node's name, since that's the actual
container being drawn.

**Depth counts once per collapsed chain, not once per skipped level.** The
`depth` parameter driving `depth_shift`/`MAX_DEPTH` increments by 1 for the
whole collapsed chain, not by the number of single-child hops it absorbed.
Otherwise a long single-child chain would push deeper real levels toward
`DEPTH_LIFT_MAX`/`MAX_DEPTH` prematurely relative to what's actually shown
on screen — depth should track meaningful visual containers, not raw
filesystem depth.

**Clicking a combined header drills the whole chain in one step.** The
`HitRect` for a combined header carries the full trail through the
effective node (all collapsed names plus the terminal node's own name), so
one click reaches the first level with real branching — today this takes
one click per level. The breadcrumb bar is unaffected: `self.focus` still
records every individual level, so back-navigation granularity (via
breadcrumb or back button) doesn't change, only the in-map click path gets
shorter.

**Drop the inset-shadow gradient mesh under the header.** Today's tray draws
a short gradient-mesh strip under the header to sell "recessed." With the
border+tint frame as the new containment cue, that mesh is redundant and is
removed — expected to be a net perf win (one less mesh per tray) as well as
simpler, but this should be confirmed, not assumed, via `--debug-perf`.

## Risks / Trade-offs

- **[Risk] A low-saturation band may not differentiate sibling folders
  enough to be useful** → **Mitigation**: tune the band's saturation floor
  against the real running app (via `--debug-screenshot*`) using several
  real sibling folder names side by side, not just a synthetic check;
  raise the floor if two common names hash too close in hue at low
  saturation.
- **[Risk] One-click multi-level drill could surprise a user expecting
  one-level-at-a-time navigation** → **Mitigation**: no capability is lost —
  the breadcrumb still exposes every intermediate level for stepping back
  granularly; only the forward in-map click path changes, and only for
  chains that had no meaningful intermediate branching to click into anyway.
- **[Trade-off] Depth now reflects "meaningful containers shown" rather than
  raw filesystem depth** — accepted, since the elevation/lift cue exists to
  communicate visual structure, not to be a literal depth counter.
- **[Risk] Removing the inset-shadow mesh could read as a regression from
  the validated "recessed" look** → **Mitigation**: this design treats
  border+tint as a *replacement* containment metaphor, not a removal of one;
  visually confirm via `--debug-screenshot-drill` at a few different depths
  before considering this done, alongside a re-run of `--debug-perf`.

## Migration Plan

Pure rendering change, no persisted state or data model involved. Implement
in `theme.rs` (new muted-hue-band function, updated tray fill/header color
helpers) then `app.rs` (chain-walk before the nest gate, revised padding,
frame painting), verify against `--debug-screenshot`/`--debug-screenshot-
drill` on a single-child-chain-heavy scan (Steam-library-shaped) and a
branchy/dense scan (Downloads-folder-shaped, matching the tree the prior
soft-elevation perf spike already models in `synth_dense_tree`), then re-run
`--debug-perf` to confirm the dropped inset-shadow mesh doesn't regress and
the new border stroke doesn't add meaningful cost. No rollback concerns
beyond reverting the commit — no external interface or persisted format
changes.

## Open Questions

- Whether a branching directory's header eventually moves to a corner-label
  overlay (no reserved strip at all) is a real follow-up idea from the
  exploration, deliberately deferred rather than folded into this change.
- Exact saturation/value numbers for the muted directory band, and the exact
  border-stroke width used for both the visual frame and the flush-packing
  inset, are implementation-time tuning — set against the real app, not
  fixed here (same approach the prior soft-elevation change used for its
  shadow/blur constants).
