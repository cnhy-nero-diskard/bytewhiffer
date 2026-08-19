## Why

Starting or relaunching a scan can detach the previous worker, accept stale results, and accumulate an unbounded live-event backlog. The app needs one explicit scan lifecycle so supersession, cancellation, assembly, target selection, and destructive-action availability all agree on which generation is authoritative.

## What Changes

- Introduce generation-based ownership for the active worker, live events, final result, and pending assembly.
- Cancel superseded scans, stop accepting their events, reap their workers asynchronously, and reject stale completion from UI state.
- Replace the unbounded discovery channel with bounded, non-blocking best-effort delivery while preserving the final tree as authoritative.
- Treat cancellation as normal control flow that never commits a cancelled generation or opens an error dialog.
- Disable Delete while a scan or authoritative assembly is active.
- Establish one requested scan target used consistently by Scan, Rescan, and Turbo elevation.
- Add orchestration and saturation regression tests covering supersession, panic recovery, reaping, stale results, delete availability, and target selection.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `disk-scanning`: Define single-generation ownership, supersession, cancellation, stale-result rejection, and worker-reaping behavior.
- `scan-responsiveness`: Bound best-effort live discovery memory without blocking scanning or weakening the authoritative final result.
- `file-actions`: Make Delete unavailable while scan or assembly state can overwrite the visible tree.
- `turbo-mode`: Use the current requested target as the sole elevation and scan root.

## Impact

- Primary code: `src/app.rs`, `src/scanner/mod.rs`, and scanner orchestration tests.
- Expected extraction: a focused scan-controller module may move lifecycle logic out of `app.rs` without otherwise reorganizing the UI.
- No persisted-data migration or public API break.
- Issue coverage: GitHub issue #7 findings 1, 2, 9, and 10.
