## ADDED Requirements

### Requirement: Requested scan target is authoritative
The system SHALL maintain one current requested scan target and SHALL use it consistently for Scan, Rescan, engine capability checks, and Turbo elevation handoff.

#### Scenario: Typed target supersedes historical scan path
- **WHEN** the user scanned folder A, enters folder B, and invokes Turbo elevation for B
- **THEN** the elevation flow receives B and does not prefer the historical path A

#### Scenario: Rescan uses the current requested target
- **WHEN** the current requested target is B and the user invokes Rescan
- **THEN** the new generation scans B

#### Scenario: Capability check matches requested target
- **WHEN** the requested target changes between filesystems
- **THEN** Turbo availability is recomputed for that target before engine selection or elevation

