## Why

Bytewhiffer's chrome is too barren to feel finished: the toolbar shows only a plain spinner and a "files · bytes" line while scanning, that information is thrown away the moment a scan completes (the toolbar regresses to just a total-bytes figure), there's no way to re-run a scan on the current root without re-picking or re-typing the path, and there's no bottom status bar at all — no persistent way to inspect a hovered block without holding the mouse still over a tooltip, and no lasting record of what a finished scan found. This surfaced from an `/opsx:explore` session that worked through what a toolbar and status bar should actually show, including an explicit decision to reject a determinate (percentage) progress bar, since the parallel walker has no way to know a scan's total size ahead of time.

## What Changes

- The toolbar gains a **Rescan** button that re-runs a scan against the current root without re-selecting or re-typing the path.
- A new **scan HUD row** appears between the toolbar and breadcrumb only while a scan is in flight: an indeterminate/pulsing progress bar (explicitly not a percentage) plus live metrics — files scanned, directories scanned, bytes scanned, scan rate (MB/s), elapsed time, and the largest top-level item discovered so far (name + size).
- A new **persistent bottom status bar** is always present: a hover readout on the left (mirrors the existing block tooltip's path/size, but doesn't disappear when the pointer moves, showing a neutral placeholder when nothing is hovered), and on the right a scan summary (files/directories/bytes/elapsed) plus the active engine's name. The summary now **survives scan completion** instead of being discarded, and goes quiet ("Scanning…") while the HUD row owns the live numbers, so the two bars never show duplicate figures.
- `ScanProgress` gains a directories-scanned counter alongside the existing files/bytes counters, updated by any conforming `ScanEngine` (the walker today, a future engine later).
- **Explicitly out of scope**, considered and rejected during exploration: a determinate/percentage progress bar (would require either a pre-count double-pass that hurts scan speed, or a whole-drive-only free-space comparison needing a new `windows` crate dependency); drive quick-pick buttons, a recent-paths dropdown, a copy-path button, a quick-filter/search box (overlaps the already-deferred V2 filtering feature), and a light/dark theme toggle (conflicts with the single deliberately-designed dark palette).

## Capabilities

### New Capabilities
- `scan-status-bar`: the toolbar's Rescan control, the in-flight scan HUD (indeterminate progress + live metrics), and the persistent bottom status bar (hover readout, post-scan summary, engine name).

### Modified Capabilities
- `disk-scanning`: the "Live progress reporting" requirement is extended so progress also tracks directories scanned, not just files and bytes, since the new HUD and status bar both display a directory count.

## Impact

- `src/scanner/mod.rs`: `ScanProgress` gains a `dirs_scanned: AtomicU64` counter.
- `src/scanner/walker.rs`: increments the new counter alongside the existing directory-discovery event emission.
- `src/app.rs`: `toolbar()` gains the Rescan button; a new scan-HUD render function and a new bottom-status-bar render function are added to the panel layout; `BytewhifferApp` gains state for elapsed time (an `Instant` captured at scan start), a rolling sample for scan rate, a running "biggest top-level item" tracker updated in `drain_scan`, and a snapshot of the finished scan's counters (taken before `ActiveScan` is dropped) so the bottom bar's summary persists after completion.
- `openspec/specs/scan-status-bar/spec.md`: new capability spec (via this change's delta spec).
- `openspec/specs/disk-scanning/spec.md`: requirement amendment for the directories-scanned counter, via this change's delta spec.
- No new crate dependencies.
