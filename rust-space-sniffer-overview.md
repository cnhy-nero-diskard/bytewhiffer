# Rust Space Sniffer — Project Overview

A modern-looking, fast, Windows-only disk space visualizer in Rust, inspired by
SpaceSniffer's treemap approach but solving its two biggest problems: it's slow,
and it looks like a 2010-era Win32 app.

> **What this document is:** a planning seed for spec-driven development, not
> exhaustive and not code. It exists to carry *decisions and their reasoning*
> out of the ideation conversation that produced it, so an implementation pass
> doesn't accidentally re-litigate or reverse them without knowing why they
> were made.

---

## 1. Platform & scope

- **Target:** Windows 10/11 only. No cross-platform ambitions for v1.
- **Scope:** a real, working prototype — not a comprehensive SpaceSniffer
  clone. See the feature list (§5) for exactly what's in vs. out.

## 2. Tech stack decisions (and why)

### UI: egui, via eframe
Chosen over two alternatives that were seriously considered:
- **Slint** — a native Rust UI toolkit purpose-built for polished-looking
  apps, but its declarative `.slint` markup is best suited to standard app
  chrome (forms, panels, lists). This app is fundamentally "draw thousands of
  custom-computed rectangles every frame with custom hit-testing," which is
  egui's immediate-mode sweet spot, not Slint's.
- **Tauri + web frontend** — rejected specifically because of the scanning
  strategy below: serializing a potentially huge file tree across an IPC
  boundary into a webview would undercut the entire point of a fast scanner.
  Native Rust keeps scan results and renderer in the same memory space.

egui's default look is fairly plain, but that's a styling problem, not a
ceiling — [Rerun](https://rerun.io) is a genuinely modern-looking, actively
used visualization tool built entirely on egui. Getting there just takes
deliberate styling (§4), not a different framework.

*Check the current stable `eframe`/`egui` version at implementation time
rather than trusting a hardcoded number here — this space moves fast (latest
was `0.35.0` as of mid-2026). Its MSRV was Rust 1.92; use `rustup` to keep the
toolchain current rather than an OS package manager's older Rust.*

### Native folder picker: `rfd`
Standard, well-maintained, uses the native Windows file/folder dialog.

### Scanning engine: two-phase strategy
1. **Phase 1 (build this first): parallel directory walker.** Multi-threaded
   recursive scan (`rayon` or `jwalk`). No admin rights needed, works on any
   filesystem/drive.
2. **Phase 2 (fast-follow, not a blocker): NTFS MFT direct-read "turbo
   mode."** Reads the volume's `$MFT` directly in one pass instead of walking
   folder-by-folder — the same trick WizTree uses for its near-instant scans.
   Needs admin elevation and only applies to local NTFS volumes; falls back
   to Phase 1 for non-NTFS drives, network shares, or a declined elevation
   prompt. Likely built on the `ntfs` crate (record parsing) plus the
   `windows` crate (raw volume handle, elevation).

**Why phased rather than MFT-first:** it's the single riskiest, most novel,
most Windows-specific piece of the whole project, and it can only be
validated on real Windows hardware with admin rights — nothing about it can
be tested in a sandboxed dev/CI environment. Building the walker first gets
the entire rest of the app (treemap, navigation, actions) working and
testable sooner. Ballpark expectation, not a measured benchmark: on a modern
NVMe SSD with a typical consumer file count, the walker should land
somewhere around single-digit-seconds to under a minute, versus MFT's
near-instant few seconds — a real but livable gap. On HDDs or drives with
very high file counts, expect the gap to stay much wider.

Architecturally, both engines should implement one common `ScanEngine`-style
interface so Phase 2 slots in later without reworking the UI layer.

### Core layout algorithm: squarified treemap
Use the **squarified treemap algorithm** (Bruls, Huizing & van Wijk, 1999) —
not naive slice-and-dice — since it keeps rectangles close to square, which
matters a lot for both readability and click targets. This is a pure
geometry function with no GUI dependency, and is easy to unit-test in
isolation (sum-of-areas conservation, no rect escaping its container, output
order matching input order, etc.).

## 3. Data model sketch

```
Entry {
  name, path,
  size: u64,       // recursive total for directories
  is_dir: bool,
  children: Vec<Entry>,  // empty for files, sorted largest-first after a scan
}
```

Background scan runs on its own thread; the UI polls shared atomic counters
for live "files scanned / bytes scanned so far" and repaints while the scan
is in flight. This is what gives the "map fills in live" feel SpaceSniffer is
liked for, rather than a blank screen until 100%.

## 4. Visual direction

- **Base:** dark theme, near-black/charcoal-navy background — not pure black
  (something in the `#0d1117`–`#0f1420` family reads as modern immediately;
  it's the same family GitHub dark mode, Linear, and Vercel use).
- **Block color:** deterministic per file type/extension — hash-derived hue,
  constrained to a fixed saturation/lightness band so the palette feels
  curated (Tailwind/Radix-scale cohesion) rather than random RGB noise.
- **Hierarchy:** communicate nesting depth via a subtle elevation/lightness
  shift, not clashing hues, so the eye reads structure without visual noise.
- **Accent:** one vivid accent color (electric blue, violet, or teal),
  reserved for hover state, the current breadcrumb, and selection — the one
  place the UI should feel "alive."
- Exact hex values to be finalized during implementation; this section fixes
  direction, not a locked palette.

## 5. Feature list

### MVP — build this first
- Treemap visualization: block size ∝ disk usage, nested folders inside folders
- Live scan updates: the map fills in and grows while the scan is still
  running, not just once it's finished
- Click-to-drill navigation: click a folder block to zoom into just its
  contents; breadcrumb or back button to zoom back out
- Basic actions: right-click a block → Delete / Open / Reveal in Explorer
- Deterministic, themed block coloring (§4)

### V2 / later — explicitly deferred
- Filtering by name, size, age, or other file properties
- Tagging/annotating items during a cleanup pass
- Exporting scan results (grouped summaries or flat file lists)
- MFT "turbo mode" fast path (§2, Phase 2)

### Explicitly out of scope (unless revisited later)
- A separate directory tree/list side panel — this stays a pure graphical
  map, like SpaceSniffer, unlike WinDirStat
- Any cross-platform support

## 6. Suggested module layout

```
src/
  main.rs        — entry point, eframe::run_native setup
  app.rs         — eframe::App impl: UI state, panel layout, background-scan
                    orchestration, navigation state (focus path/breadcrumb)
  scanner/
    mod.rs       — Entry tree type, shared ScanEngine trait, progress counters
    walker.rs    — Phase 1: parallel directory-walk engine
    mft.rs       — Phase 2: NTFS MFT-read engine (added later)
  treemap.rs     — pure squarified-treemap layout algorithm, GUI-independent
  theme.rs       — color palette + deterministic color-from-extension logic
  util.rs        — byte-size formatting and small helpers
```

Keeping `scanner/` and `treemap.rs` free of any egui dependency is
deliberate: both are pure logic and should be fully unit-testable without a
display, which matters a lot given how hard the rest of this is to test
outside real Windows hardware.

## 7. Known risks — de-risk these two first

1. **MFT parsing + raw volume access.** Windows/NTFS-specific, needs a real
   machine with admin rights; nothing about it can be validated in a
   sandboxed or CI environment.
2. **Treemap render interactivity.** Per-rectangle hit-testing, hover, and
   click-to-zoom at potentially thousands of visible rectangles.

Recommend small, standalone spikes for both before wiring up the full app —
everything else here (navigation chrome, filtering, actions) is comparatively
well-trodden.

## 8. Note on existing groundwork

Earlier in the conversation that produced this doc, a draft, unit-tested
implementation of the pure `scanner` and `treemap` (squarified layout)
modules was sketched in a sandbox as a feasibility check — not part of this
handoff by default, but it exists if it'd be useful as a starting point
rather than writing those two modules from scratch.
