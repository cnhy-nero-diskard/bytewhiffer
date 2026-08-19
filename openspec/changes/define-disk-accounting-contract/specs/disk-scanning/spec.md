## ADDED Requirements

### Requirement: Scan engines implement the shared accounting contract
Every authoritative scan engine SHALL populate stable identity, allocated bytes, and logical bytes according to the `disk-accounting` capability and SHALL perform identity deduplication before directory rollup.

#### Scenario: Walker and Turbo scan the same stable fixture
- **WHEN** Walker and Turbo scan the same quiescent supported Windows fixture
- **THEN** they produce equal root and subtree allocated totals and equal canonical hard-link attribution

#### Scenario: Required accounting metadata is unavailable
- **WHEN** an engine cannot obtain trustworthy stable identity or allocated-size metadata for an entry required by the scan
- **THEN** it reports a typed limitation or incomplete result rather than silently substituting logical length

### Requirement: Windows accounting fixtures cover filesystem edge cases
The system MUST maintain integration fixtures for ordinary, sparse, compressed, and hard-linked files on supported Windows filesystems.

#### Scenario: Accounting regression suite runs on Windows
- **WHEN** the Windows integration suite provisions the accounting fixtures
- **THEN** it verifies expected allocated/logical values, identity deduplication, and Walker/Turbo equivalence

