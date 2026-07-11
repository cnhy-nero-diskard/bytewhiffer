## Context

Today's toolbar (`app.rs::toolbar`) shows a spinner and a "files · bytes" line while a scan runs, and a bare total-bytes figure once it finishes; there is no bottom bar at all. The underlying `files_scanned`/`bytes_scanned` counters live on `ActiveScan.ctx.progress` (`scanner/mod.rs::ScanProgress`) and are discarded the moment `drain_scan` sets `self.scan = None` after a scan completes — so even today's modest live numbers don't survive to be shown afterward.

This surfaced from an `/opsx:explore` session that worked through toolbar/status-bar options against the actual code (not just visually): what a progress bar can honestly represent given the walker's architecture, which metrics are cheap vs. costly to add, and how a new bottom bar's content should relate to the toolbar's rather than duplicate it.

## Goals / Non-Goals

**Goals:**
- Give the toolbar a Rescan action and a live, informative view of an in-progress scan.
- Give the app a persistent bottom status bar that survives scan completion instead of losing information, and that offers a non-tooltip way to inspect hover state.
- Keep the split between "live, in-progress" (toolbar/HUD) and "ambient, at-rest" (bottom bar) information clean — no field is shown in both places at once.
- Add no new crate dependencies.

**Non-Goals:**
- A determinate (percentage) progress bar. Considered and rejected — see Decisions.
- Drive quick-picks, a recent-paths dropdown, copy-path, a quick-filter/search box, or a light/dark theme toggle. All were raised during exploration and set aside as either out of this change's scope or in tension with existing design decisions (V2's deferred filtering, the single dark palette identity).
- Any change to the `ScanEngine` trait's public shape beyond adding one counter field to `ScanProgress`.

## Decisions

**Progress bar is indeterminate, not a percentage.** The parallel walker (`walker.rs::scan_dir`) discovers the tree as it recurses; it has no way to know the total size or file count of a target ahead of time, so there is nothing honest for a percentage to be computed against for an arbitrary scan target. Two determinate alternatives were considered and rejected:
  - Comparing `bytes_scanned` against the target volume's total used space (via `GetDiskFreeSpaceExW`) only produces a meaningful percentage when the scan target *is* a drive root — for any subfolder scan (the common case) the volume total is unrelated to the target's size, so the bar would need a special-cased fallback anyway, and it pulls in a new `windows` crate dependency for a benefit that only applies to one scan shape.
  - A pre-count double-pass (walk once to total, then walk again for the real scan) would give a true percentage for any target, but doubles wall-clock scan time — directly undercutting Bytewhiffer's core "fast" pitch against SpaceSniffer, which is the whole reason the walker-first/MFT-later phasing exists in the project's own design doc.
  Instead, the HUD shows an animated/pulsing bar (motion only, no fill level tied to completion) alongside live counters that carry the actual information.

**The HUD row and the bottom bar never show the same fact at the same time.** The scan HUD (new row between the toolbar and breadcrumb, visible only while `self.scan.is_some()`) owns every *live* number: files/directories/bytes scanned, scan rate, elapsed time, and the biggest top-level item found so far. The bottom bar's summary slot shows a neutral "Scanning…" placeholder during that window and only populates with the finished totals once the scan completes. This was chosen over showing the same running totals in both places, which would just be visual noise repeated twice on screen.

**"Biggest item so far" is a shallow, incrementally-tracked max, not a tree walk.** A naive implementation might recompute the max over the whole live `Node` tree every frame, which grows with total node count. Instead: a file's exact size is known the instant its `ScanEvent::Discovered` arrives, so `drain_scan` can maintain a running max in `BytewhifferApp` state, updated once per event, with no re-traversal. "Top-level" means direct children of the scan root — deep single huge files are surfaced indirectly once their containing top-level directory's total overtakes the running max, which matches how the treemap itself is read (by top-level block size) rather than requiring a separate global-deepest-file scan.

**Scan-rate (MB/s) is computed from two timestamped samples, not a new atomic counter.** `bytes_scanned` already increases monotonically; `BytewhifferApp` keeps the last-seen `(Instant, u64)` sample and diffs against the current one each time `drain_scan` runs, throttled to roughly once a second so the number doesn't jitter between egui repaints (which happen every ~100ms while a scan streams). No engine-side change needed for this metric.

**Directories-scanned needs one new counter, mirroring the existing pattern exactly.** `ScanProgress` gains `dirs_scanned: AtomicU64`; the walker increments it in `scan_dir` right next to the existing `ctx.emit(ScanEvent::Discovered { is_dir: true, .. })` call (`walker.rs:80-84`), the same way `files_scanned`/`bytes_scanned` are incremented next to the file-branch emit a few lines below. This is additive to the `disk-scanning` spec's "Live progress reporting" requirement, not a breaking change to it — existing callers that only read files/bytes are unaffected.

**The post-scan snapshot is copied out before `ActiveScan` drops, not read from a channel after the fact.** `drain_scan` already has the one moment (`scan.ctx.progress` still alive, right before `self.scan = None`) where the final counters and elapsed time are known and available; `BytewhifferApp` gains plain fields (not atomics — no longer contended once the scan thread has joined) to hold that snapshot for the bottom bar to render indefinitely afterward.

## Risks / Trade-offs

- **[Risk] An indeterminate bar reads as less informative than a percentage to users used to Explorer/WizTree-style progress** → **Mitigation**: the live counters (especially scan rate and elapsed time) are deliberately more prominent than in today's toolbar specifically to compensate — the goal is "obviously alive and informative," not "looks like every other progress bar."
- **[Trade-off] "Biggest top-level item so far" can be misleading mid-scan** (a top-level directory whose true size will grow much larger later can temporarily look smaller than a directory that happened to finish scanning first) → accepted as an inherent property of any "so far" metric during a live, streaming scan; the label says "so far" specifically to set that expectation, and the final treemap layout is unaffected since it always reflects the authoritative completed tree.
- **[Trade-off] The elapsed-time/rate sampling adds a small amount of new per-frame state to `BytewhifferApp`** (a few plain fields, not atomics, since they're only ever touched from the UI thread inside `drain_scan`) — accepted as proportionate to the feature; no new synchronization primitives are needed.

## Open Questions

- Exact throttle interval for the scan-rate sample (this doc assumes ~1s) and the HUD pulse animation's speed/style are implementation-time tuning, expected to be adjusted against the running app rather than fixed here.
- Whether "biggest top-level item so far" should reset when the user changes focus/drills in in a way that's unrelated to an active scan is not applicable today (the metric is only shown while `self.scan.is_some()`, and focus/drilling requires a completed tree to navigate), but should be kept in mind if a future change makes navigation possible during an in-flight scan.
