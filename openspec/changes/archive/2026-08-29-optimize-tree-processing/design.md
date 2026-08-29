## Context

The live tree increments `tree_rev` frequently, so derived work must not turn each discovery batch into another subtree walk. The earlier completion path also cloned index paths and re-walked from the root for each item, while Insights built a second tree-shaped topology before performing multiple traversals and a global leaderboard sort. Treemap rendering likewise sorted and allocated child-layout data on every visible frame.

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

**Extract an egui-free UI-tree conversion stage.** After a scanner returns `Entry`, the background generation converts it to an owned `DisplayNode` representation with an explicit stack, moving each finished child into its parent and publishing atomic progress. The UI keeps showing the provisional live tree and atomically accepts the completed generation. This removes root re-walk/index-path assembly without making scanner modules depend on egui.

**Maintain structural counts and stable order at mutation time.** Nodes track descendant count and a structural revision. Live insert/remove operations update ancestor counts, bump only affected revisions, and reposition the changed child rather than sorting every sibling after each event. Authoritative conversion sorts each parent once before publication.

**Aggregate Insights directly through one visitor.** A traversal accumulates extension totals, blizzard flags, cleanup candidates, and leaderboard candidates together. A fixed-size worst-first `BinaryHeap` retains only the largest `N`, with deterministic size/path/name/trail tie breaking; only the bounded result is sorted for display. The visitor borrows tree nodes directly instead of allocating an `InsightNode` child vector for every node.

**Cache layouts by complete rendering inputs.** Cached child order and squarified rectangles are keyed by node structural revision, viewport dimensions, focus, and resolved abstraction/nesting settings. Pointer movement and unrelated chrome state do not invalidate layout. Live mutations invalidate only affected branches.

**Measure complexity with synthetic trees.** Tests assert result equivalence and invalidation correctness. Benchmarks compare scaling across wide/deep trees and enforce bounded top-N retention; wall-clock thresholds remain advisory to avoid flaky CI.

## Risks / Trade-offs

- **Caches increase retained memory and invalidation complexity** → Store caches only for visible/recent nodes or centrally with revision keys; correctness tests mutate every relevant input.
- **Background conversion duplicates provisional and final trees temporarily** → This already occurs conceptually; measure and coordinate with Turbo memory reporting.
- **One-pass aggregation couples outputs** → Keep independent accumulator fields and golden equivalence tests so adding an Insight remains localized.
- **Stable tie breaking may expose ordering changes** → Define it explicitly and update tests rather than rely on incidental sort stability.

## Migration Plan

Apply after the scan-generation contract is stable. First add equivalence tests, then introduce direct aggregation and top-N retention, background conversion, structural metadata, and layout caching in independently testable steps. The old `PendingAssembly` path has been removed: conversion now completes on the worker and publishes only a complete `DisplayNode` tree.

## Validation Record

The implementation was checked with `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, debug and release `cargo test` (94 tests), and `cargo check --target x86_64-pc-windows-msvc`. The existing `--debug-perf` path also ran successfully: the dense synthetic scene produced 315 visible blocks and 3.370 ms/1.505 ms median tessellation for flat/elevated rendering; the 400-card scene produced 6.402 ms/9.301 ms. These timings remain advisory rather than CI gates.

## Open Questions

None blocking. Cache retention limits can be tuned from the existing debug-performance path.
