## Context

Today a rendered treemap block paints exactly one text: its name, top-left,
gated by a single fixed-width fit check (`label_fits` in `draw_children`,
`app.rs:1780`). Directory tray headers (`draw_tray_shell`, `app.rs:1861`)
paint their name (or, for a collapsed single-child chain, the joined chain
name) unconditionally whenever the tray itself renders — no separate fit
gate. Both call sites hardcode `theme::TEXT` (near-white) for that text,
which sits right at the top of the card — exactly the region the elevation
gradient (`theme::gradient_stops`, `GRAD_LIGHTEN = 0.13`) lightens toward
white, so the label is light text on the lightest part of the surface.

The Insights drawer (`insights_panel`, `app.rs:1141`) renders its "File
types" and "Biggest items" sections as plain rows (swatch/icon + text +
right-aligned size), built with stock `ui.horizontal(...)` layout. The data
those rows already have — `InsightsData.ext_totals` and `.leaderboard`,
computed once per focus/tree-revision change in `refresh_insights`
(`app.rs:1110`) — comes from walking an `InsightNode` (`insights.rs:31`),
which already carries a `size: u64` field. The `view` built in
`refresh_insights` (`app.rs:1124`) is the `InsightNode` rooted at the
currently focused node, so `view.size` is already, today, the exact
denominator this change needs for "share of the focused subtree" — no new
aggregation pass, just one more field threaded through `InsightsData`.

Separately, `drain_scan` (`app.rs:551`) is called at the top of every
`ui()` frame and drains the *entire* currently-queued backlog of
`ScanEvent`s via `scan.events.try_iter()` — no per-frame cap. Each event
runs `Node::insert` (`app.rs:93`), which allocates a `Vec<String>` of path
components and walks/creates nodes top-down through a `HashMap` with two
string clones per level. The channel itself is unbounded
(`mpsc::channel`, `app.rs:515`), so a dense directory's rayon-parallel
walker (`scan_dir`'s `into_par_iter()` over subdirectories,
`walker.rs:112`, using rayon's default `num_cpus`-sized global pool) can
queue backlog far faster than the ~100ms repaint cadence
(`ctx.request_repaint_after`, `app.rs:1515`) drains it. Whichever frame
finally gets a turn processes the whole pile-up synchronously before
`ui()` returns and egui can repaint. At scan completion
(`app.rs:612`-`621`), the authoritative `Entry` tree returned by the
engine thread is converted into the UI's `Node` tree via one uninterrupted
call to `Node::from_entry` — a full recursive rebuild of every entry,
discarding the already-built live tree and redoing the same insertion
work in a single frame. Both are instances of the same pattern (unpaced
bulk work dumped into one frame) at two different moments, not a scan-
thread-parallelism problem: capping rayon's pool would only soften a
diffuse "everything's a bit sluggish while scanning" feel, not these
sharp stutters, and risks slowing the scan itself. The `Entry` tree must
stay the source of truth at completion regardless of pacing — the
`ScanEngine` contract (`scanner/mod.rs:118`-`128`) explicitly allows a
future non-streaming engine (the MFT "turbo" reader) to emit live events
late, coarsely, or not at all, so the already-built live tree cannot be
assumed complete and simply kept as-is.

## Goals / Non-Goals

**Goals:**
- A size label, auto-unit-formatted, on every box (file card or directory
  tray) large enough to show it without clipping or colliding with the
  existing name label.
- On-block label text that stays legible across the full range of block
  hues/lightness bands and the elevation gradient's lightened top, without
  per-color tuning.
- A proportional fill bar on the Insights drawer's "File types" and
  "Biggest items" rows, scaled to the currently focused subtree's total
  size, so it rescales correctly as the user drills in or out.
- No single frame's scan-data processing (live event draining, or the
  post-scan authoritative-tree assembly) takes longer than a small,
  bounded time slice, regardless of how large or file-dense the scanned
  directory is.

**Non-Goals:**
- No change to `treemap::squarify` geometry, layout, or sizing.
- No change to what data is computed — `extension_totals`/`leaderboard`
  already exist; only their presentation gains a size label or bar.
- No change to `scanner/` or any `ScanEngine`.
- No per-extension/per-entry tinted bars — the bar is one flat neutral
  color for every row, deliberately distinct from the existing per-
  extension swatch.
- No bars for the "Small-file clutter" or "Junk suggestions" sections —
  out of scope for this change; a natural follow-up if wanted later.
- No change to `scanner/`, any `ScanEngine`, or rayon's thread-pool
  configuration — the responsiveness fix is entirely in how `app.rs`
  paces its own consumption of scan output already being produced today.
- No attempt to make the live tree authoritative or skip the post-scan
  assembly outright — the `ScanEngine` contract requires the final
  `Entry` tree to remain the source of truth, since a future engine may
  not stream complete live events.

## Decisions

**Reuse `util::format_size` for the size label, not a fixed GB unit.**
The hover tooltip and Insights panel already format sizes this way; a file
small enough to render as its own box but forced into GB would show
misleading values like "0.0 GB". Alternative considered: literal always-GB
formatting — rejected, since it actively misinforms for anything under
~100 MB.

**Gate the size label with its own, measured fit check — not a reused or
fixed-pixel threshold.** `label_fits` was tuned for one string that's
allowed to run under the paint's clip rect; a right-aligned size string
can't rely on the same clipping (it would visually collide with the name
rather than get invisibly cut). The size-label gate measures the rendered
width of the formatted size string via the same galley-measurement pattern
the chrome toggle buttons already use (`app.rs:1978`-`2077`) and requires
enough spare width beyond a reserved name column before painting it.
Alternative considered: a single fixed pixel constant tuned by eye —
rejected, since the size string's length varies meaningfully ("12 B" vs.
"999.9 GB") and a fixed threshold would either falsely collide on the long
end or needlessly hide the label on the short end.

**Directory tray headers get their own version of that gate, measuring the
header's actual label width.** A collapsed chain (`collapse_chain`,
`app.rs:1702`) can produce an arbitrarily long joined name that alone
crowds a header. Reusing the file-card gate as-is would let a size label
render over a long chain name. The tray gate measures the header's already-
computed label string the same way, then checks remaining header width.
Alternative considered: a fixed threshold shared with file cards — rejected
for the same collision risk as above, specifically for long chains.

**Label darkening is a proportional darken (alpha-blended black overlay),
not a fixed gray constant.** One new `theme.rs` color rule (an alpha-black
`Color32`) replaces `theme::TEXT` at all three label sites: the file-card
name, the tray header name, and the new size label. Because it composites
over whatever is beneath it, it stays legible whether the block's base hue
sits at the file band (`BLOCK_VALUE = 0.46`) or the directory band
(`DIR_FRAME_VALUE = 0.60`), and whether that block is currently rendered
elevated (gradient-lightened top), dense-tier flat, or flat-fallback.
Alternative considered: swap to `theme::TEXT_SUBTLE` (or a new fixed
darker gray) — rejected per explicit product direction: a single gray
picked for the dark chrome background doesn't adapt the same way across
varying block hues/lightness, and the ask was specifically for text that
darkens *relative to* each box, not a uniform gray.

**Insights bar baseline is the focused subtree's total (`view.size`),
recomputed on every focus change — not the largest row in the list.**
`InsightsData` gains one new `total_size: u64` field, set from `view.size`
in `refresh_insights` alongside the existing `ext_totals`/`leaderboard`
fields; each row's bar fraction is `entry_size as f64 / total_size as f64`.
Because `refresh_insights` already recomputes on focus/tree-revision change,
bars rescale for free when the user drills in or out. Alternative
considered: scale relative to the largest row currently listed (closer to
WizTree's literal behavior) — explicitly not chosen; this product wants a
stable "% of what I'm currently looking at" reading instead.

**Bar color is a single new flat neutral (green) constant in `theme.rs`,
not the row's own swatch color.** Deliberately distinct from both the
existing per-extension swatch (keeps the swatch as the one color that must
never drift from the treemap, per the existing doc comment at
`app.rs:1163`-`1166`) and from the single reserved `ACCENT` (blue), which
`theming`'s existing requirement reserves for hover/breadcrumb/selection
interaction state only. This is a data-visualization surface, not an
interactive one, so it's a deliberately separate semantic color rather
than a reuse of either existing one.

**Bars are painted with a manually reserved `Rect`, not a stock `egui`
widget.** Each row reserves its rect (`ui.allocate_exact_size`, the same
pattern `swatch()` already uses at `app.rs:1597`-`1600`) and paints the
fill bar into it before the swatch/label/size content, so the bar sits
behind the row's existing widgets rather than replacing them. Alternative
considered: egui's stock `ProgressBar` — rejected; it's a self-contained
widget that owns its own text/fill and doesn't compose cleanly with the
row's existing swatch + two text labels sitting on top of it.

**`drain_scan` processes events within a wall-clock time budget per call,
not an item count.** A fixed item-count cap (e.g. "process 500 events per
frame") would still stall unpredictably depending on how expensive each
`Node::insert` happens to be for that particular path depth; a time
budget bounds the actual frame-blocking duration directly, which is the
thing users experience as a freeze. The loop checks elapsed time
periodically (not after literally every single event, to avoid paying a
clock read per item) and stops once the budget is spent, leaving the rest
of the channel's backlog queued for the next frame's call — which arrives
either at the next repaint tick or the next input-driven frame, whichever
comes first. Alternative considered: pause draining entirely while the
backlog exceeds some threshold, resuming only once it drops — rejected,
since it reintroduces a stall/resume cliff instead of smooth, bounded
per-frame progress.

**The post-scan `Node::from_entry` rebuild becomes a resumable, explicit-
worklist traversal instead of a plain recursion, paced by the same time
budget.** Plain recursive calls can't be paused mid-stack-frame across
`ui()` invocations, so the traversal is restructured around an explicit
stack/queue of pending `(Entry, target-parent)` work items that the app
can process a bounded slice of per frame and resume later from the same
struct, mirroring how `drain_scan` already checks elapsed time per unit of
work. The existing live tree (`self.root`) stays displayed and
interactive throughout — the resumable assembly builds into a separate,
not-yet-visible tree — and only replaces `self.root` in one atomic swap
once assembly fully finishes, so the displayed tree is never a partially-
assembled authoritative one. Alternative considered: keep the live tree
as final and skip the rebuild — rejected per the `ScanEngine` contract
above; a future non-streaming engine can't be trusted to have produced a
complete live tree.

## Risks / Trade-offs

- **[Risk]** An alpha-blended black label could read as low-contrast on an
  unusually dark rendering path (e.g. a block color combined with a
  non-obvious future theming change that lowers the block value band) →
  **Mitigation:** both current bands (`BLOCK_VALUE = 0.46`,
  `DIR_FRAME_VALUE = 0.60`) are fixed, mid-to-light, and labels always
  paint in the same top-corner region across all three render tiers
  (elevated card, dense tier, flat fallback); verify by eye with
  `--debug-screenshot`/`--debug-screenshot-drill` across a real tree before
  calling this done, per this repo's established practice for anything
  visual that can't be unit-tested.
- **[Risk]** A measured-width fit gate adds a per-frame text-measurement
  cost per visible block (via galley layout) that today's fixed-threshold
  check doesn't pay → **Mitigation:** the same measurement approach is
  already paid per-frame by the chrome toggle buttons; block counts at any
  one on-screen depth are bounded by the existing nest-gate constants, so
  this is not a new order-of-magnitude cost. Re-run `--debug-perf` if it
  turns out otherwise.
- **[Risk]** Two new hardcoded colors (label-darken alpha, bar green) begin
  to erode the "one reserved accent" simplicity the theming spec currently
  states → **Mitigation:** both are explicitly scoped to non-interactive
  surfaces (on-block label legibility; Insights-panel data visualization),
  so they don't compete with `ACCENT`'s reserved interactive meaning; this
  is called out explicitly in the `theming` and `insights-drawer` delta
  specs rather than left implicit.
- **[Risk]** Keeping the live tree displayed while the authoritative
  assembly runs in the background means the two trees can briefly
  disagree (e.g. if the live tree missed something the walker's streaming
  is best-effort about) → **Mitigation:** this window is bounded by the
  same time budget and ends the moment assembly finishes and swaps in;
  it's strictly better than today's behavior, where the live tree is
  shown until one big synchronous freeze replaces it with the correct one
  anyway.
- **[Risk]** Converting `Node::from_entry`'s plain recursion into a
  resumable worklist traversal is a real structural change to code that
  currently has no tests exercising pause/resume behavior →
  **Mitigation:** the existing `computes_recursive_sizes`-style walker
  tests and a new resumability-focused test (assembling a synthetic large
  tree across multiple simulated "frames" and asserting the result
  matches a one-shot `from_entry` on the same input) cover correctness;
  see task 4 in `tasks.md`.

## Migration Plan

Purely additive, UI-only rendering and scan-orchestration change — no
persisted state, data model, or `ScanEngine` changes, so there is no data
migration and no rollback complexity beyond reverting the commit. Land in
one pass, then verify (this surface is partly unit-testable, partly only
confirmable by eye, same as prior theming changes):
1. `cargo test` — existing scanner/treemap/theme/util unit tests plus any
   new `theme.rs` color-rule tests and the new resumable-assembly test
   continue to pass headlessly.
2. `cargo check --target x86_64-pc-windows-gnu` — confirms the Windows-only
   surface still type-checks (this change shouldn't touch any
   `#[cfg(windows)]` code, but the check is cheap and already this repo's
   habit).
3. `--debug-screenshot`, `--debug-screenshot-live`, and
   `--debug-screenshot-drill` against a real, varied tree (dense small-file
   clusters, a deep single-child chain, a large scan) to confirm: size
   labels appear/disappear at sensible sizes without clipping or collision,
   label text reads clearly against multiple block hues, and the Insights
   bars render and rescale correctly when drilling in and out.
4. On real Windows hardware, scan a large/dense real directory (e.g. a
   deep `node_modules` tree or a full user profile) with and without this
   change and confirm the mid-scan and post-scan stutters are gone or
   substantially shortened — this, like turbo-mode elevation and
   `--debug-perf`, can only be judged by a human on real hardware, not
   from this Linux dev environment.

## Open Questions

- Exact pixel/alpha tuning (the size-label fit thresholds, the darken
  overlay's alpha, the bar green's hue/alpha) is left to implementation-
  time visual tuning via the debug-screenshot flags, not decided here.
- Whether "Small-file clutter" and "Junk suggestions" should eventually get
  the same fill-bar treatment — explicitly deferred, not part of this
  change.
- The exact per-frame time budget (a single tuned constant, shared by
  `drain_scan` and the resumable assembly, or two separate constants) is
  left to implementation-time tuning against `--debug-perf`-style
  measurement, not decided here.
- Whether the resumable-assembly window needs any UI indication beyond the
  existing scan HUD (e.g. "finalizing…") is left open — deferred to
  implementation, since it depends on how long the window actually turns
  out to be in practice on real hardware.
- Whether rayon's global thread-pool size should eventually be capped
  below `num_cpus` to leave headroom for the UI thread — explicitly
  considered and set aside for this change (see Context above); a
  possible follow-up, not part of this change.
