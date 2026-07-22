# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Bytewhiffer is a Windows-only disk space treemap visualizer (Rust, egui/eframe), inspired by SpaceSniffer but aiming to fix its two biggest pain points: speed and dated Win32-era looks. See [rust-space-sniffer-overview.md](rust-space-sniffer-overview.md) for the full design rationale (why egui over Slint/Tauri, the two-phase scanning strategy, visual direction) — read it before making architectural changes, since it records *why* decisions were made, not just what they are.

## Commands

```sh
cargo build --release
cargo run --release
cargo test                    # scanner + treemap + theme + util unit tests, no display needed
cargo test <name>             # run a single test by name (substring match)
```

**Windows build on this machine:** there is no MSVC Build Tools install (`link.exe` missing), so release builds use the GNU toolchain, and rustup's bundled mingw fails on the `windows-*` crates' raw-dylib step — a full WinLibs mingw-w64 must be on PATH first:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu build --release
```

Output lands at `target/release/bytewhiffer.exe`. Release builds set `windows_subsystem = "windows"`, so launching from PowerShell does not block the shell — use `Start-Process -Wait` or poll for output instead of waiting on the process.

**Verifying `#[cfg(windows)]` code from this (Linux) dev machine, before a real Windows round-trip:** `rustup target add x86_64-pc-windows-gnu` (one-time; downloads the std lib, no toolchain/linker needed) then `cargo check --target x86_64-pc-windows-gnu` (add `--tests` to include test code, `--release` for the release-only `windows_subsystem` path) type-checks the entire Windows-only surface — real `windows` crate, real signatures, real feature-gate errors — without invoking the linker. This caught a real bug during the turbo-mode change (`ShellExecuteExW`/`SHELLEXECUTEINFOW` need the `Win32_System_Registry` feature, since the struct carries an `HKEY` field) purely from `cargo check` output, no Windows machine needed. `cargo build --target x86_64-pc-windows-gnu` still fails here (`dlltool` permission error — this environment's bundled mingw isn't the full WinLibs one `windows-*` crates' raw-dylib step needs), so `check` is the ceiling; full linking still needs the real Windows+WinLibs round-trip above.

**Hidden debug flags** (see `src/main.rs`), for headless-ish verification when there's no way to interact with the GUI directly:

```
bytewhiffer --debug-screenshot <out.png> <scan-path>        # capture after scan completes
bytewhiffer --debug-screenshot-live <out.png> <scan-path>   # capture mid-scan, map partially filled
bytewhiffer --debug-screenshot-drill <out.png> <scan-path>  # capture drilled into the largest child dir
bytewhiffer --debug-perf                                    # headless tessellation bench: flat baseline vs soft-elevation, prints to stdout
```

`--debug-perf` is a GUI-subsystem binary so it has no console of its own — redirect its stdout to read it: `Start-Process ... -RedirectStandardOutput perf.txt`. It runs the de-risking spike from the `soft-elevation-theming` change (flat-fill vs shadow+gradient cost on synthetic dense trees) and is the tool to re-run if the card rendering ever changes.

## Architecture

### Module layout

```
src/
  main.rs        — entry point, eframe::run_native setup, hidden --debug-screenshot flag parsing
  app.rs         — eframe::App impl: UI state, panel layout, background-scan orchestration, navigation
  scanner/
    mod.rs       — Entry tree type, ScanEngine trait, ScanContext/ScanProgress/ScanEvent
    walker.rs    — parallel (rayon) directory-walk ScanEngine implementation
    mft.rs       — NTFS $MFT-reading "turbo" ScanEngine: a pure, cross-platform
                   record parser + bottom-up tree reconstruction (unit-tested
                   against synthetic byte layouts), with raw-volume access,
                   elevation/filesystem detection, and the UAC relaunch gated
                   behind #[cfg(windows)]
  treemap.rs     — pure squarified-treemap layout algorithm (Bruls/Huizing/van Wijk 1999)
  theme.rs       — color palette + deterministic hash-derived color-from-extension logic
  insights.rs    — pure, egui-free derived analytics over a scanned tree (legend, leaderboard, blizzard/junk flags); rendered by app.rs's Insights drawer
  util.rs        — byte-size formatting
```

`scanner/` and `treemap.rs` are deliberately kept free of any `egui` dependency, so they stay fully unit-testable without a display — important since most of the render/interaction surface can only really be verified on real Windows hardware.

### ScanEngine abstraction

`scanner::ScanEngine` is the trait every scanning backend implements (`name`, `is_available`, `scan`). Two engines implement it: `WalkerEngine` (parallel `read_dir` recursion via rayon) is the universal fallback that works on any drive without elevation, and `MftEngine` (`scanner/mft.rs`) reads a local NTFS volume's `$MFT` directly — faster on large volumes but requiring administrator elevation. `MftEngine::is_available` actually distinguishes `Available` (elevated + NTFS), `RequiresElevation` (NTFS, unelevated), and `UnsupportedFilesystem` (non-NTFS / non-Windows), re-checked on every scan-target change. `app.rs`'s `start_scan` is the single engine-selection point: an elevated process uses `MftEngine` on NTFS targets and falls back to `WalkerEngine` everywhere else. `ScanError::Unavailable(Availability)` is the fallback contract; the `NotApplicable` variant is still unconstructed — don't remove it as "dead code".

A scan runs on its own thread. `ScanContext` carries a cancellation flag, atomic progress counters (`ScanProgress`), and an *optional, best-effort* `mpsc::Sender<ScanEvent>` for live discovery — the walker streams a `ScanEvent::Discovered` per entry so the UI can grow the map while scanning, but a future non-streaming engine (like the MFT reader) is allowed to emit late or not at all. The authoritative result is always the final `Entry` tree returned by `scan()`, never the event stream.

### Two parallel tree types

`scanner::Entry` is the engine's tree (final, authoritative, built after a scan completes). `app.rs::Node` is a separate UI-side tree with the same shape plus a `name -> index` map per node, built incrementally from `ScanEvent`s while a scan is in flight (so the map fills in live), then swapped wholesale for `Node::from_entry(&entry)` once the scan completes. Don't try to unify these — the incremental-insert requirement (`Node::insert`) and the index map are UI-only concerns that don't belong on the engine's tree type.

### Treemap layout

`treemap::squarify` is pure geometry: sizes in, one `Rect` per size out, in the same order — no knowledge of egui, files, or pixel/byte units. `app.rs::draw_children` is the adapter: it sorts a `Node`'s children largest-first per frame (the live tree arrives unsorted), feeds sizes to `squarify`, zips rects back to children by index, and recurses into directories that are still large enough on screen to be worth nesting (see `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` constants).

### Theming

Directories get a fixed muted-slate color (not hue-coded) so hue-coded files carry the color signal. File colors come from an FNV-1a hash of the lowercased extension mapped to a hue, with saturation/value fixed to a band (`BLOCK_SATURATION`/`BLOCK_VALUE`) so the palette reads as curated rather than random. Nesting depth is communicated via a capped lightness lift (`theme::depth_shift`), not new hues. The single accent color is reserved for hover, the active breadcrumb entry, and selection — nothing else.

### Abstraction slider

The toolbar's abstraction slider (`app.rs`'s `self.abstraction`, `0.0` detail .. `1.0` abstract) doesn't touch `treemap::squarify` itself — it only tightens the same `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`-style gates `draw_children` already uses to decide whether to recurse into a directory, via `resolve_nest_gate`. At `abstraction == 0.0` the gates reduce to the original constants exactly, so the mechanism itself is additive, not a behavior change to what `resolve_nest_gate` computes at any given slider value; every `BytewhifferApp` constructor explicitly starts `abstraction` at `1.0` (max abstraction), so the app opens on the collapsed overview and the user drags toward `0.0` for full detail. A directory collapsed by the tightened gate still supports a hover-only, non-committal preview of its contents (no focus/breadcrumb change) — see the `2026-07-17-add-treemap-abstraction-mode` spec for why the mechanism reuses gate-tightening rather than a separate render path, and `mod abstraction_tests` in `app.rs` for the invariants that must hold as `abstraction` moves (gates only ever tighten, top-level block count is never hidden, only interior structure).

### Insights drawer

`src/insights.rs` computes derived analytics (extension legend, size-by-extension breakdown, biggest-entries leaderboard, small-file-blizzard and known-junk flags) over an `InsightNode` — a minimal borrowed view that both `app::Node` and `scanner::Entry` can produce, so the aggregation logic never depends on either concrete tree type and stays unit-testable without a display, mirroring `treemap.rs`/`scanner/`. `app.rs` renders the results in a collapsible left-side panel (`self.insights_open`, closed by default — the app stays "a pure graphical map" per `rust-space-sniffer-overview.md` §5 until the drawer is explicitly summoned) and wires leaderboard clicks into the existing `focus` navigation state. Recomputed whenever the focused node or tree revision changes; no new scan data or scanner changes involved.

### Turbo mode (NTFS `$MFT` engine)

`scanner/mft.rs` splits deliberately in two, and the split is the whole point. The **pure core** — boot-sector parsing, Update-Sequence-Array fixups, `$STANDARD_INFORMATION`/`$FILE_NAME`/`$DATA` attribute parsing, data-run decoding, and the bottom-up parent→children reconstruction into an `Entry` tree — is cross-platform and unit-tested against hand-built synthetic `$MFT` byte layouts (`mod tests`), so correctness (recursive sizes, largest-first sort, hard-link dedup, reparse/deleted/metadata exclusion) is verifiable with plain `cargo test` on any host. The **`#[cfg(windows)]` `platform` module** — raw `\\.\C:` volume read (via `std::fs` + `OpenOptionsExt` to keep the `windows`-crate surface small), `GetVolumeInformationW` filesystem detection, `TOKEN_ELEVATION` elevation check, and the `ShellExecuteExW` `"runas"` self-relaunch — cannot run in the Linux dev environment at all, and has an inert non-Windows stub so the crate still compiles and the toggle is drivable off-Windows (it just reports `UnsupportedFilesystem`). The `windows` crate is a `[target.'cfg(windows)'.dependencies]` entry, so `cargo test`/`cargo build` on Linux never compiles or downloads it. The `ntfs` crate was evaluated and rejected — it has no public in-memory record-parsing entry point, only volume-relative single-threaded reads, incompatible with the flat-buffer + parallel-parse design (see the change's `design.md`).

The UI half lives in `app.rs`: `turbo_elevated` (detected once at startup — the UAC relaunch produces an elevated process — and never persisted), `turbo_availability` (recomputed per scan-target change), and `turbo_state()` mapping those to the toggle's four states (disabled / promptable / active / warning-red). The promptable click path is warn-dialog → `mft::relaunch_elevated` → UAC → the elevated process relaunches with the hidden `--elevated-scan <path>` flag (parsed in `main.rs`, same pass-through pattern as `--debug-screenshot*`) and starts a clean scan at that root. `dead_code` in the parser is suppressed *only* on non-Windows non-test builds via `#![cfg_attr(not(any(windows, test)), allow(dead_code))]`, so detection stays active where the code actually compiles-in. Anything touching raw volumes, real elevation, or turbo-vs-walker speed can only be verified by a human on real Windows hardware in an elevated session (same closing-the-loop limitation as `--debug-perf`).

## Releasing

`scripts/release.ps1 -Bump patch|minor|major` (or `-Version X.Y.Z`; `scripts/release.sh patch|minor|major|X.Y.Z` is the WSL/bash equivalent, requires `perl` for the `Cargo.lock` sync) bumps `Cargo.toml`'s version, keeps `Cargo.lock`'s matching entry in sync, commits, and creates the `vX.Y.Z` tag — it does not push. Review the commit/tag, then `git push && git push origin vX.Y.Z`. Pushing the tag triggers `.github/workflows/release.yml`, which now verifies the tag matches `Cargo.toml`'s version before building (fails fast on mismatch) and then builds/publishes the GitHub Release with `bytewhiffer.exe` attached.

## Development process

This repo uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) for spec-driven change management (`openspec/` directory: `changes/<name>/proposal.md`, `design.md`, `specs/*/spec.md`, `tasks.md`). Check `openspec/changes/` for in-progress work before starting something that might overlap.
