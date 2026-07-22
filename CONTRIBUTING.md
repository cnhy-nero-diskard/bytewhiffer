# Contributing

Bytewhiffer is a small Windows-only Rust project. This doc covers the basics
for sending a patch; see [CLAUDE.md](CLAUDE.md) for build quirks and
architecture notes.

## Getting started

```sh
cargo build --release
cargo test
```

`cargo test` runs the scanner/treemap/theme/util unit tests and needs no
display or Windows machine — most of the logic is deliberately kept free of
`egui` and platform-specific code so it's testable anywhere. See
[CLAUDE.md](CLAUDE.md) for the Windows release-build recipe (GNU toolchain +
WinLibs mingw) and the hidden `--debug-screenshot*`/`--debug-perf` flags used
to verify UI/rendering changes without interacting with the GUI directly.

## Before you start

Non-trivial changes go through [OpenSpec](https://github.com/Fission-AI/OpenSpec)
(`openspec/changes/`). Check that directory for in-progress work that might
overlap with what you're planning before starting.

## Pull requests

- Keep `scanner/` and `treemap.rs` free of `egui` dependencies — they must
  stay unit-testable without a display.
- Run `cargo test` and, where relevant, `cargo check --target
  x86_64-pc-windows-gnu` (see CLAUDE.md) before opening a PR.
- CI (`.github/workflows/ci.yml`) runs `cargo test --release` and `cargo build
  --release` on Windows for every PR.

## Releasing

Maintainers only: see the "Releasing" section in [CLAUDE.md](CLAUDE.md).
