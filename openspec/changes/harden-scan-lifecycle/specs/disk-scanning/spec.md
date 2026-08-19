## MODIFIED Requirements

### Requirement: Scan cancellation
The system SHALL support cancelling an in-progress scan from another thread. Cancellation SHALL terminate that generation as normal control flow, SHALL stop new traversal at bounded cooperative checkpoints, and SHALL NOT publish a partial tree as an authoritative successful result or show a failure dialog.

#### Scenario: Cancelling mid-scan halts further traversal
- **WHEN** a cancellation signal is set while a scan is in progress
- **THEN** the scan stops visiting new entries at its next cooperative checkpoint and terminates with a cancelled outcome

#### Scenario: Pre-set cancellation prevents traversal
- **WHEN** a scan is started with cancellation already signaled
- **THEN** it terminates without reading the root contents or publishing a successful tree

#### Scenario: Cancellation is not shown as an error
- **WHEN** a scan generation terminates because it was cancelled
- **THEN** the UI shows no scan-failure dialog and does not finalize that generation as a successful scan

## ADDED Requirements

### Requirement: Single authoritative scan generation
The system SHALL assign every scan a generation identity and SHALL allow only the current generation's events, progress, final result, and assembly result to mutate current UI state.

#### Scenario: New scan supersedes an active scan
- **WHEN** scan B starts while scan A is active
- **THEN** A is cancelled and no later event or result from A can replace or modify B's state

#### Scenario: Rescan supersedes the current scan
- **WHEN** the user invokes Rescan while a generation is active
- **THEN** the active generation is retired and exactly one new generation becomes authoritative

#### Scenario: Late completion is ignored
- **WHEN** a retired generation completes after the current generation has started
- **THEN** its final tree and summary are discarded

### Requirement: Superseded workers are reaped without blocking the UI
The system SHALL retain ownership of every started scan worker until it has been joined, while SHALL NOT perform an unbounded worker join on the UI thread.

#### Scenario: Superseded worker exits cooperatively
- **WHEN** a superseded worker observes cancellation and returns
- **THEN** a non-UI reaping path joins it and releases its resources

#### Scenario: Scan worker panics
- **WHEN** a scan worker panics
- **THEN** the worker is joined, the panic is contained to that generation, and a later scan can start and complete normally

