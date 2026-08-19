## Context

Turbo currently reads all MFT extents into one `Vec<u8>`, copies each record before applying fixups, retains parsed records, reconstructs `Entry`, and later builds the UI tree. Cancellation is checked only after the full raw read. Attribute-list resolution requires cross-record information, so naive one-record streaming would break correctness.

## Goals / Non-Goals

**Goals:**

- Check cancellation before and after bounded raw reads and parse batches.
- Bound raw/transient memory independently of total MFT byte length.
- Avoid per-record allocation while preserving validated cross-record resolution.
- Measure peak components and large-record behavior reproducibly.

**Non-Goals:**

- Force the retained parsed-record and final-tree state below O(record count).
- Guarantee cancellation during a single blocking kernel read.
- Change accounting or attribute-list correctness rules.

## Decisions

**Introduce a chunked MFT record source.** Validated extents are read in aligned chunks no larger than a named maximum. Partial records carry into the next chunk. Cancellation is checked before issuing and immediately after each read, so latency is bounded by one configured chunk read plus scheduling.

**Apply fixups in chunk storage.** Complete records are exposed as mutable fixed-size slices and processed with Rayon `par_chunks_mut` or equivalent batching. This eliminates `to_vec()` per record. Parsed facts are copied into compact owned structures because attribute-list resolution needs random access after raw bytes are released.

**Separate unavoidable from transient memory.** The documented budget is: bounded raw chunk/carry buffers, bounded per-worker scratch, compact parsed-record index, reconstructed authoritative tree, and any UI tree retained by the lifecycle proposal. Only the first two are constant with respect to MFT byte length; benchmarks report every component rather than claiming constant total memory.

**Make cancellation a first-class pipeline exit.** Read, parse, resolve, reconstruction, and result-transfer loops all poll the shared token at bounded work units and return `Cancelled`. No partial authoritative tree is emitted.

**Benchmark a synthetic record source.** A deterministic generator feeds large record counts without requiring a raw volume. Measurements capture elapsed time, maximum buffered raw bytes, copied-record allocations, parsed-record size, and cancellation work after signaling.

## Risks / Trade-offs

- **Smaller chunks increase system-call overhead** → Tune one named default using measurements while keeping the bound testable.
- **Rayon mutation of chunk slices is complex** → Keep record boundaries explicit and property-test carry/partition logic.
- **Parsed state can still be large** → Use compact fields and document it honestly; a future external-memory design is out of scope.
- **One OS read can stall** → The UI remains responsive and the worker is reaped off-thread; hard cancellation of kernel I/O is a later Windows-specific option.

## Migration Plan

Land after `harden-ntfs-reconstruction`. Add the chunk source and measurement harness, switch parsing to borrowed mutable record slices, then remove the whole-MFT buffer. Preserve the prior pipeline behind small internal seams until equivalence tests pass. No user data migration.

## Open Questions

None blocking. Chunk size and Rayon batch size are measurement-driven constants selected during implementation.
