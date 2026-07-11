## 1. Engine-side: directories-scanned counter

- [ ] 1.1 Add `dirs_scanned: AtomicU64` to `ScanProgress` (`src/scanner/mod.rs`)
- [ ] 1.2 Increment it in `walker.rs::scan_dir` alongside the existing directory-discovery `ctx.emit(ScanEvent::Discovered { is_dir: true, .. })` call (`walker.rs:80-84`)
- [ ] 1.3 Update/add a walker unit test asserting `dirs_scanned` matches the number of directories in a fixture tree, mirroring the existing `progress_counters_track_files_and_bytes` test

## 2. App-side state for live metrics

- [ ] 2.1 Add a scan-start `Instant` to `BytewhifferApp`, captured in `start_scan`
- [ ] 2.2 Add a rolling `(Instant, u64)` sample pair for scan rate (MB/s), updated roughly once per second in `drain_scan` by diffing against the current `bytes_scanned`
- [ ] 2.3 Add a running "biggest top-level item" tracker (name + size) on `BytewhifferApp`, updated incrementally in `drain_scan` as `ScanEvent::Discovered` events for the scan root's direct children stream through — no full-tree walk
- [ ] 2.4 Add fields to hold the post-scan snapshot (files scanned, directories scanned, bytes scanned, elapsed time), populated in `drain_scan` immediately before `self.scan = None` drops `ActiveScan`

## 3. Toolbar: Rescan control

- [ ] 3.1 Add a "Rescan" button to `toolbar()`, enabled only when a previously-scanned root path is known, disabled otherwise
- [ ] 3.2 Wire it to call `start_scan` with the current root's path, without touching `path_input`

## 4. Scan HUD (new row, visible only while scanning)

- [ ] 4.1 Add a new render function for the HUD row, inserted between the toolbar and breadcrumb in the top panel, shown only when `self.scan.is_some()`
- [ ] 4.2 Render an indeterminate/pulsing progress bar — animation only, no percentage and no binding to any "total"
- [ ] 4.3 Render live metrics: files scanned, directories scanned, bytes scanned, scan rate, elapsed time (from state added in section 2)
- [ ] 4.4 Render "biggest top-level item so far" (name + size), from the tracker added in 2.3

## 5. Persistent bottom status bar

- [ ] 5.1 Add a new bottom panel, always shown regardless of scan state or whether a root has been scanned yet
- [ ] 5.2 Left side: hover readout mirroring the existing tooltip's path/size (reuse `hovered_path` state), showing a neutral placeholder ("Hover a block to inspect") when nothing is hovered
- [ ] 5.3 Right side: while a scan is in progress, show a neutral "Scanning…" placeholder instead of live counts, since the HUD already owns those
- [ ] 5.4 Right side: once a scan has completed, show the persisted snapshot from 2.4 (files/dirs/bytes/elapsed), and keep showing it until a new scan starts
- [ ] 5.5 Right side: show the active/last-used engine's name (`ScanEngine::name()`) alongside the summary

## 6. Verification

- [ ] 6.1 Run `cargo test` (scanner/theme/treemap/util suites) and fix any regressions from the `ScanProgress` field addition
- [ ] 6.2 Manually verify via `--debug-screenshot-live` that the HUD appears mid-scan with live-updating metrics and no percentage claim
- [ ] 6.3 Manually verify via `--debug-screenshot` that the bottom bar shows the persisted scan summary and engine name after completion
- [ ] 6.4 Manually verify the Rescan button re-triggers a scan against the same root without retyping the path
- [ ] 6.5 Confirm the HUD and bottom bar never display the same figure at the same time during an in-progress scan
