## Why

The scan HUD and status bar currently lie about being done. `drain_scan` freezes the scan summary and clears `self.scan` the instant the walker thread returns, but the authoritative-tree assembly (`PendingAssembly`, see the `scan-responsiveness` capability) can keep running for many more frames on a large or deep tree. Once `self.scan` goes `None`, the in-flight HUD disappears entirely and the bottom status bar prints a final, static "N files · M dirs · X GB · Ys" summary — while the map is still silently reassembling underneath it. On top of that, the two numbers doing the most second-to-second work (elapsed time, byte/rate readouts) are needlessly coarse: `format_duration` never shows sub-second precision even while ticking live every ~100ms, and `format_size` hardcodes exactly one decimal everywhere, hiding real movement in both the byte counter and the scan-rate readout.

## What Changes

- Redefine "scan in progress" for HUD-lifetime purposes as `self.scan.is_some() || self.pending_assembly.is_some()`, not just scan-thread-running. The elapsed-time clock keeps reading live for as long as either is true.
- Stop finalizing `ScanSummary` (in particular its `elapsed` field) at the moment the walker thread returns. Finalize it only once `advance_pending_assembly` performs the atomic swap of `self.root`, i.e. once assembly has actually completed.
- Give the assembly phase its own visible HUD state instead of going dark: while assembly is pending, show a "finishing up" indicator distinct from the walk phase's indeterminate bar — real progress if a total item count is captured at `PendingAssembly::start` and compared against remaining `worklist` length, since (unlike the open-ended walk) assembly's total work is knowable up front.
- Add sub-second decimal precision to the live elapsed-time display while a scan (walk or assembly) is actively running; keep the completed-scan summary's elapsed time at coarser (whole-second) precision, since sub-second precision is signal only while something is visibly moving.
- Add decimal precision to the live byte counter and scan-rate readout in the HUD (`format_size`'s hardcoded single decimal is the current bottleneck).
- Smooth the scan-rate figure (e.g. an EMA over the existing once-per-second `rate_sample`) before adding display precision, so extra decimals expose real trend rather than raw per-second sampling noise.
- Use tabular/monospace figure rendering for the ticking elapsed-time and byte-count numbers so digit-count changes (`"9s"` → `"10s"`, `"3.1 GB"` → `"12.4 GB"`) don't reflow the HUD row on every tick.

## Capabilities

### New Capabilities
(none)

### Modified Capabilities
- `scan-status-bar`: the in-flight HUD's lifetime extends to cover tree assembly (not just the walk phase), the assembly phase gets a distinct visible state, the completed-scan summary is no longer stamped until assembly truly finishes, and elapsed-time/byte/rate figures gain deliberate, context-dependent decimal precision.

## Impact

- `src/app.rs`: `drain_scan`'s finished branch (`ScanSummary` construction, `self.scan = None`), `advance_pending_assembly` (swap-in point becomes where the summary is finalized), `scan_hud` (visibility condition, new assembly-phase sub-state, rendering of higher-precision figures, monospace digit styling), `status_bar` (elapsed no longer sourced from a value frozen too early), `rate_sample`/`scan_rate_bps` (smoothing).
- `src/util.rs`: `format_duration` and `format_size` gain precision parameters/variants instead of fixed formatting.
- No scanner-engine or treemap-layout changes; this is purely the informational chrome around an already-correct two-phase scan/assembly pipeline.
