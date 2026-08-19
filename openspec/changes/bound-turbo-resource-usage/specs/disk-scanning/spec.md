## ADDED Requirements

### Requirement: Turbo cancellation is checked across bounded work units
The MFT engine SHALL check cancellation before and after every bounded raw read, record-parse batch, attribute-resolution batch, reconstruction batch, and result-transfer batch.

#### Scenario: Cancellation during raw MFT reading
- **WHEN** cancellation is signaled while Turbo is reading MFT extents
- **THEN** no additional raw chunk larger than the configured maximum is started and the engine returns `Cancelled` after the in-flight read completes

#### Scenario: Cancellation during parsing or reconstruction
- **WHEN** cancellation is signaled during CPU processing
- **THEN** processing stops at the next bounded batch checkpoint without publishing a partial authoritative tree

#### Scenario: Cancellation remains non-error control flow
- **WHEN** any Turbo phase observes cancellation
- **THEN** the generation terminates without a user-visible failure or Walker fallback

