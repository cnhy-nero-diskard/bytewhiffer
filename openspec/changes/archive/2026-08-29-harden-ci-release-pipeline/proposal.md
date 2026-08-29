## Why

Current CI can silently skip Test and Build for some main-branch commits, and releases can publish an executable without running the full quality gate or emitting integrity metadata. Validation and publication must fail closed and be reproducible.

## What Changes

- Replace skip-on-main gate semantics with explicit success or failure; required validation must never appear successful through skipped jobs.
- Pin the Rust toolchain and use explicit intended Windows targets consistently.
- Add formatting, clippy-with-warnings-denied, debug tests, release tests, and release builds to the required CI gate.
- Add an explicit dependency vulnerability/license policy with documented exception handling.
- Make release publication run or depend on the trusted quality gate for the exact commit being released.
- Emit SHA-256 checksums and pin release-critical third-party actions to immutable commit SHAs.
- Preserve the existing tag/version guard and release scripts; track code signing and richer SBOM/provenance as later enhancements unless configured now.

## Capabilities

### New Capabilities

- `delivery-pipeline`: Defines fail-closed CI, reproducible toolchains/targets, release quality gates, immutable automation dependencies, and artifact integrity metadata.

### Modified Capabilities

- None.

## Impact

- Primary files: `.github/workflows/ci.yml`, `.github/workflows/release.yml`, a new `rust-toolchain.toml`, and dependency-policy configuration.
- Independent of product-code proposals and suitable for parallel implementation.
- CI duration may increase because debug/release validation becomes explicit; caching should offset repeated compilation.
- Issue coverage: GitHub issue #7 findings 18 and 19.
