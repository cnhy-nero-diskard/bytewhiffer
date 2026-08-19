## Why

Turbo cancellation cannot interrupt its full MFT read, and the current pipeline holds several large representations concurrently. Large NTFS volumes therefore risk slow cancellation and excessive peak memory precisely where Turbo is intended to help.

## What Changes

- Check cancellation between bounded raw-volume reads/extents and return cancellation as normal control flow.
- Replace whole-MFT and per-record temporary allocation patterns with bounded chunks/windows or another measured bounded pipeline.
- Preserve cross-record correctness for attribute-list dependencies while reducing transient copies.
- Define and document peak-memory components and enforce a measurable bound relative to configured chunk size plus retained parsed/tree state.
- Add deterministic cancellation tests and synthetic large-record benchmarks/measurements.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `disk-scanning`: Require bounded-interval cancellation throughout Turbo I/O and parsing.
- `scan-responsiveness`: Require Turbo peak raw-buffer/transient memory to be bounded and measured without introducing a completion-frame freeze.

## Impact

- Primary code: Windows MFT I/O/parsing pipeline in `src/scanner/mft.rs`, scan cancellation plumbing, and performance fixtures.
- Prerequisite: `harden-ntfs-reconstruction`, so streaming/chunking preserves validated attribute-list dependencies.
- No change to the authoritative accounting contract or visible tree shape.
- Issue coverage: GitHub issue #7 findings 7 and 8.
