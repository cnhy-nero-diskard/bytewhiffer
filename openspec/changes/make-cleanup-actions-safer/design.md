## Context

The current fixed name matcher groups broad terms such as `build`, `dist`, and installers under “Junk suggestions.” Right-click Delete immediately calls the recycle-bin API and then mutates ancestor sizes through raw pointers. The scan-lifecycle proposal separately prevents deletion while a scan or assembly can overwrite the tree.

## Goals / Non-Goals

**Goals:**

- Present cleanup matches as advisory candidates with reasons and confidence.
- Require a deliberate confirmation before any recycle-bin operation.
- Update tree sizes/focus through safe Rust after successful deletion.
- Keep actions consistent between treemap and Insights entry points.

**Non-Goals:**

- Automatically delete, bulk-select, or calculate whether deletion is globally safe.
- Replace the operating-system recycle bin with permanent deletion.
- Expand into content-based malware or duplicate-file analysis.

## Decisions

**Return structured classifications.** The pure classifier returns a category, human-readable reason, and confidence enum (`High`, `Medium`, `ContextDependent`). Specific disposable caches may be high confidence; generic build/output names and installers are context-dependent. UI wording never calls a match safe.

**Use one staged delete command.** Both treemap and Insights context menus create a `PendingDelete` containing path, display name, type, and trail. A modal confirmation names the exact item and states it will be sent to the recycle bin. Cancel clears the command; confirm invokes `trash::delete` once.

**Mutate UI state only after filesystem success.** Failures preserve the node and show the existing error path. Success calls a safe recursive `remove` that returns removed accounted bytes while unwinding, subtracts from each ancestor with checked/saturating policy, rebuilds affected indexes, repairs focus, and increments structural revision.

**Rely on lifecycle gating.** The delete command cannot be opened or confirmed while scanning/assembly is active. If lifecycle state changes while a confirmation is open, confirmation is disabled/cancelled and must be initiated again from stable state.

## Risks / Trade-offs

- **Confirmation adds friction** → Destructive intent matters more than one-click speed; keep the dialog concise and keyboard accessible.
- **Heuristic confidence can be mistaken for certainty** → Pair every confidence label with a concrete reason and explicit advisory language.
- **Filesystem changes between prompt and confirm** → Treat recycle-bin errors as authoritative and never remove UI state on failure.
- **Recursive removal could mishandle deep paths** → Unit-test leaf, nested, missing, focused, and large-size cases without `unsafe`.

## Migration Plan

Add structured classification and new labels, introduce the shared confirmation state, then replace `Node::remove` after regression tests capture existing successful/failed behavior. Remove old “junk” terminology once specs/UI/tests use “Cleanup candidates.” No persisted migration.

## Open Questions

None blocking.
