## Context

Turbo currently self-relaunches the complete application with `runas`. The elevated process continues to render UI and expose Open, Reveal, and Delete, expanding privilege far beyond raw-volume reading. The preceding Turbo proposals establish typed results, bounded progress/cancellation, and trustworthy fallback semantics suitable for a process boundary.

## Goals / Non-Goals

**Goals:**

- Keep the long-lived GUI at normal user privilege.
- Elevate only a small read-only Turbo helper.
- Define authenticated, versioned, bounded IPC with fail-closed behavior.
- Preserve Turbo session reuse, progress, cancellation, and Walker fallback.

**Non-Goals:**

- Run arbitrary commands or general file actions in the helper.
- Create a persistent Windows service or install system-wide components.
- Support remote/network IPC.

## Decisions

**Use a dedicated helper mode/binary.** The helper entrypoint contains only argument validation, named-pipe connection, raw-volume/MFT scanning, and response serialization. It has no egui, shell-open, Explorer, or recycle-bin action surface. The normal UI launches it with `runas` and remains running.

**Use a local named pipe with least-privilege binding.** The UI creates a uniquely named pipe with a restrictive DACL for the current user/session before elevation. The helper connects to that existing pipe. Both sides verify protocol version, peer process/session where Windows APIs permit, and a per-launch random capability. Frames are length-prefixed and capped before allocation.

**Define explicit protocol messages.** Requests contain only a canonical scan target and scan ID. Responses are availability/ready, bounded progress, typed terminal outcome, and an authoritative tree encoded with a deterministic binary codec. Cancellation is a distinct request. Unknown versions/messages, oversized frames, target changes, or malformed trees terminate the helper session and are never interpreted permissively.

**Keep one helper for the UI session.** After successful UAC, the scoped helper may service later NTFS scans until UI exit, avoiding repeated prompts. The UI owns its process handle and closes/cancels it on shutdown. Helper crash or timeout marks Turbo unavailable for that request and permits Walker fallback according to typed outcome policy.

**Validate targets inside the helper.** The helper canonicalizes the requested local path, derives its volume itself, verifies NTFS/read-only scope, and never accepts a caller-supplied raw device path or arbitrary offset/read command.

## Risks / Trade-offs

- **IPC adds serialization and attack surface** → Keep the protocol small, length-bounded, versioned, fuzzed, and capability-bound.
- **Large authoritative trees are expensive to transfer** → Use compact binary encoding and coordinate memory measurements with `bound-turbo-resource-usage`.
- **UAC and pipe security are difficult to automate** → Unit-test codecs/state machines and add explicit real-Windows integration evidence before claiming completion.
- **Helper lifetime leaks after UI crash** → Bind lifetime to the UI process/job object or monitor the parent handle and exit promptly.

## Migration Plan

Implement only after reconstruction/resource prerequisites. Add protocol types and an unelevated fake helper first, introduce the scoped elevated entrypoint, switch Turbo launch to the pipe handshake, then remove elevated-GUI relaunch arguments. Rollback can restore the prior relaunch path until the final removal commit; no persisted data changes.

## Open Questions

None blocking. The implementation must select and document the exact Windows peer-verification and process-lifetime APIs after testing them on supported Windows versions.
