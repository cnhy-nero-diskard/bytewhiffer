## Context

`scanner::ScanEngine` (`src/scanner/mod.rs`) already exists as a trait
boundary specifically for a second engine: `Availability` has
`RequiresElevation` and `UnsupportedFilesystem` variants that are
`#[allow(dead_code)]`-marked because nothing constructs them yet, and
`ScanError::Unavailable(Availability)` is similarly unconstructed. `app.rs`'s
orchestration layer only ever drives `WalkerEngine` today.

This design adds `MftEngine`, a second `ScanEngine` that reads a volume's
`$MFT` directly (the technique WizTree uses), and the toolbar/elevation flow
that makes it opt-in. Manual benchmarking (this repo, 2026-07-18) showed the
walker already lands within 1.5–2s of WizTree on drives that finish in
single-digit seconds either way — the case for `MftEngine` rests on that gap
widening on larger/higher-file-count volumes, not on the benchmarked drives
themselves.

## Goals / Non-Goals

**Goals:**
- A second `ScanEngine` implementation that reads NTFS's `$MFT` in one
  sequential I/O pass, parses records in parallel, and reconstructs the same
  `Entry` tree shape the walker produces, sorted largest-first.
- A Turbo toggle with three states (greyed out / promptable / warning-red)
  driven entirely by `Availability`, with a WizTree-style warn-then-UAC
  elevation flow.
- Elevation is opt-in per click and, once granted, holds for the rest of
  that elevated process's lifetime — no repeat UAC prompts.

**Non-Goals:**
- Preserving deep navigation state (breadcrumb, focus, abstraction slider
  position) across the UAC relaunch. Explicitly a clean slate: the new
  elevated process starts fresh at the same scan root.
- Persisting the "user accepted turbo" choice across separate app launches
  (a config file remembering elevation preference). Out of scope for this
  change — see Open Questions.
- Manifest-level (always-elevated) admin requests. Rejected earlier in favor
  of the opt-in flow this design describes.
- Network shares, non-local volumes, or any non-NTFS filesystem. These
  always report `UnsupportedFilesystem` and use the walker.

## Decisions

**Flat single-pass + parallel record parse + serial rollup, not a
directory-index-driven recursive walk.** The `ntfs` crate's
`NtfsFile::directory_index()` would let `MftEngine` recurse the same shape
as `WalkerEngine` (directory by directory), but that only removes
filesystem-layer call overhead — it doesn't reach WizTree-tier speed,
because it's still scattered reads following tree shape rather than one
sequential sweep. WizTree's actual advantage is reading the `$MFT` as one
linear I/O stream. Since fixed-size MFT records are independently
parseable, that stream can be chunked and parsed with rayon, then rolled up
bottom-up (parent file-reference → children map, sum sizes) in a single
cheap in-memory pass. The sequential read itself is the floor and is not
parallelizable; parsing is.

**Hand-roll the `$MFT` record/attribute parse; the `ntfs` crate can't do it.**
The original plan was to lean on the `ntfs` crate for the fiddly
`$STANDARD_INFORMATION`/`$FILE_NAME`/`$DATA` attribute parsing (resident vs.
non-resident, attribute headers) and hand-build only the flat-pass iteration and
parent-child reconstruction on top. That turned out to be impossible: the
`ntfs` crate has **no public entry point for parsing a record out of an
in-memory buffer** — its only record access is `Ntfs::file(fs, n)`, which reads
each record *through the volume's own `$MFT` data runs* via a single `&mut`
reader (`NtfsFile::new` is private). That is fundamentally incompatible with
this engine's whole reason to exist: one sequential read of the entire `$MFT`
into a buffer, then fixed-size records parsed **in parallel** with rayon. Using
the crate would force scattered, volume-relative, single-threaded reads — i.e.
abandoning the flat-buffer + parallel-parse decision above. So the minimal
subset of the FILE-record format actually needed here is hand-rolled in
`scanner::mft`: the FILE-record header, Update-Sequence-Array fixups, the three
attribute types, and data-run decoding (only to locate the `$MFT`'s own
fragments during the read). It is well-specified and small, and — importantly —
being a pure byte-in / tree-out function it is **fully unit-testable on the
Linux dev machine against synthetic record layouts**, which the crate's
volume-backed API would not have been. This deepens, rather than contradicts,
the testability note in Risks below. Decided during implementation
(2026-07-18); the `ntfs` crate is not a dependency.

**Elevation via self-relaunch (`ShellExecuteExW`, verb `"runas"`), not a
manifest-level `requireAdministrator`.** Keeps the default (unelevated)
launch path — including the existing `--debug-screenshot*` / `--debug-perf`
scriptable verification flags — unaffected. Only clicking Turbo pays the
UAC cost. The current scan root is passed to the relaunched process as a
CLI argument (same pattern as the existing hidden debug flags), and the old
process exits once the new elevated one spawns.

**`is_available` distinguishes not-elevated from wrong-filesystem, and the
orchestration layer maps that directly to toggle color — no new
`Availability` variants needed.** `MftEngine::is_available(target)`: checks
the target volume's filesystem first (not NTFS → `UnsupportedFilesystem`,
greys the toggle out); if NTFS, checks the process token for elevation (not
elevated → `RequiresElevation`, toggle is clickable and warns before UAC;
elevated → `Available`, toggle just runs turbo). Re-run on every scan-root
change so switching to a non-NTFS target after elevation correctly flips
the toggle to warning-red rather than staying green.

**Turbo-on state lives in-memory on the elevated process for its lifetime,
not on disk.** "Permanently toggled on after first UAC" is scoped to that
elevated process's session; a fresh launch starts unelevated and
unprompted again. Simpler, and avoids a new persisted-settings surface for
this change.

## Risks / Trade-offs

- **Live filesystem changes during a `$MFT` read produce a snapshot with
  eventual-consistency artifacts** (a file deleted/moved mid-scan may be
  stale or briefly double-represented). → Same accepted limitation WizTree
  has; not something Bytewhiffer can fully close either. Not a regression
  versus the walker's own mid-scan mutation exposure.
- **Hard links, reparse points/junctions, and deleted-but-unreclaimed MFT
  records can double-count size or produce ghost entries** if not
  explicitly handled (`hard_link_count()` and reparse/deletion flags need
  explicit checks; the crate does not dedupe or filter these for you). →
  Cross-check `MftEngine` output against `WalkerEngine` output for the same
  tree during development as a correctness oracle; explicit unit tests for
  each case using synthetic record layouts.
- **The UAC relaunch discards in-flight state** (deep navigation,
  abstraction slider). → Accepted per Non-Goals; if this proves jarring in
  practice, revisit passing more than just the scan root.
- **Raw volume access + a self-elevating relaunch can read as suspicious to
  antivirus/EDR software.** → No mitigation planned for this change; flag if
  it surfaces during real-hardware testing.
- **Nothing in `MftEngine`'s raw-volume-access, elevation, or real-disk-speed
  behavior can be exercised from this development environment** (WSL, no
  admin token, no raw NTFS access). → Parsing/reconstruction logic stays
  pure and unit-testable against synthetic `$MFT` byte layouts (mirroring
  how `scanner/`/`treemap.rs` stay engine-agnostic and display-free); the
  raw-access/elevation/speed parts are verified by a human on real Windows
  hardware via an elevated PowerShell session, the same closing-the-loop
  pattern the existing `--debug-perf` flag already relies on.

## Migration Plan

Purely additive: a new engine and a new toggle, both inert until a user
clicks Turbo. No data migration, no change to default launch behavior, no
feature flag needed beyond the toggle itself. Rollback is deleting the
toggle/engine wiring; nothing downstream depends on `MftEngine` existing.

## Open Questions

- Should "turbo accepted" persist across separate app launches (a small
  config file), so a user doesn't re-approve UAC every time they open
  Bytewhiffer on the same machine? Left as in-memory/per-launch for this
  change; revisit if that friction turns out to matter in practice.
- Exact Windows API for the elevation check backing `RequiresElevation`
  (e.g. checking the process token's elevation type) needs to be nailed
  down during implementation — noted here rather than in Decisions since
  it's a lookup, not a trade-off.
- **Deferred, not blocking this proposal:** cross-check `MftEngine` output
  against `WalkerEngine` output for the same real directory tree as a
  correctness oracle, and re-run the manual WizTree-vs-Bytewhiffer benchmark
  with turbo mode active against the walker-only numbers recorded above.
  Both need real Windows hardware and a human, same as tasks 8.1/8.2 in
  `tasks.md` (which now cover only the UI-flow and UAC-decline checks).
  Earmarked for the planned future debugging-framework expansion — that
  change should pick these two up as its first exercises rather than this
  one re-absorbing them.
