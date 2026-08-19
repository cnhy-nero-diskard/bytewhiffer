## 1. Prerequisite and Measurement Baseline

- [ ] 1.1 Confirm `harden-ntfs-reconstruction` is present in the implementation base and its resolver accepts compact parsed records independent of raw-buffer lifetime.
- [ ] 1.2 Add a deterministic synthetic MFT source and baseline measurements for full raw buffer, per-record copies, parsed facts, reconstructed tree, and cancellation work.

## 2. Bounded Raw Pipeline

- [ ] 2.1 Implement an aligned chunked MFT record source with named maximum chunk/carry capacities and checked extent arithmetic.
- [ ] 2.2 Handle FILE records split across read boundaries without loss, duplication, or unbounded carry growth.
- [ ] 2.3 Parse complete records from mutable chunk slices with bounded per-worker scratch and remove the per-record `to_vec()` allocation.
- [ ] 2.4 Release raw chunks after compact parsed facts are retained and remove the contiguous full-MFT buffer.

## 3. Cancellation

- [ ] 3.1 Check cancellation before and after every raw read and before issuing the next configured-size chunk.
- [ ] 3.2 Add bounded cancellation checkpoints to parse, attribute resolution, reconstruction, and result-transfer batches.
- [ ] 3.3 Propagate `Cancelled` without partial authoritative output, Walker fallback, or an error dialog.

## 4. Resource and Equivalence Tests

- [ ] 4.1 Add chunk-boundary/property tests across record sizes, fragmented extents, short reads/errors, and cancellation positions.
- [ ] 4.2 Prove synthetic raw/transient buffering remains within the documented bound as record count grows.
- [ ] 4.3 Prove chunked output matches the hardened non-streaming corpus for attribute-list and fragmented-MFT cases.
- [ ] 4.4 Record parsed-record/tree retained memory separately so total peak claims remain honest.

## 5. Verification

- [ ] 5.1 Run formatting, clippy with warnings denied, debug/release tests, and the synthetic large-record benchmark/measurement.
- [ ] 5.2 Exercise cancellation during raw reads on elevated Windows NTFS hardware and document that latency is bounded by one in-flight OS read rather than claiming hard kernel-I/O interruption.

