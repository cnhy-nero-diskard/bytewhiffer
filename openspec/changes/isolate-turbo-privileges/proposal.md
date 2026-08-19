## Why

Turbo currently elevates the entire GUI process, so unrelated Open, Reveal, and Delete actions inherit administrator privileges. Raw-volume reading should be the only elevated responsibility.

## What Changes

- Replace full-GUI elevation with a small elevated Turbo helper and a normal-privilege UI process.
- Define a narrow, versioned IPC protocol for scan request, bounded progress, cancellation, typed failure, and authoritative result transfer.
- Authenticate/bind the helper session to the launching UI process and validate all target/protocol inputs.
- Restrict the helper to read-only raw-volume acquisition and NTFS processing; it must not expose arbitrary file actions or general command execution.
- Ensure helper exit, crash, timeout, and malformed responses fail closed and allow trustworthy Walker fallback where appropriate.
- Keep Open, Reveal, Delete, dialogs, rendering, and general application state in the unelevated process.

## Capabilities

### New Capabilities

- `turbo-privilege-isolation`: Defines the least-privilege helper boundary, IPC contract, validation, lifecycle, and failure behavior.

### Modified Capabilities

- `turbo-mode`: Elevation launches a scoped helper rather than replacing the application with an elevated GUI process.
- `file-actions`: Guarantee that user file actions execute only in the normal-privilege UI process.

## Impact

- Primary code: Turbo process/elevation logic in `src/scanner/mft.rs` and `src/app.rs`, executable/argument dispatch in `src/main.rs`, and a new helper/IPC module or binary.
- Prerequisites: `harden-ntfs-reconstruction` and `bound-turbo-resource-usage`, which stabilize typed outcomes, cancellation, and result transport expectations.
- **BREAKING** for the internal elevation protocol and command-line handoff; no persisted user-data migration.
- Requires Windows integration tests for UAC, helper lifecycle, IPC rejection, and proof that the UI remains unelevated.
- Issue coverage: GitHub issue #7 finding 15 and the privilege-oriented module extraction from finding 16.
