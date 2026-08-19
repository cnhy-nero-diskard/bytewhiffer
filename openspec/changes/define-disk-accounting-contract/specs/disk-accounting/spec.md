## ADDED Requirements

### Requirement: Unique allocated bytes are authoritative
The system SHALL use `UniqueAllocatedBytes` as the authoritative metric for treemap area, directory rollups, scan totals, and size-based Insights. This metric SHALL represent physical allocation within the scan root, counted once per stable file identity.

#### Scenario: Ordinary file accounting
- **WHEN** a file has one directory entry and an allocated size reported by the filesystem
- **THEN** its allocated bytes contribute once to its ancestors and authoritative totals

#### Scenario: Sparse or compressed file accounting
- **WHEN** a file's physical allocation differs from its logical content length
- **THEN** treemap area and aggregate totals use physical allocation rather than logical length

### Requirement: Logical length remains secondary metadata
The system SHALL keep logical content length separate from authoritative allocated bytes and SHALL label each metric unambiguously wherever both are shown.

#### Scenario: Tooltip shows both metrics
- **WHEN** the UI displays allocated and logical sizes for an entry
- **THEN** each value is explicitly labeled and neither is presented as the other

### Requirement: Stable identity deduplicates hard links
The system SHALL identify files independently of path and SHALL count one physical allocation only once when multiple directory entries reference the same file identity.

#### Scenario: Two hard links inside the scan root
- **WHEN** two paths in the scan root reference the same stable file identity
- **THEN** their shared allocated bytes contribute exactly once to the root total

#### Scenario: Same name does not imply same identity
- **WHEN** two different files have equal names or lengths
- **THEN** they remain distinct unless their stable file identities match

### Requirement: Hard-link attribution is deterministic
The system SHALL attribute shared allocation to one deterministic canonical relative path and SHALL preserve other paths as aliases without adding their allocation again.

#### Scenario: Discovery order changes
- **WHEN** the same hard-link fixture is scanned with a different parallel discovery order
- **THEN** the same canonical path owns the allocated bytes and all aggregate totals remain identical

#### Scenario: Alias is retained
- **WHEN** a non-canonical hard-link path is present
- **THEN** its path and logical metadata remain available as an alias while its accounted allocated bytes are zero

### Requirement: Accounting wording matches the metric
The system SHALL describe authoritative totals as allocated disk usage and SHALL NOT imply that logical byte length is physical space consumption.

#### Scenario: Scan completes
- **WHEN** the UI presents the completed authoritative total
- **THEN** the label identifies the total as allocated usage

