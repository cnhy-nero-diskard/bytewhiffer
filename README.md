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

See [rust-space-sniffer-overview.md](rust-space-sniffer-overview.md) for the
full design rationale, tech stack decisions, and planned V2 work (filtering,
tagging, export, NTFS MFT "turbo mode" scanning).

## Requirements

- Windows 10/11 (no cross-platform support planned for v1)
- Rust (stable toolchain, kept current via `rustup`)

## Building & running

```sh
cargo build --release
cargo run --release
```

## Project layout

```
src/
  main.rs        — entry point, eframe::run_native setup
  app.rs         — eframe::App impl: UI state, panel layout, background-scan
                   orchestration, navigation state
  scanner/
    mod.rs       — Entry tree type, shared ScanEngine trait, progress counters
    walker.rs    — parallel directory-walk scan engine
  treemap.rs     — pure squarified-treemap layout algorithm, GUI-independent
  theme.rs       — color palette + deterministic color-from-extension logic
  util.rs        — byte-size formatting and small helpers
```

`scanner/` and `treemap.rs` are kept free of any `egui` dependency so they
stay fully unit-testable without a display.

## Development

This repo uses [OpenSpec](https://github.com/Fission-AI/OpenSpec) for
spec-driven change management — see the `openspec/` directory for active and
archived change proposals.
