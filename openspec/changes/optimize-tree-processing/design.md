## Context

The live tree increments `tree_rev` frequently, causing repeated descendant and Insights traversal. Insights first builds another tree-shaped borrowed topology, then performs four walks and globally sorts leaderboard entries. Treemap rendering sorts/allocates child vectors per visible node per frame. `PendingAssembly` stores cloned index paths and re-walks from the root for each item.

## Goals / Non-Goals

**Goals:**

- Keep derived work proportional to meaningful state changes.
- Aggregate Insights in one traversal with bounded top-N retention.
- Reuse treemap ordering/layout until relevant inputs change.
- Move authoritative conversion off the UI frame while retaining progress and atomic handoff.

**Non-Goals:**

- Change visible Insights results, treemap geometry, navigation, or abstraction semantics.
- Add premature GPU rendering or a new UI framework.
- Optimize before lifecycle generation/revision rules are stable.

## Decisions

**Extract an egui-free UI-tree conversion stage.** After a scanner returns `Entry`, the background generation converts it to an owned `DisplayTree`/`Node` representation while publishing atomic progress. The UI keeps showing the provisional tree and atomically accepts the completed generation. This removes root re-walk/index-path assembly without making scanner modules depend on egui.

**Maintain structural counts and stable order at mutation time.** Nodes track descendant count and a structural revision. Live insert/remove operations update ancestor counts and mark only affected child ordering dirty. Authoritative conversion produces recursively sorted children once.

**Aggregate Insights directly through one visitor.** A traversal accumulates extension totals, blizzard flags, cleanup candidates, and leaderboard candidates together. A fixed-size min-heap retains only the largest `N`, with deterministic size/path tie breaking. The visitor borrows tree nodes directly instead of allocating an `InsightNode` child vector for every node.

**Cache layouts by complete rendering inputs.** Cached child order and squarified rectangles are keyed by node structural revision, viewport dimensions, focus, and resolved abstraction/nesting settings. Pointer movement and unrelated chrome state do not invalidate layout. Live mutations invalidate only affected branches.

**Measure complexity with synthetic trees.** Tests assert result equivalence and invalidation correctness. Benchmarks compare scaling across wide/deep trees and enforce bounded top-N retention; wall-clock thresholds remain advisory to avoid flaky CI.

## Risks / Trade-offs

- **Caches increase retained memory and invalidation complexity** → Store caches only for visible/recent nodes or centrally with revision keys; correctness tests mutate every relevant input.
- **Background conversion duplicates provisional and final trees temporarily** → This already occurs conceptually; measure and coordinate with Turbo memory reporting.
- **One-pass aggregation couples outputs** → Keep independent accumulator fields and golden equivalence tests so adding an Insight remains localized.
- **Stable tie breaking may expose ordering changes** → Define it explicitly and update tests rather than rely on incidental sort stability.

## Migration Plan

Apply after `harden-scan-lifecycle`. First add equivalence benchmarks, then introduce direct aggregation and top-N retention, background conversion, structural metadata, and layout caching in independently testable steps. Remove `PendingAssembly` only after HUD finishing progress and atomic swap tests pass.

## Open Questions

None blocking. Cache retention limits can be tuned from the existing debug-performance path.
