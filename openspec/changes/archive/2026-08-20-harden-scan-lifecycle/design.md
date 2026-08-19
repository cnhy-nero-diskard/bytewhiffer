## Context

`BytewhifferApp` currently owns an `ActiveScan` directly, uses an unbounded `mpsc` channel for best-effort discovery, and replaces the active handle when another scan starts. The live tree is advisory while the completed `Entry` tree is authoritative, but that distinction is not protected by a generation identity. Pending UI assembly and delete actions add two more writers to related state.

## Goals / Non-Goals

**Goals:**

- Make exactly one scan generation authoritative at a time.
- Bound live-event memory and keep event delivery non-blocking.
- Cancel and eventually join every superseded worker without blocking the UI.
- Prevent cancelled, stale, or panicked workers from corrupting later state.
- Centralize requested-target and delete-availability decisions.

**Non-Goals:**

- Redesign scanner algorithms or disk-accounting semantics.
- Guarantee instantaneous cancellation inside one blocking operating-system call.
- Preserve discarded live trees from superseded generations.

## Decisions

**Extract a generation-aware `ScanController`.** Each start receives a monotonically increasing `ScanId`. The controller owns current scan state, pending authoritative assembly, and retired handles. Events and final outcomes carry the generation ID; only the current ID may mutate visible state. A counter is sufficient because IDs are process-local and wrapping can be handled by skipping IDs still retained.

**Retire workers through a dedicated reaper path.** Superseding first sets cancellation, disconnects the old event receiver, and transfers its `JoinHandle` to a reaper thread or queue. The UI never calls an unbounded join. The reaper always joins and discards late results, including panics, so no worker is detached indefinitely.

**Use a bounded best-effort discovery channel.** `ScanContext` uses a bounded sender and `try_send`. Full or disconnected delivery is ignored for live rendering only; progress counters and the final `Entry` tree remain authoritative. Scanner work must never wait for UI capacity.

**Represent cancellation explicitly.** Engines return a cancellation outcome distinct from failure and success. A cancelled generation does not install a partial tree, finalize a success summary, or show an error. This replaces the current implicit empty/partial-success contract.

**Resolve one requested target at action time.** Folder selection, Scan, Rescan, and Turbo elevation update/read one normalized `requested_target`. Free-form text is resolved when the user invokes an action; `last_scanned_path` remains historical display data, not an input precedence rule.

**Gate Delete on controller stability.** Delete is disabled whenever the current generation is scanning or its authoritative tree is assembling. Confirmation/classification changes remain in `make-cleanup-actions-safer`.

## Risks / Trade-offs

- **Dropped live events make the preview less complete under pressure** → Keep UI copy explicit that live discovery is provisional and atomically replace it with the complete final tree.
- **A worker stuck in an OS call delays reaping** → Reaping is eventual and off the UI thread; engine-specific bounded I/O cancellation is handled by `bound-turbo-resource-usage`.
- **Generation plumbing touches several UI paths** → Put transition logic behind controller methods and test transitions without egui.
- **Bounded capacity needs tuning** → Make capacity a named constant and test saturation independently of a particular capacity value.

## Migration Plan

Introduce the controller behind existing Scan/Rescan/Turbo actions, switch event emission to bounded delivery, then route completion and assembly through generation checks. Remove direct `self.scan` replacement only after orchestration tests cover supersession and panic recovery. Rollback is code-only; no persisted state changes.

## Open Questions

None blocking. The exact queue capacity can be selected through synthetic stress measurements during implementation.
