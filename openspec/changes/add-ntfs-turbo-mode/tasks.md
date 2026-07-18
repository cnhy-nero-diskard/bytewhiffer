## 1. Dependencies and module scaffolding

- [x] 1.1 Add the `ntfs` crate (record/attribute parsing) and any additional
      `windows` crate features needed for raw volume handles and process
      token/elevation queries
      <!-- The `ntfs` crate was evaluated and dropped: it has no public
      in-memory record-parsing entry point, only volume-relative single-threaded
      reads, incompatible with the flat-buffer + parallel-parse design. The
      record parse is hand-rolled instead (see design.md). Added the `windows`
      crate as a `[target.'cfg(windows)'.dependencies]` entry so Linux
      builds/tests never compile it. -->
- [x] 1.2 Create `src/scanner/mft.rs`, free of `egui`, matching the
      no-GUI-dependency rule the rest of `scanner/` follows

## 2. $MFT record parsing (pure, unit-testable without hardware)

- [x] 2.1 Parse a raw `$MFT` record buffer into `$STANDARD_INFORMATION`,
      `$FILE_NAME`, and `$DATA` attributes using the `ntfs` crate's
      record/attribute-level APIs (not its directory-traversal APIs)
      <!-- Hand-rolled `parse_record` per the revised design decision. -->
- [x] 2.2 Handle resident vs. non-resident `$DATA` attributes when
      determining a file's size
- [x] 2.3 Detect hard-linked records (`hard_link_count()` > 1) and resolve to
      a single counted entry
- [x] 2.4 Detect reparse points/junctions and deleted-but-unreclaimed records
      via attribute flags, and exclude/skip them per spec
- [x] 2.5 Unit test each of the above against synthetic, hand-built
      `$MFT`-record byte layouts (no real volume or elevation required)

## 3. Flat-pass read, parallel parse, and tree reconstruction

- [x] 3.1 Implement the sequential single-pass read of the `$MFT` from a raw
      volume handle
      <!-- Windows-only `read_mft`; cannot be executed from the Linux dev env. -->
- [x] 3.2 Chunk the read buffer and parse records in parallel with rayon
- [x] 3.3 Build the parent-file-reference → children map and compute
      recursive directory sizes in a single bottom-up rollup pass
- [x] 3.4 Sort children largest-first at every level, matching the walker's
      existing output contract
- [x] 3.5 Wire cancellation (`ScanContext::cancel`) and progress counters
      (`ScanProgress`) through the read/parse/rollup phases
- [x] 3.6 Unit test recursive-size and largest-first-sort invariants against
      synthetic record sets (same test shape as the existing walker tests)

## 4. Capability check (`Availability`)

- [x] 4.1 Implement filesystem-type detection for a scan target's volume
      (NTFS vs. not)
      <!-- Windows `GetVolumeInformationW`; non-Windows stub returns false. -->
- [x] 4.2 Implement elevation detection for the current process (elevated
      token vs. not)
      <!-- Windows token `TOKEN_ELEVATION` check; non-Windows stub returns false. -->
- [x] 4.3 Implement `MftEngine::is_available` combining both checks into
      `Available` / `RequiresElevation` / `UnsupportedFilesystem` per the
      `disk-scanning` delta spec
- [x] 4.4 Unit test the three-way branching logic with mocked/stubbed
      filesystem-type and elevation inputs
      <!-- Pure `resolve_availability` + `availability_branches_three_ways` test. -->

## 5. Elevation relaunch

- [x] 5.1 Implement the elevated relaunch via `ShellExecuteExW` with verb
      `"runas"`, passing the current scan root as a CLI argument
      <!-- Windows-only; cannot be *run* from the Linux dev env, but the whole
      #[cfg(windows)] platform module (this included) now cross-compiles
      cleanly under `cargo check --target x86_64-pc-windows-gnu` — real
      windows-0.62.2 signatures, real feature gates, no linker needed. This
      caught and fixed one real bug: SHELLEXECUTEINFOW carries an HKEY field,
      so windows-rs gates the whole ShellExecuteExW/SHELLEXECUTEINFOW pair
      behind the unrelated-sounding `Win32_System_Registry` feature. See
      CLAUDE.md's "Verifying #[cfg(windows)] code" note. -->
- [x] 5.2 Add CLI argument parsing in `main.rs` for the relaunched process to
      pick up and resume scanning that root (same pattern as the existing
      hidden `--debug-screenshot*` flags)
      <!-- `--elevated-scan <path>` → `BytewhifferApp::with_elevated_scan`. -->
- [x] 5.3 Handle UAC decline: leave the original process running unelevated
      with no crash or dangling state
      <!-- ERROR_CANCELLED (1223) maps to Ok(false); the app keeps running. -->

## 6. Turbo toggle UI and dialogs

- [x] 6.1 Add Turbo toggle state to `app.rs`, driven by `MftEngine::is_available`
      for the current scan target
- [x] 6.2 Render the three visual states: disabled/greyed out
      (`UnsupportedFilesystem`), promptable (`RequiresElevation`), active
      (`Available`)
      <!-- Plus a fourth warning-red state for elevated + non-NTFS (task 6.4). -->
- [x] 6.3 Implement the warning dialog shown before triggering UAC, gated on
      user confirmation
- [x] 6.4 Implement the "turbo does not work for this drive" warning dialog
      and red toggle state for an already-elevated process whose target is
      non-NTFS
- [x] 6.5 Track "elevated this process lifetime" in-memory so turbo applies
      automatically to later NTFS targets without re-prompting

## 7. Orchestration wiring

- [x] 7.1 Wire engine selection in `app.rs` so an elevated process uses
      `MftEngine` for NTFS targets and falls back to `WalkerEngine` for
      non-NTFS targets, without special-casing engine choice outside this
      one selection point
- [x] 7.2 Confirm the fallback path produces identical `Entry`-tree shape and
      downstream behavior (treemap layout, insights) regardless of which
      engine produced it
      <!-- Both engines return the same `Entry` type; the walker fallback path
      was exercised end-to-end on Linux (screenshot: toggle greyed, engine
      "walker", tree renders normally). MFT-produced trees are verified for
      shape/correctness by the pure reconstruction tests. -->

## 8. Real-hardware verification (requires a Windows admin session)

> These tasks cannot be performed from the Linux/WSL dev environment (no raw
> NTFS access, no admin token, no real-disk timing). They must be run by a
> human on real Windows hardware from an elevated PowerShell session — the same
> closing-the-loop limitation the existing `--debug-perf` flag has.

- [ ] 8.1 Cross-check `MftEngine` output against `WalkerEngine` output for
      the same real directory tree as a correctness oracle
- [ ] 8.2 Re-run the manual WizTree-vs-Bytewhiffer benchmark with turbo mode
      active and compare against the walker-only numbers already recorded
      in the proposal
- [ ] 8.3 Manually verify the full click-to-turbo flow: greyed-out state on
      a non-NTFS target, warning dialog, UAC prompt, relaunch, active state,
      and the post-elevation non-NTFS warning state
- [ ] 8.4 Test UAC decline path end-to-end

## 9. Docs

- [x] 9.1 Update `CLAUDE.md` and `README.md`'s feature list and architecture
      sections to describe the MFT engine and Turbo toggle
