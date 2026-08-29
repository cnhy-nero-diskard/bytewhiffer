## Why

The Insights drawer labels broad heuristics as disposable, and Delete is immediate despite being fed by advisory matches. Cleanup guidance should explain uncertainty and require deliberate confirmation, while tree mutation should not rely on application-level `unsafe` bookkeeping.

## What Changes

- Rename the broad heuristic section to “Cleanup candidates.”
- Show the match reason and a confidence/context classification for every candidate.
- Distinguish high-confidence caches from context-dependent build outputs and installers without claiming any candidate is safe to delete.
- Require an explicit confirmation step before sending any file or directory to the recycle bin from either treemap or Insights actions.
- Replace raw-pointer ancestor mutation in `Node::remove` with a safe recursive size-propagation API.
- Preserve failed-delete error reporting, focus repair, and post-delete ancestor totals.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `insights-drawer`: Present advisory cleanup candidates with reasons and confidence rather than using an unqualified disposable-item label.
- `file-actions`: Require delete confirmation and safe, consistent visible-tree updates after successful deletion.

## Impact

- Primary code: `src/insights.rs`, cleanup rendering/actions and `Node::remove` in `src/app.rs`, plus UI/pure-helper tests.
- Prerequisite: `harden-scan-lifecycle` for the separate rule that Delete is unavailable during scan/assembly.
- No change to Open or Reveal behavior; deletion continues to use the recycle bin.
- Issue coverage: GitHub issue #7 finding 17 and the safe-removal portion of finding 16.
