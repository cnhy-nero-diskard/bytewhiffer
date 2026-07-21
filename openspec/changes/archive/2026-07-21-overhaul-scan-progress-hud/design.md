## Context

`src/app.rs` runs scanning in two sequential phases: a background-thread walk (`ActiveScan`, drained each frame by `drain_scan`) followed by a resumable, budgeted authoritative-tree assembly (`PendingAssembly`, advanced each frame by `advance_pending_assembly`, per the `scan-responsiveness` capability). The HUD/status-bar chrome (`scan-status-bar` capability) was written as if "scan done" were a single moment — the walk finishing — and never accounted for the second phase:

```
walker thread returns
        │
        ▼
drain_scan(): finished = true
        │
        ├─ last_summary = { files, dirs, bytes, elapsed: scan_started_at.elapsed() }   ← frozen here (app.rs:735-743)
        ├─ pending_assembly = Some(PendingAssembly::start(entry))                       ← keeps running
        └─ self.scan = None

next frame onward:
   scan_hud()    → self.scan is None → doesn't render at all           (app.rs:916)
   status_bar()  → prints frozen last_summary as if fully complete     (app.rs:982-992)
   advance_pending_assembly() → still popping worklist, budgeted 8ms/frame, can span many more frames (app.rs:783-798)
```

On a tree large/deep enough for assembly to actually take multiple frames (the reason it's budgeted at all), the user sees a final-looking summary and elapsed time before the map has actually finished swapping in.

Separately, `format_duration` and `format_size` (`src/util.rs`) each hardcode one fixed precision used everywhere they're called, so the live HUD and the frozen summary share formatting that's tuned for neither case well.

## Goals / Non-Goals

**Goals:**
- Make "still working" — for HUD visibility and elapsed-time liveness purposes — cover both the walk and assembly phases.
- Finalize the completed-scan summary (in particular its elapsed time) only once assembly actually swaps the tree in, not when the walk thread returns.
- Give assembly a distinct, visible HUD state instead of silence.
- Introduce context-dependent decimal precision: more precision while a number is actively live-ticking, coarser once a value is finalized/historical.
- Smooth the scan-rate figure before increasing its displayed precision, so added decimals show trend, not sampling noise.
- Stabilize the HUD row's layout width as ticking digits change.

**Non-Goals:**
- No change to the scanner engines (`WalkerEngine`/`MftEngine`) or to `treemap::squarify`. This change is confined to the informational chrome in `app.rs`/`util.rs`.
- No change to `scan-responsiveness`'s pacing/budget mechanics (`SCAN_FRAME_BUDGET`, `SCAN_BUDGET_CHECK_INTERVAL`) — assembly keeps working exactly as it does today; only the *display* of its state changes.
- Not attempting to make the walk phase's progress bar a real percentage — the walk genuinely can't know a total ahead of time (existing `scan-status-bar` requirement, unchanged). Only the assembly phase gets a real fraction, since its total (`worklist` length) is knowable at `PendingAssembly::start`.

## Decisions

### 1. "In progress" becomes a two-phase predicate
Introduce the notion (not necessarily a literal new field — could be a helper method) of `scan_or_assembly_active() = self.scan.is_some() || self.pending_assembly.is_some()`. This predicate drives:
- Whether the elapsed-time clock reads `scan_started_at.elapsed()` live vs. reads the frozen `ScanSummary.elapsed`.
- Whether `scan_hud` renders at all (it currently renders only for `self.scan.is_some()`).

**Alternative considered:** keep `scan_hud`'s visibility tied only to `self.scan`, and instead have `status_bar` grow an "assembling…" branch. Rejected — it splits one logical "still working" state across two call sites with separate conditions, which is exactly the kind of drift that produced the current bug.

### 2. Move `ScanSummary` finalization to the assembly swap-in point
Today `ScanSummary` (including `elapsed`) is built in `drain_scan`'s finished branch, before `pending_assembly` is even created. Move the `elapsed` capture (and, by extension, the point at which `last_summary` is considered authoritative for display) to `advance_pending_assembly`, at the same point `self.root` is swapped in. `files`/`dirs`/`bytes` counters are already stable by the time the walk finishes (the engine contract guarantees final counts before returning), so only `elapsed`'s sampling point actually needs to move — but relocating the whole `ScanSummary` construction to the swap-in point keeps one code path responsible for "the scan is now fully, truly done" instead of splitting it across two.

**Alternative considered:** keep sampling `elapsed` at walk-finish, and separately track an "assembly done" flag the status bar checks before showing the summary at all (i.e. show neither the frozen nor a live number until both phases finish). Rejected as strictly worse UX — it reintroduces a silent gap (nothing shown during assembly) instead of a live-ticking one, which is the opposite of what "make it actually live" asked for.

### 3. Assembly gets a real fractional progress signal
`PendingAssembly::start` is handed the full `Entry` tree up front; walk it once (or track a running total-nodes count as `worklist` is populated, since it's already a stack push per child in `step`) to know a total item count `N` at start. Then `remaining = worklist.len()`, so the assembly phase can show `(N - remaining) / N` as a genuine completion fraction — unlike the open-ended walk phase, whose bar stays deliberately indeterminate per the existing (unchanged) requirement.

**Alternative considered:** derive a percentage from elapsed time and a historical average assembly rate. Rejected — needlessly speculative when an exact count is available for free from data already in hand.

### 4. Precision is contextual, not global
Rather than changing `format_duration`/`format_size` to always show more decimals, give them a precision parameter (or a `_live`/`_final` variant each) so call sites choose:
- Live elapsed (HUD, while `scan_or_assembly_active()`): sub-second precision (e.g. tenths).
- Finalized elapsed (status-bar summary once truly done): whole seconds, as today — sub-second precision is noise on a historical number nobody is watching tick.
- Live byte counter / live rate (HUD): one extra decimal beyond today's fixed `.1`.
- Finalized byte total (status-bar summary): unchanged, coarse.

**Alternative considered:** a single global higher-precision format used everywhere. Rejected per the proposal's own reasoning — precision is only signal while something is actively moving; applied to a frozen historical value it's just visual noise (and in `format_duration`'s case, actively misleading — implying a completed scan's time was measured to sub-second accuracy worth reading, when the point of that figure is a rough historical record).

### 5. Rate smoothing before rate precision
`scan_rate_bps` is currently a raw `bytes_delta / dt` computed once per second (`rate_sample`). Before showing an extra decimal on it, apply a simple exponential moving average (`rate = rate * (1 - α) + instantaneous * α` at each ~1s sample) so the added precision reflects trend rather than exposing per-second sampling jitter as if it were meaningful.

**Alternative considered:** shorten the sampling interval instead of smoothing. Rejected — a shorter interval amplifies jitter (smaller `dt` in the denominator), the opposite of what's needed; smoothing is the correct tool for "show more precision without also showing more noise."

### 6. Monospace/tabular figures for ticking numbers
Apply a fixed-width (tabular) font styling to the elapsed-time and byte/rate labels in `scan_hud` specifically (not necessarily the whole toolbar), so digit-count changes don't reflow neighboring HUD elements. egui's default font may or may not already have tabular figures for the digit glyphs used — verify during implementation; if not, this may need a monospace font swap scoped to just these labels via `egui::RichText`/`TextStyle`.

## Risks / Trade-offs

- **[Risk]** Moving `ScanSummary` construction later means `files`/`dirs`/`bytes` are read at a different point than today (assembly swap-in vs. walk-finish). → **Mitigation**: these counters are already frozen by the engine contract before the walk thread returns (`ScanEngine`'s guarantee), so reading them slightly later changes nothing about their value — only `elapsed`'s meaning changes (correctly, to "total including assembly").
- **[Risk]** Computing a total node count for assembly's fractional progress adds a bit of bookkeeping to `PendingAssembly::start`/`step`. → **Mitigation**: the count can be derived incrementally as children are pushed onto `worklist` (already happening in `step`), so no extra tree walk is needed — just an extra counter.
- **[Risk]** EMA smoothing adds a small amount of state (`smoothed_rate: f64` alongside the existing `rate_sample`) and a tunable `α`. → **Mitigation**: this is a narrow, self-contained addition next to existing rate-sampling code; α can start conservative (e.g. 0.3) and be adjusted by feel during manual testing, no persistence or config surface needed.
- **[Risk]** egui may not expose true tabular/monospace figures for the default proportional font used elsewhere in the toolbar, forcing a visually distinct font just for these labels. → **Mitigation**: acceptable trade-off — a monospace treatment on numeric HUD readouts is a common, recognizable convention (dashboards, terminals) rather than a jarring inconsistency.

## Open Questions

- Exact EMA `α` and live-elapsed decimal count (tenths vs. hundredths) are tuning choices best settled by looking at the running app, not decided abstractly here.
- Whether the assembly phase's real percentage should replace the walk phase's indeterminate bar visually (same widget, different fill semantics) or be a distinct second bar/label — left to implementation to decide by what reads clearest in `scan_hud`'s existing layout.
