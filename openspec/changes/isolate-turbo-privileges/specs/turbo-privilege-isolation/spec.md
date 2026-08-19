## ADDED Requirements

### Requirement: Turbo elevation is limited to a scan helper
The system SHALL keep the interactive application process at normal user privilege and SHALL elevate only a helper whose exposed responsibility is validated read-only NTFS Turbo scanning.

#### Scenario: User accepts Turbo elevation
- **WHEN** the user confirms the UAC prompt
- **THEN** a scoped helper starts elevated while the existing UI process remains unelevated and retains its state

#### Scenario: Helper action surface
- **WHEN** the helper processes protocol requests
- **THEN** it accepts only supported scan, progress, cancellation, and shutdown operations and exposes no arbitrary file action or command execution

### Requirement: Helper IPC is local, authenticated, versioned, and bounded
The system SHALL use local IPC restricted to the launching user/session, bind the helper to the launching UI instance, version every protocol exchange, and reject oversized or malformed frames before unbounded allocation.

#### Scenario: Valid helper handshake
- **WHEN** the expected helper connects with the supported version and per-launch capability
- **THEN** the UI establishes the Turbo session and may submit the validated requested target

#### Scenario: Invalid peer or capability
- **WHEN** a process presents the wrong capability, session, or peer identity
- **THEN** the connection is rejected without performing raw-volume work

#### Scenario: Unsupported or oversized message
- **WHEN** either endpoint receives an unknown version/message or a frame above the configured maximum
- **THEN** it terminates the session without permissive parsing or unbounded allocation

### Requirement: Helper validates scan scope independently
The elevated helper SHALL canonicalize the requested local target, derive the volume itself, verify supported NTFS/read-only scope, and SHALL NOT accept caller-selected raw device paths, offsets, or arbitrary reads.

#### Scenario: Valid local NTFS target
- **WHEN** the UI requests a canonical path on a supported local NTFS volume
- **THEN** the helper derives and opens only that volume for read-only Turbo scanning

#### Scenario: Raw device path is requested
- **WHEN** a request supplies a raw device path or unsupported target form
- **THEN** the helper rejects it without opening the device

### Requirement: Helper lifetime follows the UI session
The system SHALL reuse one accepted helper for subsequent Turbo scans in the same UI session and SHALL terminate it promptly when the UI exits, cancels ownership, or loses the authenticated connection.

#### Scenario: Second Turbo scan in one session
- **WHEN** the established UI session requests another supported NTFS target
- **THEN** the existing helper services it without another UAC prompt

#### Scenario: UI exits unexpectedly
- **WHEN** the helper detects that its owning UI process or authenticated pipe has ended
- **THEN** it cancels active work, releases raw handles, and exits

### Requirement: Helper failure fails closed
The system SHALL treat helper crash, timeout, malformed response, or protocol loss as a typed Turbo failure and SHALL use Walker fallback only when the requested generation remains current and fallback is trustworthy.

#### Scenario: Helper crashes during a scan
- **WHEN** the helper exits before a terminal authoritative result
- **THEN** no partial helper result is accepted and the UI may start Walker fallback for the current generation

#### Scenario: Stale helper response arrives
- **WHEN** a helper response belongs to a superseded scan generation
- **THEN** the UI discards it without mutating current state

