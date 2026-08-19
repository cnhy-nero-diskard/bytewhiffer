## ADDED Requirements

### Requirement: Untrustworthy Turbo results fall back visibly
The system SHALL use the directory Walker when Turbo reports incomplete, corrupt, or target-not-found metadata and SHALL expose a concise diagnostic that Turbo was not authoritative for that scan.

#### Scenario: Walker fallback succeeds
- **WHEN** Turbo reports a recoverable trust failure and Walker completes successfully
- **THEN** the Walker tree becomes authoritative and the UI identifies Walker as the engine used

#### Scenario: Turbo false success is prohibited
- **WHEN** only a partial MFT or an empty placeholder tree is available
- **THEN** Turbo is not shown as a successful authoritative scan

