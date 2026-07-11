## MODIFIED Requirements

### Requirement: Live progress reporting
The system SHALL expose a progress mechanism that a caller can poll from
another thread while a scan is in flight, sufficient to display "files
scanned," "directories scanned," and "bytes scanned so far," and that every
conforming engine updates in a way consistent with monotonically increasing
completion, whether or not that engine can report smooth incremental partial
trees.

#### Scenario: Progress counters increase monotonically during a scan
- **WHEN** a scan is in progress and progress is polled repeatedly
- **THEN** the reported files-scanned, directories-scanned, and bytes-scanned
  counters never decrease between polls

#### Scenario: Progress reaches a final state when the scan completes
- **WHEN** a scan finishes (successfully, with partial skips, or via
  cancellation)
- **THEN** the progress state reflects a stopped, final value that the caller
  can treat as "no longer in flight"
