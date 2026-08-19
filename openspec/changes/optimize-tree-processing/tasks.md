## 1. Prerequisite and Baselines

- [ ] 1.1 Confirm `harden-scan-lifecycle` is present and expose its generation/structural revision signals to tree preparation and cache invalidation.
- [ ] 1.2 Add large wide/deep synthetic trees and golden outputs for current assembly, Insights, density, and treemap ordering/layout behavior.

## 2. Authoritative Tree Preparation

- [ ] 2.1 Extract an egui-free owned display-tree type/converter that can run on the generation's background worker.
- [ ] 2.2 Publish exact conversion progress and atomically hand the completed current-generation tree to the UI.
- [ ] 2.3 Remove index-path cloning/root re-walk assembly after equivalence, finishing-HUD, stale-generation, and no-completion-frame-freeze tests pass.

## 3. Incremental Tree Metadata

- [ ] 3.1 Maintain descendant counts and structural revisions through live insertion, authoritative conversion, and safe removal.
- [ ] 3.2 Replace whole-subtree density refresh on every live revision with incremental/latching logic and focused invalidation tests.
- [ ] 3.3 Preserve or lazily repair deterministic largest-first child ordering at mutation boundaries.

## 4. Insights Aggregation

- [ ] 4.1 Replace tree-shaped `InsightNode` construction and separate traversals with one direct borrowed-tree visitor.
- [ ] 4.2 Implement bounded top-N retention with deterministic size/path ties and no all-descendant leaderboard allocation/sort.
- [ ] 4.3 Verify extension totals, leaderboard navigation, blizzard flags, and cleanup candidates are golden-equivalent and reused across pointer-only frames.

## 5. Treemap Layout Reuse

- [ ] 5.1 Cache child order/layout by structural revision, focus, viewport geometry, and resolved abstraction settings.
- [ ] 5.2 Add invalidation tests for insertion, removal, size changes, focus, viewport, and abstraction changes plus reuse tests for hover/chrome-only repaints.

## 6. Verification

- [ ] 6.1 Run formatting, clippy with warnings denied, debug/release tests, and synthetic complexity benchmarks.
- [ ] 6.2 Use the existing debug-performance path on a large tree to record before/after traversal counts, allocations, finishing responsiveness, and frame behavior without turning wall-clock numbers into flaky CI gates.

