## ADDED Requirements

### Requirement: Turbo raw and transient memory are bounded
The system SHALL process MFT bytes in bounded chunks and SHALL NOT retain a contiguous raw buffer proportional to the full MFT or allocate a separate heap buffer for every parsed record.

#### Scenario: MFT exceeds one chunk
- **WHEN** the MFT is larger than the configured raw chunk capacity
- **THEN** reading and parsing proceed across multiple chunks while buffered raw bytes remain within the documented chunk/carry bound

#### Scenario: Records cross chunk boundaries
- **WHEN** a FILE record is split between adjacent reads
- **THEN** bounded carry handling reconstructs exactly one complete record without losing or duplicating bytes

#### Scenario: Parallel parsing applies fixups
- **WHEN** a chunk contains many complete records
- **THEN** workers apply fixups using bounded chunk/scratch storage without one heap allocation per record

### Requirement: Turbo peak-memory components are measured
The system MUST provide a reproducible synthetic large-record measurement that reports bounded raw/transient buffers separately from unavoidable retained parsed-record and tree state.

#### Scenario: Synthetic large MFT measurement
- **WHEN** the measurement runs with increasing record counts
- **THEN** raw/transient buffering remains within its configured bound and retained-state growth is reported explicitly

#### Scenario: Cancellation measurement
- **WHEN** cancellation is triggered during the synthetic workload
- **THEN** the harness reports the bounded amount of work performed after signaling

