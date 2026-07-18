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

## Migration Plan

Purely additive, UI-only rendering change — no persisted state, data model,
or scan-engine changes, so there is no data migration and no rollback
complexity beyond reverting the commit. Land in one pass, then verify
visually (this surface can only really be confirmed by eye, same as prior
theming changes):
1. `cargo test` — existing scanner/treemap/theme/util unit tests plus any
   new `theme.rs` color-rule tests continue to pass headlessly.
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

## Open Questions

- Exact pixel/alpha tuning (the size-label fit thresholds, the darken
  overlay's alpha, the bar green's hue/alpha) is left to implementation-
  time visual tuning via the debug-screenshot flags, not decided here.
- Whether "Small-file clutter" and "Junk suggestions" should eventually get
  the same fill-bar treatment — explicitly deferred, not part of this
  change.
