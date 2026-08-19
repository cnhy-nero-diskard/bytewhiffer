## REMOVED Requirements

### Requirement: Elevation relaunch starts clean at the same scan root
**Reason**: Relaunching the entire GUI elevated grants administrator privilege to unrelated UI and file actions.

**Migration**: Keep the current UI running and pass the requested target to the scoped elevated helper defined by `turbo-privilege-isolation`.

### Requirement: Turbo stays on for the elevated process's lifetime
**Reason**: The GUI will no longer become the elevated process.

**Migration**: Persist Turbo availability through the scoped helper session for the lifetime of the normal-privilege UI process.

## ADDED Requirements

### Requirement: Elevation starts a scoped helper for the requested target
The system SHALL, after warning and UAC confirmation, start an elevated Turbo helper for the current requested scan target while preserving the unelevated UI process and its navigation state. Declining UAC SHALL leave the UI unchanged and promptable.

#### Scenario: Accepting elevation starts helper scanning
- **WHEN** the user accepts UAC for the requested target
- **THEN** the elevated helper begins the validated Turbo request and the existing UI remains active at normal privilege

#### Scenario: Declining elevation leaves the UI unchanged
- **WHEN** the user declines or cancels UAC
- **THEN** no helper session is established, no error is shown, and the UI remains unelevated and promptable

### Requirement: Turbo remains available for the helper session
The system SHALL reuse a successfully established helper for supported NTFS targets during the current UI session without repeated warning or UAC prompts. This state SHALL NOT persist across separate UI launches.

#### Scenario: Another NTFS target uses the existing helper
- **WHEN** the current UI session requests another supported NTFS scan
- **THEN** the established helper performs it without another UAC prompt

#### Scenario: Fresh application launch
- **WHEN** the application closes and is started again
- **THEN** no prior helper authority is inherited and Turbo returns to the target-dependent promptable state

