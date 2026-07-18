## Why

The directory-walker engine is already reasonably fast (single-digit seconds
on typical drives, per manual benchmarking against WizTree: 9s vs 7.52s on a
C: drive, 5s vs ~3s on a D: drive), but WizTree's advantage comes from
reading the NTFS `$MFT` directly in one sequential pass instead of walking
the directory tree call-by-call, and that advantage is expected to widen on
larger/higher-file-count volumes than were benchmarked. `ScanEngine`,
`Availability`, and `ScanError::Unavailable` in `src/scanner/mod.rs` were
already shaped for a second engine to slot in — this change is that engine,
plus the opt-in UI flow (elevation is a real cost, so users choose it, they
don't get defaulted into it).

## What Changes

- Add an `MftEngine` `ScanEngine` implementation: one sequential read of the
  volume's `$MFT`, fixed-size records parsed in parallel (rayon), followed by
  a single in-memory bottom-up rollup pass (parent → children map, recursive
  size sums) to produce the same `Entry` tree the walker produces today.
  Requires raw volume access (`\\.\C:`-style handle) and administrator
  elevation; applies only to local NTFS volumes.
- Add a "Turbo" toggle to the toolbar with three visual states:
  - **Disabled/greyed out** — current target is not on an NTFS volume.
  - **Enabled, not yet elevated** — target is NTFS; clicking shows a
    WizTree-style warning dialog explaining that turbo mode needs
    administrator privileges, then triggers a UAC prompt.
  - **Warning/red** — the process is already elevated (turbo previously
    accepted) but the *current* target is on a non-NTFS volume; clicking or
    switching to such a target shows a dialog stating turbo mode does not
    work for this drive, and the engine falls back to the walker for that
    target.
- Elevation flow: accepting the UAC prompt relaunches the app with an
  elevated token and the current scan root passed through (clean slate —
  the new process starts fresh at that root, not a restored deep
  navigation/breadcrumb state). Declining UAC leaves the app unelevated and
  the Turbo toggle back in its promptable state.
- Once elevation succeeds, turbo is permanently toggled on for the rest of
  the session — no repeat UAC prompts for subsequent NTFS targets in the
  same run.
- `is_available` on `ScanEngine` starts actually returning
  `Availability::RequiresElevation` and `Availability::UnsupportedFilesystem`
  (currently unconstructed placeholders) to drive the three toggle states
  above.

## Capabilities

### New Capabilities
- `turbo-mode`: the Turbo toggle's UI states, the warning-dialog-then-UAC
  elevation flow, the post-elevation "permanently on" behavior, and the
  non-NTFS-after-elevation warning/fallback flow.

### Modified Capabilities
- `disk-scanning`: adds the `MftEngine` implementation of `ScanEngine`, and
  changes the `is_available` capability-check requirement so it actually
  distinguishes "not elevated" (`RequiresElevation`) from "elevated but
  wrong filesystem" (`UnsupportedFilesystem`) — today only
  `Availability::Available` is ever produced.

## Impact

- New module `src/scanner/mft.rs`: raw volume handle + elevation via the
  `windows` crate, `$MFT` record parsing (evaluate the `ntfs` crate's
  lower-level record/attribute parsing vs. hand-rolled — its
  `directory_index()` convenience isn't used here since this is a flat
  single-pass read, not a directory-by-directory walk).
  Free of `egui`, matching `scanner/`'s existing no-GUI-dependency rule, and
  unit-testable against synthetic `$MFT`-record byte layouts without real
  hardware or elevation.
- `app.rs`: Turbo toggle state and its three visual states, warning dialogs,
  and the elevated-relaunch trigger.
- `main.rs`: a new argument so a relaunched, already-elevated process knows
  to resume scanning the same root it was elevated from (same pattern as
  the existing hidden `--debug-screenshot*` flags).
- No change to default (unelevated) launch behavior — elevation stays
  opt-in per this proposal, not manifest-wide.
- Testability gap (same shape as the existing `--debug-perf` risk note):
  raw volume access, real elevation, and real-disk turbo-vs-walker speed
  comparisons can only be verified on real Windows hardware by a human at
  an elevated PowerShell session — not from this development environment.
