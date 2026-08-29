## ADDED Requirements

### Requirement: Authoritative tree preparation stays off the UI frame
The system SHALL prepare the completed scan tree in bounded background work, publish genuine finishing progress, and atomically install it without path-cloning/root-rewalk work proportional to node depth on the UI thread.

#### Scenario: Large authoritative result completes
- **WHEN** a scan returns a large authoritative `Entry` tree
- **THEN** the provisional tree remains interactive while background preparation advances and the completed tree is installed atomically

#### Scenario: Deep tree preparation
- **WHEN** the authoritative tree contains deeply nested paths
- **THEN** preparation does not repeatedly traverse from the root for every node solely to locate its parent

### Requirement: Live density work is incrementally maintained
The system SHALL avoid a whole-focused-subtree descendant walk after every live discovery batch while preserving the correct dense-rendering decision.

#### Scenario: Live tree grows below the density threshold
- **WHEN** entries are inserted into the current focus
- **THEN** descendant metadata updates with the mutation and density state remains correct without rescanning the entire subtree

#### Scenario: Density threshold is crossed
- **WHEN** incremental descendant count exceeds the configured threshold
- **THEN** dense rendering activates and remains valid until a relevant focus or structural change invalidates it

