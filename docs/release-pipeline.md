# CI and Release Pipeline

Bytewhiffer CI and releases use Rust 1.97.0, with `rustfmt`, `clippy`, and the
`x86_64-pc-windows-msvc` target declared in `rust-toolchain.toml`. All hosted
quality and release builds explicitly target MSVC; the GNU target is not part
of the delivery pipeline.

## Required quality gate

The reusable `Quality Gate` workflow checks out the exact SHA supplied by its
caller and runs formatting, all-target clippy with warnings denied, debug and
release tests, an MSVC release build, and `cargo deny check`. Its final job
fails if any required job fails, is cancelled, or is skipped.

Run the same commands on a Windows machine with the MSVC Build Tools:

```powershell
$target = 'x86_64-pc-windows-msvc'
cargo fmt --all -- --check
cargo clippy --all-targets --target $target --locked -- -D warnings
cargo test --target $target --locked
cargo test --release --target $target --locked
cargo build --release --target $target --locked
cargo install cargo-deny --version 0.20.2 --locked
cargo deny --locked check
```

Every command that resolves dependencies passes `--locked` so CI fails on a
stale or inconsistent `Cargo.lock` instead of silently re-resolving it.

## Dependency-policy exceptions

`deny.toml` enforces advisories, licenses, banned dependencies, and sources;
duplicate dependency versions are reported as warnings rather than blocked,
since the GUI stack legitimately carries incompatible major versions. Do not
add broad ignores or blanket allowlists to bypass a finding. A
temporary exception must identify the exact crate and version, include a
rationale, and state the date on which it must be reviewed or removed.

## Releases

Tag pushes first verify that `vX.Y.Z` matches `Cargo.toml`, then run the same
quality gate for the exact tag commit. Only a successful gate may build and
publish `bytewhiffer.exe` with its matching SHA-256 checksum.

Code signing, SBOM generation, and provenance attestations are explicitly
deferred follow-ups. This pipeline does not imply that any of them is present.
