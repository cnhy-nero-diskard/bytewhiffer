## Why

Bytewhiffer currently presents logical file lengths as disk usage while Walker and Turbo can disagree on hard links, sparse files, and compressed files. An authoritative disk-space visualizer needs one explicit, engine-independent accounting contract.

## What Changes

- Adopt `UniqueAllocatedBytes` as the authoritative treemap and aggregate metric: physical allocation counted once per stable file identity within the scan root.
- Retain logical length as secondary metadata where useful, but do not mix it into allocated-byte totals.
- Define deterministic hard-link ownership so parallel Walker discovery and MFT name ordering cannot change aggregate attribution.
- Make Walker and Turbo obtain equivalent file identity and allocated-size information and produce equivalent totals.
- Update user-facing size wording to distinguish allocated usage from logical length.
- Add Windows fixtures for ordinary, sparse, compressed, and multiply-linked files, including cross-engine comparisons.

## Capabilities

### New Capabilities

- `disk-accounting`: Defines authoritative allocated-byte semantics, identity-based deduplication, deterministic hard-link attribution, and secondary logical-size metadata.

### Modified Capabilities

- `disk-scanning`: Require every scan engine to populate the shared accounting model and produce equivalent totals for the same snapshot.

## Impact

- Primary code: `src/scanner/mod.rs`, `src/scanner/walker.rs`, `src/scanner/mft.rs`, `src/app.rs`, and Windows integration fixtures.
- The `Entry`/UI node data model will need explicit accounted and logical-size fields or equivalent typed values.
- Depends on Windows APIs for stable file identity and allocated size in the Walker path.
- This is a visible semantics correction: totals may differ from prior releases, especially for sparse, compressed, and hard-linked files.
- Issue coverage: GitHub issue #7 finding 6.
