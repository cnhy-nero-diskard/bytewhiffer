## Why

A treemap block's color is currently undecodable — the hash-derived hue that makes sibling extensions distinguishable carries no legend, so "why is this file teal" has no answer. More broadly, the map shows *where* space went but gives no assist for *what to do about it*: spotting the biggest offenders, dense small-file clutter, or common junk (installers, build caches, `node_modules`) all require eyeballing the tree by hand. A collapsible "Insights" drawer surfaces these as derived analytics over the tree a scan already produces, with zero new scanning cost.

## What Changes

- Add a toolbar toggle that opens/closes a left-side "Insights" drawer. Closed by default so the treemap stays full-width — this preserves the original design decision that the app is "a pure graphical map, like SpaceSniffer, unlike WinDirStat" (`rust-space-sniffer-overview.md` §5); the drawer is summoned chrome, not a permanent second pane.
- Drawer content, all computed from the already-in-memory focused `Node`/`Entry` tree (no scanner or `Entry` schema changes):
  - **Extension color legend**: every extension present in the current tree, paired with the color `theme::color_for_extension` already assigns it.
  - **Top-extensions-by-size breakdown**: total size per extension across the focused subtree, rendered with the same colors as the legend and the treemap itself.
  - **Biggest files/folders leaderboard**: top-N entries by size in the focused subtree; clicking one focuses the treemap on that path via the existing `focus` navigation state.
  - **Small-file-blizzard flag**: directories with a high child count but low average child size (e.g. `node_modules`-style clutter).
  - **Known-junk suggestions**: directories/files matching common junk name patterns (installers, build caches, `node_modules`, browser cache dirs), surfaced as a list that hooks into the existing right-click Delete/Open/Reveal context menu rather than adding new action UI.
- All drawer content recomputes when the focused node or the root tree changes (drilling in/out, a new scan completing) — it describes the *current view*, not a fixed whole-scan snapshot.

## Capabilities

### New Capabilities
- `insights-drawer`: a collapsible analytics panel presenting derived, read-only insights (legend, size breakdown, leaderboard, clutter/junk flags) about the currently focused subtree, computed without any new scan data.

### Modified Capabilities
(none — no existing capability's requirements change; the drawer is additive UI reading data `treemap-navigation` and `theming` already expose)

## Deferred (explicitly out of scope for this change)

Recorded here so they aren't lost, not because they're unimportant:

- **Tier 2 — needs new scanner metadata**: file-age/staleness tracking and scan-to-scan delta both require adding a modification-time field to `Entry` and threading it through `walker.rs`. This change touches no scanner types, so both wait for a follow-up change.
- **Tier 3 — larger, separately-scoped efforts**: name/size/age filtering that dims non-matching treemap blocks (this reopens the V2 filtering item already deferred in `rust-space-sniffer-overview.md` §5, rather than being new scope here); duplicate-file detection via content hashing (an I/O-heavy, opt-in feature in its own right, not an "insight" over data already scanned).

## Impact

- `src/app.rs`: new drawer render function, toolbar toggle button/state, panel layout change (`egui::SidePanel` alongside the existing top/bottom panels and central treemap panel).
- New module (e.g. `src/insights.rs`): pure functions computing legend entries, extension size totals, the leaderboard, and clutter/junk flags from a `Node`/`Entry`-shaped tree — kept free of `egui` where possible so the aggregation logic stays unit-testable, mirroring how `treemap.rs`/`scanner/` are kept display-independent today.
- `src/theme.rs`: no logic changes; `color_for_extension` is reused, not modified.
- No changes to `src/scanner/` (`Entry`, `ScanEngine`, `walker.rs`) — this is the load-bearing constraint that keeps this change Tier-1-only.
