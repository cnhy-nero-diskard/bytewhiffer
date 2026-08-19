## ADDED Requirements

### Requirement: Bounded best-effort live discovery
The system SHALL bound the number of live discovery events retained for UI preview independently of total filesystem entry count. Producers MUST use non-blocking delivery and MAY drop preview events when capacity is exhausted.

#### Scenario: Producer outruns the UI
- **WHEN** scanners discover entries faster than the UI drains live events
- **THEN** queued live events remain within the configured capacity and scanner progress does not block on queue space

#### Scenario: Live events are dropped under pressure
- **WHEN** the bounded live-event channel is full
- **THEN** the event may be omitted from the provisional tree without affecting progress counters or the authoritative final tree

#### Scenario: Final tree replaces an incomplete preview
- **WHEN** most live discovery events were dropped but the scan completes successfully
- **THEN** the complete authoritative tree is assembled and atomically replaces the provisional tree

