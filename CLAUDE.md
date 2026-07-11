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
  treemap.rs     — pure squarified-treemap layout algorithm (Bruls/Huizing/van Wijk 1999)
  theme.rs       — color palette + deterministic hash-derived color-from-extension logic
  util.rs        — byte-size formatting
```

`scanner/` and `treemap.rs` are deliberately kept free of any `egui` dependency, so they stay fully unit-testable without a display — important since most of the render/interaction surface can only really be verified on real Windows hardware.

### ScanEngine abstraction

`scanner::ScanEngine` is the trait every scanning backend implements (`name`, `is_available`, `scan`). `WalkerEngine` (parallel `read_dir` recursion via rayon) is the only engine today, but the trait is shaped for a planned v2 NTFS `$MFT`-reading engine (needs elevation, only applies to local NTFS volumes) to slot in without reworking `app.rs`'s orchestration. `Availability` and `ScanError::Unavailable` exist for that future engine's fallback contract — don't remove them as "dead code" just because only `WalkerEngine::is_available` ever returns `Available` today.

A scan runs on its own thread. `ScanContext` carries a cancellation flag, atomic progress counters (`ScanProgress`), and an *optional, best-effort* `mpsc::Sender<ScanEvent>` for live discovery — the walker streams a `ScanEvent::Discovered` per entry so the UI can grow the map while scanning, but a future non-streaming engine (like the MFT reader) is allowed to emit late or not at all. The authoritative result is always the final `Entry` tree returned by `scan()`, never the event stream.

### Two parallel tree types

`scanner::Entry` is the engine's tree (final, authoritative, built after a scan completes). `app.rs::Node` is a separate UI-side tree with the same shape plus a `name -> index` map per node, built incrementally from `ScanEvent`s while a scan is in flight (so the map fills in live), then swapped wholesale for `Node::from_entry(&entry)` once the scan completes. Don't try to unify these — the incremental-insert requirement (`Node::insert`) and the index map are UI-only concerns that don't belong on the engine's tree type.

### Treemap layout

`treemap::squarify` is pure geometry: sizes in, one `Rect` per size out, in the same order — no knowledge of egui, files, or pixel/byte units. `app.rs::draw_children` is the adapter: it sorts a `Node`'s children largest-first per frame (the live tree arrives unsorted), feeds sizes to `squarify`, zips rects back to children by index, and recurses into directories that are still large enough on screen to be worth nesting (see `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` constants).

### Theming

Directories get a fixed muted-slate color (not hue-coded) so hue-coded files carry the color signal. File colors come from an FNV-1a hash of the lowercased extension mapped to a hue, with saturation/value fixed to a band (`BLOCK_SATURATION`/`BLOCK_VALUE`) so the palette reads as curated rather than random. Nesting depth is communicated via a capped lightness lift (`theme::depth_shift`), not new hues. The single accent color is reserved for hover, the active breadcrumb entry, and selection — nothing else.

## Development process

This repo uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) for spec-driven change management (`openspec/` directory: `changes/<name>/proposal.md`, `design.md`, `specs/*/spec.md`, `tasks.md`). Check `openspec/changes/` for in-progress work before starting something that might overlap.
