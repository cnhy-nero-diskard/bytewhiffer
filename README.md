# Bytewhiffer

A fast, modern-looking disk space treemap visualizer for Windows, built with
Rust and [egui](https://github.com/emilk/egui)/[eframe](https://github.com/emilk/egui/tree/master/crates/eframe).

Inspired by SpaceSniffer's treemap approach, aiming to fix its two biggest
pain points: it's slow, and it looks like a 2010-era Win32 app.

## Features (MVP)

- Treemap visualization — block size proportional to disk usage, folders
  nested inside folders
- Live scan updates — the map fills in and grows while the background scan is
  still running
- Click-to-drill navigation — click a folder block to zoom into its contents;
  breadcrumb/back to zoom out
- Right-click actions — Delete / Open / Reveal in Explorer
- Deterministic, themed block coloring by file type
- Insights drawer — extension color legend, size breakdown, biggest
  files/folders leaderboard, small-file-blizzard and known-junk flags
- Abstraction slider — collapse the map to fewer, bigger top-level blocks;
  hover a collapsed block for a non-committal preview of its contents

See [rust-space-sniffer-overview.md](rust-space-sniffer-overview.md) for the
full design rationale, tech stack decisions, and planned V2 work (filtering,
tagging, export, NTFS MFT "turbo mode" scanning).

## Requirements

- Windows 10/11 (no cross-platform support planned for v1)
- Rust (stable toolchain, kept current via `rustup`)

## Building & running

### Quick start

```sh
cargo build --release
cargo run --release
```

### Windows build with MinGW

On Windows without MSVC Build Tools installed, use the GNU toolchain:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin;$env:PATH"
cargo +stable-x86_64-pc-windows-gnu build --release
```

Output: `target/release/bytewhiffer.exe`

**Note:** The release build is compiled with `windows_subsystem = "windows"`, so launching from PowerShell does not block the shell. Use `Start-Process -Wait` or poll for output.

## Testing

```sh
cargo test                    # all tests (scanner, treemap, theme, util)
cargo test <name>             # single test by name (substring match)
```

Tests run without a display and exercise the core logic in isolation.

## Debug flags

Hidden flags for headless verification and profiling:

```sh
bytewhiffer --debug-screenshot <out.png> <scan-path>
bytewhiffer --debug-screenshot-live <out.png> <scan-path>
bytewhiffer --debug-screenshot-drill <out.png> <scan-path>
bytewhiffer --debug-perf
```

- `--debug-screenshot` — capture after scan completes
- `--debug-screenshot-live` — capture mid-scan with partial map
- `--debug-screenshot-drill` — capture drilled into the largest subdirectory
- `--debug-perf` — headless tessellation benchmark (flat baseline vs soft-elevation); redirect stdout: `Start-Process bytewhiffer --debug-perf -RedirectStandardOutput perf.txt`

## Architecture

### Module layout

```
src/
  main.rs        — entry point, eframe::run_native setup, debug flag parsing
  app.rs         — eframe::App impl: UI state, panel layout, background-scan
                   orchestration, navigation
  scanner/
    mod.rs       — Entry tree type, ScanEngine trait, ScanContext, progress counters
    walker.rs    — parallel (rayon) directory-walk ScanEngine implementation
  treemap.rs     — pure squarified-treemap layout algorithm (Bruls/Huizing/van Wijk 1999)
  theme.rs       — color palette + deterministic hash-derived color-from-extension logic
  insights.rs    — pure, egui-free derived analytics (legend, leaderboard, blizzard/junk flags)
  util.rs        — byte-size formatting
```

`scanner/` and `treemap.rs` are deliberately free of any `egui` dependency, staying fully unit-testable without a display.

### ScanEngine abstraction

Every scanning backend implements the `ScanEngine` trait: `name()`, `is_available()`, and `scan()`. Currently only `WalkerEngine` (parallel `read_dir` recursion via rayon) exists, but the trait is designed for a v2 NTFS MFT-reading engine (requires elevation, local NTFS only). Scans run on a background thread. `ScanContext` carries a cancellation flag, atomic `ScanProgress` counters, and an optional `mpsc::Sender<ScanEvent>` for live discovery — the walker streams `ScanEvent::Discovered` per entry so the UI grows the map while scanning. The authoritative result is always the final `Entry` tree returned by `scan()`, never the event stream.

### Two parallel tree types

`scanner::Entry` is the engine's tree (final, authoritative, built after a scan completes). `app.rs::Node` is a separate UI-side tree with the same shape plus a `name -> index` map per node, built incrementally from `ScanEvent`s while scanning (so the map fills in live), then swapped wholesale via `Node::from_entry(&entry)` once the scan finishes. These are not unified — incremental insertion (`Node::insert`) and the index map are UI-only concerns.

### Treemap layout

`treemap::squarify` is pure geometry: sizes in, one `Rect` per size out, in order — no knowledge of egui, files, or units. `app.rs::draw_children` adapts: it sorts a `Node`'s children largest-first per frame, feeds sizes to `squarify`, zips rects back to children by index, and recurses into directories large enough on screen (see `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` constants).

### Theming

Directories are fixed muted-slate (not hue-coded) so hue-coded files carry the signal. File colors come from an FNV-1a hash of the lowercased extension, mapped to hue, with fixed saturation/value (see `BLOCK_SATURATION`/`BLOCK_VALUE`) for a curated look. Nesting depth uses capped lightness lift (see `theme::depth_shift`), not new hues. Accent color (hover, breadcrumb, selection) is reserved and never reused.

### Abstraction slider

A toolbar slider (`0.0` detail .. `1.0` abstract) tightens the same pixel-size gates `draw_children` already uses to decide whether to recurse into a directory (`resolve_nest_gate` in `app.rs`) — fewer, bigger top-level blocks as it moves toward `1.0`. `squarify` itself is untouched; only how often it's invoked recursively changes. Hovering a collapsed directory block shows a temporary, non-committal preview of its contents without changing focus/breadcrumb — moving the pointer away discards it, and clicking still drills down exactly as before.

### Insights drawer

A collapsible left-side panel (toolbar toggle, closed by default) presenting derived analytics over the currently focused subtree — extension color legend, size-by-extension breakdown, biggest-entries leaderboard, small-file-blizzard and known-junk flags. All of it is computed in `insights.rs` from the tree a scan already produced, with no new scanning cost; clicking a leaderboard entry focuses the treemap on that path via the existing navigation state.

## Development

This repo uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) for
spec-driven change management — see the `openspec/` directory for active and
archived change proposals.
