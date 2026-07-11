## Why

Existing disk-space visualizers on Windows are either slow (recursive walkers with
no live feedback) or dated-looking (SpaceSniffer's 2010-era Win32 chrome).
Bytewhiffer is a fast, modern-looking, Windows-only treemap-based disk visualizer
that fixes both: a live-filling scan so the map grows while scanning is still in
progress, and a deliberately styled dark UI in the vein of GitHub dark / Linear /
Vercel rather than default toolkit chrome. `rust-space-sniffer-overview.md` is the
planning seed carrying the tech-stack and scope decisions (and their reasoning)
that this change turns into a real, working MVP prototype — not a full
SpaceSniffer clone (see `Capabilities` below for exact scope).

## What Changes

- New eframe/egui desktop app skeleton (`main.rs`, `app.rs`) with background-thread
  scan orchestration and navigation state (focus path / breadcrumb).
- `ScanEngine` trait introduced now (not a free function), with the existing
  root-level `scanner.rs` walker refactored to implement it — anticipating a v2
  NTFS `$MFT` "turbo mode" engine (out of scope for this change) so it can slot in
  later without reworking the UI layer. The trait bakes in three things the
  walker alone wouldn't need: a capability/fallback check
  (`is_available(target) -> Available | RequiresElevation | UnsupportedFilesystem | NotApplicable`),
  a `Result<Entry, ScanError>` return that distinguishes "partial success, some
  entries silently skipped" from "this engine categorically cannot run here, fall
  back," and a progress contract (monotonic done-ness + a final complete tree)
  that doesn't assume every engine can stream continuously-growing partial
  subtrees the way the walker does.
- Squarified treemap layout (existing root-level `treemap.rs`) wired into the UI
  as the rendering/hit-testing surface: block size proportional to disk usage,
  nested folders inside folders.
- Live scan rendering: the treemap fills in and grows while a scan is still
  running, driven by the walker's shared progress counters.
- Click-to-drill navigation: click a folder block to zoom into its contents;
  breadcrumb or back control to zoom back out.
- Right-click actions on a block: Delete, Open, Reveal in Explorer.
- Deterministic, themed block coloring: dark base theme, hash-derived hue per
  file extension within a constrained saturation/lightness band, depth
  communicated via elevation/lightness shift, one reserved accent color for
  hover/breadcrumb/selection.
- Module reorganization of the existing root-level `scanner.rs` / `treemap.rs`
  into the `src/` layout from the overview doc (§6): `scanner/` (`mod.rs`,
  `walker.rs`, with `mft.rs` deferred), `treemap.rs`, `theme.rs`, `util.rs`,
  `app.rs`, `main.rs`.
- Dev environment groundwork: Rust toolchain setup (this repo currently has none
  on either its WSL or Windows side), and a `.gitattributes` entry so the
  existing CRLF/LF drift between the committed modules and future edits stops
  recurring.

**Explicitly not in this change** (deferred to v2 per the overview doc, §5):
filtering by name/size/age, tagging/annotating, exporting scan results, and the
NTFS `$MFT` turbo-mode scan engine.

## Capabilities

### New Capabilities
- `disk-scanning`: the `ScanEngine` trait contract (capability check, cancellation,
  progress, result/error shape) and the parallel-directory-walker implementation
  that satisfies it for this change.
- `treemap-layout`: pure squarified-treemap geometry — sizes in, rectangles out,
  no GUI or filesystem dependency.
- `treemap-navigation`: rendering the live treemap, hit-testing/hover, click-to-drill
  zoom, and breadcrumb/back navigation chrome.
- `file-actions`: right-click context menu on a treemap block — Delete, Open,
  Reveal in Explorer — and their confirmation/error behavior.
- `theming`: dark base palette, deterministic extension-to-color mapping,
  depth-based elevation shift, and the single reserved accent color.

### Modified Capabilities
_None — no existing specs in this repo yet._

## Impact

- **New code**: `src/main.rs`, `src/app.rs`, `src/scanner/{mod,walker}.rs`,
  `src/treemap.rs`, `src/theme.rs`, `src/util.rs`, plus a new `Cargo.toml`
  declaring `eframe`/`egui`, `rfd`, and a directory-walk dependency.
- **Reorganized code**: root-level `scanner.rs` and `treemap.rs` move into `src/`
  and `scanner.rs` is split to introduce the `ScanEngine` trait; existing unit
  tests move with them and continue to run under plain `cargo test`.
- **Dev environment**: Rust toolchain (`rustup`) needs installing before any of
  this is buildable; most of the app (scanning, layout, rendering, interactivity,
  theming) can be built and visually/interactively verified under WSL via WSLg,
  which is already running in this environment. Reveal-in-Explorer and real
  delete/recycle-bin behavior are the only MVP pieces that need confirming on an
  actual Windows session.
- **Not impacted**: `openspec/specs/` has no pre-existing capabilities to modify;
  this is a greenfield spec set.
