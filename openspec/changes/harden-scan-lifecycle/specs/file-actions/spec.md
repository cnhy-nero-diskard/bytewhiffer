## ADDED Requirements

### Requirement: Delete is unavailable while tree state is provisional
The system SHALL make Delete unavailable while a scan generation is active or while its authoritative tree is still being assembled.

#### Scenario: Delete during active scan
- **WHEN** the user opens actions for an entry while a scan is active
- **THEN** Delete cannot be invoked and the UI explains that deletion is available after scanning finishes

#### Scenario: Delete during authoritative assembly
- **WHEN** scanning has finished but authoritative assembly remains active
- **THEN** Delete remains unavailable until the authoritative tree is installed

#### Scenario: Delete after stable completion
- **WHEN** no scan or assembly is active
- **THEN** Delete is available subject to the normal confirmation and filesystem rules

