## 1. Prerequisite and Classification

- [x] 1.1 Confirm the existing scan/assembly lifecycle gate keeps Delete unavailable while the visible tree can still change.
- [x] 1.2 Replace string-only cleanup classification with a pure structured category, reason, and confidence model.
- [x] 1.3 Define/test high-confidence, medium-confidence, context-dependent, non-match, case-insensitive, and nested-candidate behavior.

## 2. Cleanup Candidate UX

- [x] 2.1 Rename the drawer section and empty/help text to Cleanup candidates.
- [x] 2.2 Render each candidate's reason and confidence with explicit advisory language and preserve Open/Reveal/action navigation.
- [x] 2.3 Update documentation and tests to avoid claims that a heuristic match is safe to delete.

## 3. Confirmed Delete Flow

- [x] 3.1 Add one shared pending-delete state for treemap and Insights entry points containing the exact path, type, display name, and trail.
- [x] 3.2 Add an accessible confirmation dialog that performs no filesystem operation until confirmed and cancels safely if lifecycle state becomes unstable.
- [x] 3.3 Preserve filesystem failure reporting and mutate visible state only after `trash::delete` succeeds.

## 4. Safe Tree Mutation

- [x] 4.1 Replace raw-pointer/`unsafe` ancestor bookkeeping with a recursive safe removal API that returns removed allocated bytes while unwinding.
- [x] 4.2 Repair affected child indexes, ancestor sizes/revisions, and focus after successful removal.
- [x] 4.3 Add tests for leaf/nested/missing removal, focused-directory removal, large values, cancelled confirmation, failed deletion, and both UI entry points.

## 5. Verification

- [x] 5.1 Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo test --release`.
- [x] 5.2 Manually verify confirmation, recycle-bin behavior, failure messaging, focus repair, and candidate wording on Windows.

