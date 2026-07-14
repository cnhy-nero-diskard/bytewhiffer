## Context

`app.rs` currently lays out three fixed panels around a central treemap: a top toolbar+HUD+breadcrumb panel, a bottom status bar, and the central treemap itself (`egui::Panel::top`, `egui::Panel::bottom`, `egui::CentralPanel`). There is no left/right panel today. The original design doc (`rust-space-sniffer-overview.md` §5) explicitly rules out "a separate directory tree/list side panel" so the app stays "a pure graphical map, like SpaceSniffer, unlike WinDirStat." An Insights drawer is a different kind of panel — derived aggregate stats, not a redundant re-listing of the tree — but it still competes for the same screen real estate and needs to honor the same intent: the map is the default view, not a dual-pane app.

The scan tree already lives in memory as `app.rs::Node` (live, incrementally built) or, post-scan, is convertible from `scanner::Entry`. Both shapes carry `name`, `path`, `size`, `is_dir`, `children` — enough to compute every Tier 1 insight without touching `scanner/`.

## Goals / Non-Goals

**Goals:**
- Insights are available on demand without permanently narrowing the treemap.
- Every insight is computed purely from the existing tree shape (no new fields, no new scanning pass).
- Insights describe the *focused* subtree (whatever the breadcrumb/drill-down currently shows), not always the whole scan — consistent with how the treemap itself only ever renders the focused node's children.
- The extension legend and the size breakdown reuse `theme::color_for_extension` directly, so the drawer's colors and the map's colors can never drift apart.

**Non-Goals:**
- No change to `Entry`, `ScanEngine`, or `walker.rs` (rules out file-age/staleness and scan-to-scan delta — deferred, see proposal).
- No new interactive filtering of the treemap (no dimming/highlighting non-matching blocks from a drawer click) — this change surfaces information and supports navigation-by-click into existing focus state, but does not add a filter/query layer.
- No duplicate-file detection (no hashing pass).
- No persistence of insights across app restarts or scans; the drawer is a live view over the current session's tree, recomputed as the tree changes.

## Decisions

**Drawer shape: `egui::SidePanel::left`, toggled by a toolbar button, closed by default.**
Alternative considered: a permanent left rail (always visible). Rejected — it would permanently narrow the treemap even when the user has no interest in insights, which is exactly the dual-pane shape the original design doc rejected. A collapsible drawer keeps the map full-width by default and makes insights something the user summons, not ambient chrome.
Alternative considered: a modal/overlay window instead of a docked panel. Rejected — insights are most useful *while* looking at the map (e.g., clicking a leaderboard entry to jump the treemap focus), which needs both visible simultaneously; a modal would block the map entirely.

**Insight computation lives in a new `src/insights.rs`, not inline in `app.rs`.**
Mirrors the existing separation of concerns: `treemap.rs` and `scanner/` are kept free of `egui` so they're unit-testable without a display. The aggregation functions (extension totals, leaderboard, blizzard/junk detection) are pure `Node`/`Entry`-shaped-tree-in, data-out functions, testable the same way `treemap::squarify` and `theme::color_for_extension` are tested today. `app.rs` stays the adapter that renders `insights.rs`'s output and wires click-to-focus.

**Recompute on demand each time the drawer is open and the focused node or tree identity changes, not incrementally during a live scan.**
Alternative considered: updating insights incrementally per `ScanEvent::Discovered`, the way `top_level_sizes`/`biggest_top_level` already are. Rejected for Tier 1: the existing incremental trackers exist because the scan HUD needs them *during* a scan with no full-tree walk available yet. The drawer's insights (full extension breakdown, leaderboard, blizzard detection) are naturally whole-tree aggregations; recomputing them from the current `Node` tree once per focus/tree change (not once per discovery event) is simpler and cheap enough — these are at most a few tree walks over data already in memory, not disk I/O. If a future profiling pass shows this is too slow on very large trees while a scan is live, incremental maintenance can be added without changing the drawer's rendering code.

**Junk-pattern matching is a fixed, hardcoded ruleset for Tier 1, not user-configurable.**
Keeps scope small; a settings surface for custom junk patterns is a reasonable future extension but not needed to validate the feature.

**Leaderboard and junk-flag entries reuse the existing `focus` navigation field to jump the treemap, the same mechanism the breadcrumb and click-to-drill already use.**
No new navigation concept is introduced; clicking a drawer entry is equivalent to clicking the corresponding treemap block would be, once drilled to its parent.

## Risks / Trade-offs

- **[Risk] Whole-subtree recomputation on every drawer-open/focus-change could be noticeably slow on very large trees (hundreds of thousands of entries) if done naively (e.g., re-hashing every extension string every frame).** → Mitigation: compute once per focus/tree-identity change (not per frame) and cache the result until the next change; `insights.rs`'s functions take a tree snapshot and return an owned result rather than being called from the render loop every frame.
- **[Risk] A left `SidePanel` toggled on top of the existing top/bottom panels could visually collide with the toolbar or breadcrumb at small window sizes.** → Mitigation: verify via `--debug-screenshot` at the app's minimum practical window size before considering this done; the panel should be given a fixed or min/max width range, not allowed to grow unbounded.
- **[Risk] "Known-junk" name-pattern matching can produce false positives (e.g., a legitimately named `installers` folder that isn't disposable) and erode trust if presented too assertively.** → Mitigation: present junk flags as suggestions surfaced alongside the existing Delete/Open/Reveal context menu (user still has to explicitly act), never as an auto-delete or one-click "clean all" action.
- **[Trade-off] Insights describe only the focused subtree, not the whole scan, for consistency with the treemap's own framing.** This means a user has to return to the root focus to see whole-scan totals; accepted because it matches the mental model the breadcrumb already establishes ("this view is scoped to where you are"), rather than introducing a second, differently-scoped concept of "current insights."
