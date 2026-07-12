# disk-scanning

## Purpose
Defines the scanning backend contract and the directory-walker implementation
that discovers files and folders on disk, computes recursive sizes, and
reports live progress — the data source the rest of Bytewhiffer visualizes.

## Requirements

### Requirement: ScanEngine trait contract
The system SHALL define a `ScanEngine` trait that any scanning backend (the
directory walker in this change, a future NTFS `$MFT` reader in v2) must
implement, so the UI orchestration layer can select and drive a scan engine
without depending on its internal implementation.

#### Scenario: A conforming engine can be driven generically
- **WHEN** the UI orchestration layer starts a scan against a chosen
  `ScanEngine` implementation and a target path
- **THEN** it does so only through the trait's methods (availability check,
  scan execution, cancellation, progress polling), with no engine-specific
  code in the orchestration layer

### Requirement: Engine capability/fallback check
The system SHALL provide a way to ask an engine whether it can handle a given
target before starting a scan, returning one of: available, requires
elevation, unsupported filesystem, or not applicable.

#### Scenario: Walker engine reports availability for any target
- **WHEN** the capability check is run against the walker engine for any
  readable path
- **THEN** it reports `Available`

#### Scenario: Orchestration layer can react to a non-available engine
- **WHEN** a capability check reports anything other than `Available` for the
  engine currently selected
- **THEN** the orchestration layer has a defined signal to fall back to
  another engine rather than starting a scan that cannot succeed

### Requirement: Scan result distinguishes partial success from total failure
The system SHALL return scan results as `Result<Entry, ScanError>`, where
individual unreadable entries within an otherwise successful scan are skipped
and reflected only in the resulting tree, while an engine's inability to run
against the target at all is surfaced as an error distinct from a normal
completed scan.

#### Scenario: A single unreadable file does not fail the whole scan
- **WHEN** the walker encounters a file or directory entry it cannot read
  (e.g. a permission error) while scanning an otherwise accessible tree
- **THEN** the scan completes successfully, the unreadable entry is omitted
  from the resulting tree, and no error is returned

#### Scenario: An engine that cannot run at all reports an error
- **WHEN** an engine determines it cannot process the requested target (for
  example, a capability check already reported non-availability)
- **THEN** attempting to scan returns an error result rather than an empty or
  partial `Entry` tree indistinguishable from "nothing found"

### Requirement: Directory walker engine
The system SHALL provide a `ScanEngine` implementation that recursively scans
a directory tree via the filesystem, computing the total recursive size of
each directory and producing an `Entry` tree with children sorted
largest-first.

#### Scenario: Recursive size is the sum of all descendants
- **WHEN** the walker scans a directory containing files and nested
  subdirectories
- **THEN** each directory's `Entry.size` equals the sum of the sizes of all
  files and subdirectories beneath it

#### Scenario: Children are sorted largest-first
- **WHEN** a scan of a directory with multiple children completes
- **THEN** that directory's `children` are ordered from largest to smallest by
  size, at every level of the tree

#### Scenario: Symlinks and junctions do not cause double-counting or loops
- **WHEN** the walker encounters a symlink or junction during traversal
- **THEN** it does not follow it, avoiding double-counted sizes or infinite
  recursion on a cyclic link

### Requirement: Scan cancellation
The system SHALL support cancelling an in-progress scan from another thread,
after which the scan stops descending further and returns whatever partial
tree it had already built.

#### Scenario: Cancelling mid-scan halts further traversal
- **WHEN** a cancellation signal is set while a scan is in progress
- **THEN** the scan stops visiting new directory entries soon after and
  returns the partial tree built so far, rather than continuing to
  completion

#### Scenario: Pre-set cancellation prevents any traversal
- **WHEN** a scan is started with cancellation already signaled
- **THEN** the scan returns immediately without reading the root directory's
  contents

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
