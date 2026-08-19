## 1. Shared Accounting Model

- [ ] 1.1 Add typed logical/allocated size metrics, stable file identity, and hard-link alias metadata to scanner entries without retaining ambiguous size semantics.
- [ ] 1.2 Implement shared deterministic identity deduplication and directory rollup using the canonical normalized relative-path rule.
- [ ] 1.3 Add pure tests for canonical hard-link attribution, discovery-order independence, alias retention, and ancestor allocated totals.

## 2. Engine Metadata

- [ ] 2.1 Implement Windows Walker probes for stable volume/file identity, logical length, and actual allocated bytes without following reparse targets.
- [ ] 2.2 Update Walker discovery/finalization to preserve identity-bearing paths and apply the shared accounting pass.
- [ ] 2.3 Update MFT parsing/reconstruction to retain file-reference identity, logical size, and unnamed-data allocated size for the shared accounting pass.
- [ ] 2.4 Return typed limitation/incomplete outcomes when required identity or allocated-size metadata cannot be trusted instead of substituting logical length.

## 3. Product Presentation

- [ ] 3.1 Switch treemap area, scan summaries, Insights, and directory totals to authoritative allocated bytes.
- [ ] 3.2 Update labels/tooltips to say allocated usage explicitly and show logical length only as separately labeled secondary metadata.
- [ ] 3.3 Document the unique-allocation, alias-attribution, sparse/compressed, and snapshot semantics in user-facing documentation.

## 4. Windows Equivalence Coverage

- [ ] 4.1 Build Windows fixtures for ordinary files, sparse files, NTFS-compressed files, two hard links, and hard links in different directories.
- [ ] 4.2 Assert expected logical/allocated values and deterministic deduplicated totals for Walker.
- [ ] 4.3 Compare Walker and Turbo root/subtree totals and canonical alias ownership on the same quiescent fixtures.

## 5. Verification

- [ ] 5.1 Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo test --release`.
- [ ] 5.2 Run the Windows accounting integration suite on NTFS and record filesystem/API evidence; do not claim Turbo parity if raw-volume/elevation tests were not exercised.

