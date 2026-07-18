## ADDED Requirements

### Requirement: MFT engine capability check
The system SHALL implement `is_available` for the NTFS `$MFT`-reading engine
so it reports `Available` only when the target's volume is NTFS *and* the
current process is elevated, `RequiresElevation` when the volume is NTFS but
the process is not elevated, and `UnsupportedFilesystem` for any non-NTFS
target — re-evaluated whenever the scan target changes, not cached from an
earlier check.

#### Scenario: NTFS target, elevated process
- **WHEN** the capability check runs against the MFT engine for a target on
  an NTFS volume, from a process already holding an elevated token
- **THEN** it reports `Available`

#### Scenario: NTFS target, unelevated process
- **WHEN** the capability check runs against the MFT engine for a target on
  an NTFS volume, from a process that is not elevated
- **THEN** it reports `RequiresElevation`

#### Scenario: Non-NTFS target
- **WHEN** the capability check runs against the MFT engine for a target on
  a non-NTFS volume, regardless of elevation
- **THEN** it reports `UnsupportedFilesystem`

#### Scenario: Re-checked on target change
- **WHEN** an elevated process's scan target changes from an NTFS volume to
  a non-NTFS one, or vice versa
- **THEN** the next capability check reflects the new target's filesystem
  rather than a result cached from the previous target

### Requirement: NTFS $MFT-reading engine
The system SHALL provide a `ScanEngine` implementation that reads a local
NTFS volume's `$MFT` in one sequential pass, parses records in parallel, and
reconstructs an `Entry` tree equivalent in shape and correctness to the
directory walker's output: recursive directory sizes, children sorted
largest-first, no double-counting from hard links or reparse
points/junctions, and no ghost entries from deleted-but-unreclaimed MFT
records.

#### Scenario: Recursive size matches sum of descendants
- **WHEN** the MFT engine scans a volume and reconstructs the tree
- **THEN** each directory's `Entry.size` equals the sum of the sizes of all
  files and subdirectories beneath it, matching what the directory walker
  would compute for the same tree

#### Scenario: Children are sorted largest-first
- **WHEN** an MFT engine scan completes
- **THEN** every directory's `children` are ordered largest-to-smallest by
  size, at every level of the tree

#### Scenario: Hard links do not cause double-counted size
- **WHEN** the `$MFT` contains a record with more than one `$FILE_NAME`
  attribute (a hard-linked file)
- **THEN** the reconstructed tree counts that file's size once, not once per
  name

#### Scenario: Reparse points and junctions are not followed
- **WHEN** the MFT engine encounters a reparse point or junction record
  while reconstructing the tree
- **THEN** it does not traverse through it as if it were an ordinary
  directory, avoiding double-counted sizes or infinite recursion on a cyclic
  link

#### Scenario: Deleted-but-unreclaimed records are excluded
- **WHEN** the `$MFT` contains a record flagged as deleted whose space has
  not yet been reclaimed for a new file
- **THEN** that record does not appear in the reconstructed tree

#### Scenario: Scan cancellation is honored
- **WHEN** a cancellation signal is set while the MFT engine's parallel
  record-parsing or rollup phase is in progress
- **THEN** the scan stops and returns the partial result built so far,
  consistent with the same cancellation contract the walker honors

#### Scenario: Progress reporting stays monotonic
- **WHEN** an MFT engine scan is polled for progress during its sequential
  read and parsing phases
- **THEN** the reported counters never decrease between polls, even though
  this engine cannot stream live per-entry discovery the way the walker
  does
