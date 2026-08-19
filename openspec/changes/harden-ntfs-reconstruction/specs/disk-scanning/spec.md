## ADDED Requirements

### Requirement: NTFS attribute-list extension records are resolved
The MFT engine SHALL parse `$ATTRIBUTE_LIST` entries, validate referenced FILE records, and merge supported attributes from extension records into their correct base record before reconstruction.

#### Scenario: Unnamed data resides in an extension record
- **WHEN** a base file record references an extension containing its unnamed `$DATA`
- **THEN** the reconstructed file uses the validated extension data for authoritative accounting

#### Scenario: Multiple extensions contribute attributes
- **WHEN** a base record has a valid chain of multiple extension records
- **THEN** relevant names, unnamed data, and flags are merged exactly once

#### Scenario: Stale or missing extension reference
- **WHEN** an attribute-list reference is missing or its sequence number no longer matches
- **THEN** the affected required record is classified as incomplete and is not trusted as successful reconstruction

#### Scenario: Cyclic or duplicate extension chain
- **WHEN** attribute-list references form a cycle or repeat the same extension
- **THEN** resolution terminates within configured bounds and reports malformed or incomplete metadata

#### Scenario: Named streams are irrelevant to authoritative data size
- **WHEN** an extension contains named data streams but no relevant unnamed stream
- **THEN** those named streams do not contribute to the authoritative file allocation

### Requirement: MFT data-run discovery handles extension metadata
The MFT engine SHALL resolve fragmented and attribute-list-backed `$MFT::$DATA` metadata iteratively and SHALL NOT assume record 0 alone contains every required data run.

#### Scenario: MFT data runs span an extension record
- **WHEN** record 0 references an addressable extension containing additional `$MFT::$DATA` runs
- **THEN** the engine validates and incorporates those runs before reading the complete MFT

#### Scenario: Required MFT extension cannot be addressed
- **WHEN** a required `$MFT` extension reference cannot be reached from validated known extents
- **THEN** Turbo returns `IncompleteMft` instead of a record-0-only successful result

### Requirement: Raw NTFS parsing is checked and bounded
The system SHALL treat boot sectors, fixups, record headers, attributes, attribute lists, data runs, and filesystem-derived sizes as hostile input. Invalid input MUST return a controlled parse outcome without panic, arithmetic overflow, out-of-bounds access, or allocation beyond configured limits.

#### Scenario: Invalid shift or run width
- **WHEN** metadata encodes a shift or data-run field wider than the destination representation
- **THEN** parsing rejects it without evaluating an invalid shift or overflowing

#### Scenario: Offset or length arithmetic overflows
- **WHEN** filesystem fields would overflow offset, length, cluster multiplication, or `usize` conversion
- **THEN** parsing returns a controlled malformed/resource-limit result before slicing or allocating

#### Scenario: Record zero fixups fail
- **WHEN** update-sequence fixups for MFT record 0 fail validation
- **THEN** no data runs from that record are trusted and Turbo cannot report success

#### Scenario: Arbitrary parser input
- **WHEN** fuzz/property harnesses supply arbitrary byte sequences to pure parser entry points
- **THEN** they do not panic or request pathological allocations

### Requirement: Turbo failures distinguish trust and fallback outcomes
The system SHALL distinguish unavailable Turbo, incomplete MFT, corrupt filesystem metadata, target not found, unreadable target, and cancellation. It SHALL use Walker fallback for recoverable Turbo trust failures and SHALL never represent them as an empty successful tree.

#### Scenario: Target is absent from reconstructed tree
- **WHEN** Turbo cannot locate the requested target in otherwise parsed records
- **THEN** it returns `TargetNotFound` and does not display a zero-byte success

#### Scenario: Turbo metadata is incomplete
- **WHEN** required MFT metadata cannot be reconstructed completely
- **THEN** the app attempts Walker fallback and records a non-fatal Turbo diagnostic

#### Scenario: Scan is cancelled
- **WHEN** Turbo terminates with `Cancelled`
- **THEN** the app neither falls back nor shows an error dialog for that generation

#### Scenario: Both engines cannot read the target
- **WHEN** Turbo falls back and Walker also cannot read the requested root
- **THEN** the user receives a root-unreadable error rather than a plausible partial result

