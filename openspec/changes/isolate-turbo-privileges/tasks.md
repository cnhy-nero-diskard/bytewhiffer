## 1. Prerequisites and Protocol Model

- [ ] 1.1 Confirm `harden-ntfs-reconstruction` and `bound-turbo-resource-usage` are present with typed outcomes, bounded progress/cancellation, and compact authoritative results.
- [ ] 1.2 Define versioned, length-bounded request/progress/cancel/result/error/shutdown messages with scan-generation identity and deterministic encoding.
- [ ] 1.3 Add codec/state-machine tests and fuzz/property coverage for malformed, unknown-version, oversized, truncated, duplicate, and stale-generation messages.

## 2. Scoped Helper

- [ ] 2.1 Add a dedicated helper entrypoint/module containing no egui, Open, Reveal, Delete, or arbitrary command surface.
- [ ] 2.2 Validate/canonicalize local targets inside the helper, derive the volume internally, and reject raw device paths, caller offsets, non-NTFS, and unsupported scope.
- [ ] 2.3 Execute hardened read-only Turbo scanning and emit bounded progress, typed outcomes, cancellation, and one authoritative result.

## 3. Secure Local IPC and Lifetime

- [ ] 3.1 Create a uniquely named local pipe with a restrictive current-user/session DACL before elevation and generate a per-launch random capability.
- [ ] 3.2 Verify protocol version, capability, and peer/session/process identity where supported before accepting any scan request.
- [ ] 3.3 Bind helper lifetime to the UI process/pipe using Windows process or job-object monitoring, with prompt cancellation and handle cleanup after UI loss.
- [ ] 3.4 Enforce frame and result bounds before allocation and fail closed on protocol loss, timeout, or malformed data.

## 4. UI Integration

- [ ] 4.1 Replace elevated-GUI relaunch with `runas` helper launch while preserving the current unelevated UI state and requested target.
- [ ] 4.2 Reuse the established helper for subsequent NTFS scans in the same UI session and shut it down on normal exit.
- [ ] 4.3 Route helper progress/outcomes through the current scan generation and discard stale responses.
- [ ] 4.4 Fall back to Walker only for eligible current-generation helper failures and keep Open, Reveal, and confirmed Delete in the normal UI process.
- [ ] 4.5 Remove obsolete elevated-GUI command-line handoff and state after helper tests pass.

## 5. Verification

- [ ] 5.1 Run formatting, clippy with warnings denied, debug/release tests, protocol fuzz smoke tests, and fake-helper integration tests.
- [ ] 5.2 On supported Windows versions, verify UAC accept/decline, UI token remains unelevated, helper token is elevated, target validation, progress/cancel/result flow, helper reuse, crash/timeout fallback, and UI-exit cleanup.
- [ ] 5.3 Record Windows named-pipe ACL/peer-verification evidence and confirm the helper exposes no file-action or arbitrary-read command surface.

