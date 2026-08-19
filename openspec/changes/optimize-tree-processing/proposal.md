## Why

Large live and completed trees repeatedly pay whole-subtree walks, full analytics sorts, per-frame child sorting/layout allocation, and path-based assembly traversal. These costs can erode the responsiveness already promised by the scan HUD and treemap.

## What Changes

- Maintain or latch descendant-density information without rescanning the growing focused subtree after every live batch.
- Compute Insights through one aggregation traversal and retain leaderboard top-N with bounded state instead of cloning and sorting every descendant.
- Avoid rebuilding a second tree-shaped borrowed Insights topology solely to aggregate `Node` data.
- Preserve authoritative sorted order or cache treemap ordering/layout by tree, focus, viewport, and abstraction revisions.
- Replace `PendingAssembly` root re-walk/path cloning with a bounded conversion strategy that retains incremental progress and avoids a completion-frame freeze.
- Add large synthetic-tree complexity, equivalence, cache-invalidation, and frame-budget tests/benchmarks.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `scan-responsiveness`: Bound authoritative assembly and live-tree derived work while preserving incremental progress.
- `insights-drawer`: Produce equivalent analytics with one-pass aggregation and bounded top-N retention.
- `treemap-layout`: Reuse stable ordering/layout work and invalidate it only when relevant state changes.

## Impact

- Primary code: `src/app.rs`, `src/insights.rs`, `src/treemap.rs`, and synthetic performance tests.
- Prerequisite: `harden-scan-lifecycle`, whose generation/revision contract supplies reliable cache invalidation.
- UI output and navigation behavior remain unchanged; this proposal changes computational strategy and performance guarantees.
- Issue coverage: GitHub issue #7 findings 11 through 14.
