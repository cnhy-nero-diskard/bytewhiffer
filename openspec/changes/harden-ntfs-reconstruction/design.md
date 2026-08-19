## Context

The current parser reduces each FILE record independently, records only whether it is a base record, and discards extension records during reconstruction. It also uses `Option`/empty collections for both absence and malformed input. Windows MFT reading trusts record 0 fixups, falls back to record 0 when runs are absent, and treats a missing target subtree as an empty successful directory.

## Goals / Non-Goals

**Goals:**

- Resolve supported attributes across validated `$ATTRIBUTE_LIST` chains.
- Bootstrap fragmented/attribute-list-backed `$MFT::$DATA` safely.
- Distinguish absent, malformed, incomplete, cancelled, and unreadable outcomes.
- Fall back to Walker whenever Turbo cannot produce a trustworthy complete result.
- Make arbitrary parser input non-panicking and allocation-bounded.

**Non-Goals:**

- Implement every NTFS attribute or alternate data stream.
- Recover corrupted filesystems or bypass Windows access controls.
- Reduce peak MFT memory; that follows in `bound-turbo-resource-usage`.

## Decisions

**Split parsing from resolution.** Parsed records retain base-reference identity, sequence numbers, attribute-list entries, and relevant raw attribute facts. A second resolver indexes records, validates references and sequence numbers, follows each base record's extension graph with visited/duplicate guards, and emits one merged logical record.

**Use typed parse errors.** Pure parsers return `Result` values that distinguish truncation, invalid widths, overflow, impossible geometry, malformed attribute layout, and resource-limit violations. Ordinary absence remains distinct. Record-level corruption may skip a non-required record with diagnostics; corruption in required MFT metadata or the requested subtree makes the Turbo result incomplete.

**Bootstrap `$MFT` extents iteratively.** Start with record 0's validated base data runs and attribute-list references. Use known extents to address reachable MFT extension records, validate them, merge newly discovered `$MFT::$DATA` runs, and repeat until no new run/reference appears. An unreachable required reference, cycle, overlap, or inconsistent VCN coverage returns `IncompleteMft` rather than record-0-only success.

**Classify scan outcomes centrally.** `Unavailable`, `IncompleteMft`, `CorruptFilesystem`, and `TargetNotFound` permit Walker fallback with a non-fatal diagnostic. `RootUnreadable` remains user-visible when Walker cannot establish a trustworthy result. `Cancelled` is control flow and never triggers fallback or an error dialog.

**Apply checked arithmetic before slicing or allocation.** Shift widths, attribute offsets/lengths, UTF-16 lengths, run widths/deltas, LCN arithmetic, cluster multiplication, and `usize` conversions are checked against format and configured limits. `apply_fixups` must succeed before any record is trusted.

**Extract cohesive MFT modules while changing behavior.** Boot geometry, fixups/records, attributes/lists, data runs, reconstruction, and Windows I/O become focused egui-free modules. This is incremental extraction, not an unrelated rewrite.

## Risks / Trade-offs

- **Strict validation may reject volumes previously shown partially** → Automatically run Walker and preserve a diagnostic explaining why Turbo was not authoritative.
- **Attribute-list resolution increases parser state** → Keep raw facts compact and cap list entries, chain depth, and referenced-record counts.
- **Synthetic fixtures may miss real layouts** → Add a reusable corpus and require Windows hardware comparison before claiming raw-volume verification.
- **Fallback can be slower** → Correctness takes priority; surface the active engine and reason.

## Migration Plan

Introduce typed parser results and modules behind existing `MftEngine`, add the resolver and tests, then replace record-0 and empty-subtree fallbacks. Wire typed outcomes into engine selection only after Walker fallback tests pass. Rollback is code-only.

## Open Questions

None blocking. Corpus expansion should continue when real-volume samples reveal additional valid layouts.
