## 1. Prerequisite and Module Boundaries

- [ ] 1.1 Confirm `define-disk-accounting-contract` is present in the implementation base and stop if its entry/size contract is unavailable.
- [ ] 1.2 Extract focused egui-free MFT boot, record/fixup, attribute, run, reconstruction, and Windows-I/O modules while preserving current passing behavior.

## 2. Checked Parser Model

- [ ] 2.1 Replace ambiguous parser `Option`/empty outcomes with typed absence, malformed, overflow, truncation, and resource-limit results.
- [ ] 2.2 Add checked shift, offset, length, cluster/LCN, signed-delta, multiplication, and `usize` conversion helpers with explicit format limits.
- [ ] 2.3 Require successful fixups before trusting every FILE record, including record 0, and distinguish skippable record corruption from required-metadata failure.

## 3. Attribute-List Resolution

- [ ] 3.1 Parse bounded `$ATTRIBUTE_LIST` entries including attribute type/name, lowest VCN, referenced record/sequence, and attribute identity.
- [ ] 3.2 Index base and extension records and resolve validated chains with cycle, duplicate, stale-reference, and depth/count guards.
- [ ] 3.3 Merge supported `$FILE_NAME`, unnamed `$DATA`, and reparse/flag facts exactly once under the shared accounting contract.
- [ ] 3.4 Implement iterative `$MFT::$DATA` extent discovery across addressable extension records and reject incomplete VCN/reference coverage.

## 4. Trustworthy Outcomes and Fallback

- [ ] 4.1 Add typed `IncompleteMft`, `CorruptFilesystem`, `TargetNotFound`, and `Cancelled` outcomes alongside availability and unreadable-root failures.
- [ ] 4.2 Remove record-0-only and empty-target successful fallbacks from the Windows MFT path.
- [ ] 4.3 Route recoverable Turbo trust failures to Walker with engine/diagnostic state, while keeping cancellation non-error and dual-engine unreadability user-visible.

## 5. Parser and Reconstruction Coverage

- [ ] 5.1 Add synthetic tests for data in one/multiple extension records, name/flag merging, named-stream exclusion, and fragmented MFT metadata.
- [ ] 5.2 Add tests for missing/stale references, cycles, duplicates, truncated lists/attributes, invalid geometry/run widths, overflow boundaries, stale parents, reparse combinations, and target-not-found fallback.
- [ ] 5.3 Add `cargo-fuzz` or equivalent pure-parser harnesses for boot parsing, fixups, record/attribute parsing, attribute lists, and data runs with bounded-allocation invariants.

## 6. Verification

- [ ] 6.1 Run formatting, clippy with warnings denied, debug/release tests, focused corpus tests, and a documented fuzz smoke run.
- [ ] 6.2 Compare Turbo and Walker on real elevated Windows NTFS fixtures covering nested targets and extension/fragmentation cases where available; record any hardware coverage gaps explicitly.

