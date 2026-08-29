- [x] 1.1 Confirm the scan-generation/structural-revision contract is present for tree preparation and cache invalidation.
- [x] 1.2 Add wide/deep synthetic trees and golden-equivalence coverage for assembly, Insights, density, and treemap ordering/layout behavior.

## 2. Authoritative Tree Preparation

- [x] 2.1 Extract an egui-free owned display-tree type/converter that runs on the generation's background worker.
- [x] 2.2 Publish exact conversion progress and atomically hand the completed current-generation tree to the UI.
- [x] 2.3 Remove index-path cloning/root re-walk assembly after equivalence, finishing-HUD, stale-generation, and cancellation tests pass.

## 3. Incremental Tree Metadata

- [x] 3.1 Maintain descendant counts and structural revisions through live insertion, authoritative conversion, and safe removal.
- [x] 3.2 Replace whole-subtree density refresh on every live revision with maintained metadata and focused invalidation tests.
- [x] 3.3 Preserve deterministic largest-first child ordering by repositioning affected mutations and sorting authoritative parents once.

## 4. Insights Aggregation

- [x] 4.1 Replace tree-shaped `InsightNode` construction and separate traversals with one direct borrowed-tree visitor.
- [x] 4.2 Implement bounded top-N retention with deterministic size/path ties and no all-descendant leaderboard allocation/sort.
- [x] 4.3 Verify extension totals, leaderboard navigation, blizzard flags, cleanup candidates, and Insights reuse behavior.

## 5. Treemap Layout Reuse

- [x] 5.1 Cache child order/layout by structural revision, focus, viewport geometry, and resolved abstraction settings.
- [x] 5.2 Add invalidation tests for insertion, removal, size changes, focus, viewport, and abstraction changes plus reuse/pruning tests.

## 6. Verification

- [x] 6.1 Run formatting, clippy with warnings denied, debug/release tests, Windows-target type checking, and the synthetic performance benchmark.
- [x] 6.2 Run the existing debug-performance path and record its advisory dense/all-card tessellation results without turning wall-clock numbers into flaky CI gates.

