## 1. Precision helpers in `util.rs`

- [ ] 1.1 Split `format_duration` into a live variant (sub-second precision, e.g. tenths) and a finalized variant (whole seconds, current behavior), or add a precision parameter — call sites choose per context
- [ ] 1.2 Add an extra decimal of precision to `format_size` for live HUD call sites (byte counter, scan rate), without changing its output for finalized/summary call sites
- [ ] 1.3 Update/add `util.rs` unit tests covering both precision variants of `format_duration` and `format_size`

## 2. Two-phase "still working" state in `app.rs`

- [ ] 2.1 Add a helper (e.g. `fn scan_or_assembly_active(&self) -> bool`) that returns `self.scan.is_some() || self.pending_assembly.is_some()`
- [ ] 2.2 Change `scan_hud`'s visibility condition (and its call site in the `eframe::App::ui` panel layout) from `self.scan.is_some()` to the new helper
- [ ] 2.3 Move `ScanSummary` construction (currently in `drain_scan`'s finished branch) so `elapsed` is captured at the point `advance_pending_assembly` swaps `self.root` in, not when the walk thread returns
- [ ] 2.4 Update `status_bar` so the scan-summary section stays quiet (matching the existing in-progress branch) for as long as `scan_or_assembly_active()` is true, only showing `last_summary` once truly finalized

## 3. Assembly-phase visible progress

- [ ] 3.1 Track a total-item count in `PendingAssembly` as children are pushed onto `worklist` (incrementally, no extra tree walk), alongside the existing `worklist` remaining-length
- [ ] 3.2 In `scan_hud`, add a distinct "finishing up" sub-state shown when `self.scan.is_none() && self.pending_assembly.is_some()`, replacing the indeterminate bar with a real completion fraction derived from the assembly total/remaining counts
- [ ] 3.3 Keep the walk-phase progress bar indeterminate/unchanged (per the existing, unmodified requirement that the walk can't know a total ahead of time)

## 4. Elapsed-time liveness through both phases

- [ ] 4.1 Change the elapsed-time read in `scan_hud` (and wherever `status_bar` used to read the frozen `last_summary.elapsed` mid-scan) to keep reading `scan_started_at.elapsed()` live for as long as `scan_or_assembly_active()` is true
- [ ] 4.2 Use the live-precision `format_duration` variant for this reading, and the finalized variant once `last_summary` is actually set (post-swap-in)

## 5. Scan-rate smoothing

- [ ] 5.1 Add a smoothed rate value (EMA) alongside the existing `rate_sample`/`scan_rate_bps`, updated at the same ~1s cadence
- [ ] 5.2 Display the smoothed rate (not the raw per-second delta) in `scan_hud`, using the higher-precision `format_size` variant from task 1.2
- [ ] 5.3 Pick a starting smoothing factor (e.g. α = 0.3) by running the app against a large scan target and adjusting for feel

## 6. HUD layout stability

- [ ] 6.1 Apply monospace/tabular-figure styling to the elapsed-time and byte/rate labels in `scan_hud` (verify whether egui's default font already provides tabular figures for digits before introducing a separate font)
- [ ] 6.2 Manually verify (via `--debug-screenshot-live` against a large scan target) that the HUD row no longer visibly reflows as digits change width during a scan

## 7. Verification

- [ ] 7.1 Run `cargo test` (scanner/treemap/theme/util unit tests)
- [ ] 7.2 Manually verify against a large/deep local directory (real Windows round-trip, or `--debug-screenshot-live`/`--debug-screenshot`) that: the HUD stays visible and elapsed keeps ticking through the assembly phase, the status bar's summary only appears once assembly truly completes, and the assembly phase shows real completion progress
- [ ] 7.3 Manually verify against a small/fast scan target that behavior is unchanged when assembly completes within a single frame (per the design's non-goals — same-frame case should look identical to today)
