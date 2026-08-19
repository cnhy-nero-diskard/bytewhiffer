## Why

Turbo currently ignores NTFS attribute-list extension records and can turn malformed or incomplete MFT reconstruction into plausible-looking success. Raw filesystem metadata must either produce a complete, trustworthy tree or an explicit fallback/error outcome.

## What Changes

- Parse `$ATTRIBUTE_LIST` entries and safely associate referenced extension records with their base record.
- Merge relevant names, unnamed data, flags, and MFT data-run metadata across validated extension chains.
- Detect cycles, duplicates, stale references, malformed chains, and missing required attributes.
- Replace record-0-only and target-not-found success paths with typed incomplete/corrupt outcomes.
- Automatically fall back to Walker for Turbo unavailability or untrustworthy reconstruction, while keeping unreadable targets user-visible and cancellation non-error.
- Harden all parser offsets, widths, shifts, arithmetic, conversions, and allocation bounds.
- Add synthetic attribute-list coverage plus fuzz/property harnesses for boot-sector, fixup, record, attribute, and data-run parsing.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `disk-scanning`: Require complete extension-record reconstruction, checked hostile-input handling, explicit failure classification, and trustworthy fallback behavior.
- `turbo-mode`: Fall back with an appropriate diagnostic when NTFS metadata is incomplete or corrupt rather than displaying partial success.

## Impact

- Primary code: `src/scanner/mft.rs`, `src/scanner/mod.rs`, Turbo selection/error handling in `src/app.rs`, and parser test/fuzz fixtures.
- Expected extraction: split MFT boot, record, attributes, runs, reconstruction, and Windows I/O responsibilities as behavior is hardened.
- Prerequisite: `define-disk-accounting-contract`, because extension merging must use the selected allocated-size semantics.
- Real raw-volume validation remains Windows-hardware work in addition to pure synthetic coverage.
- Issue coverage: GitHub issue #7 findings 3, 4, and 5.
