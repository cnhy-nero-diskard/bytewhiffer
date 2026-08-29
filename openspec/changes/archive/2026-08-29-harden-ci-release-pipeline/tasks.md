## 1. Reproducible Tooling and Dependency Policy

- [x] 1.1 Select the newest repository-compatible exact Rust release and add `rust-toolchain.toml` with rustfmt, clippy, and `x86_64-pc-windows-msvc`.
- [x] 1.2 Add pinned `cargo-deny` tooling/config for advisories, licenses, bans, and sources with no undocumented broad exceptions.
- [x] 1.3 Document the chosen toolchain/target and the process for reviewing temporary dependency-policy exceptions.

## 2. Reusable Quality Gate

- [x] 2.1 Create or refactor a reusable quality workflow that checks out an explicit SHA and uses least job permissions.
- [x] 2.2 Run format check, all-target clippy with warnings denied, debug tests, release tests, and explicit-target release build with caching.
- [x] 2.3 Run dependency policy as a required job and add a final aggregate job that fails when any required dependency fails or is unexpectedly skipped.

## 3. CI Semantics

- [x] 3.1 Replace the `proceed=false` skip path with explicit non-zero commit-policy failure or remove the policy gate if repository rules make it redundant.
- [x] 3.2 Invoke the reusable quality gate for pull requests and every main-branch commit and verify required checks reach terminal success/failure.
- [x] 3.3 Remove the unused GNU target installation unless a separate job explicitly builds and validates it.

## 4. Release Hardening

- [x] 4.1 Make release validation run/depend on the reusable quality gate for the exact tag commit before artifact creation.
- [x] 4.2 Preserve and test the tag-to-`Cargo.toml` version guard and existing release scripts.
- [x] 4.3 Compute and upload a SHA-256 checksum alongside the exact executable and verify it before publication.
- [x] 4.4 Pin every release-critical third-party action to a full commit SHA with an upstream-version comment.
- [x] 4.5 Document code signing, SBOM, and provenance as explicitly deferred follow-ups rather than implying they are present.

## 5. Verification

- [x] 5.1 Validate workflow YAML and run the full local equivalents for format, clippy, debug/release tests, release build, and dependency policy.
- [x] 5.2 Exercise PR, acceptable-main, rejected-main, failing-check, and tag/release paths and confirm no required gate is silently skipped.
  - PR path: verified via PR #10 — all required checks reached terminal success.
  - Acceptable-main path: verified via the PR #10 merge commit's push-triggered `CI` run — terminal success.
  - Rejected-main path: verified via an intentionally unverified merge commit pushed directly to `main` (reverted immediately after) — `commit-policy` correctly failed with "Unverified merge commits are not accepted on main.", and the aggregate `CI` job correctly failed overall even though `Quality Gate` passed on its own.
  - Failing-check path: verified via a disposable draft PR (#11, closed without merging) with a deliberate formatting violation — `Quality / Format` failed, `Quality Gate` and `CI` both correctly failed rather than skipping/passing.
  - Tag/release path: verified via the real `v0.1.9` release run (`33244866419`) for commit `14c11309e414910cea5ace191b5dcb3c3c4f024f`; tag validation, all quality checks, the aggregate gate, and publication completed successfully. This repository has no separate non-publishing tag dry-run because every `v*` tag invokes the publishing workflow.
- [x] 5.3 Complete a non-production release rehearsal or next real release and verify the executable/checksum pair and immutable action references before marking the change done.
  - Verified by the real `v0.1.9` release: the exact executable and `.sha256` pair were published, and the downloaded executable matched the published digest `5d592489c9a7cab850b004eafd2e63f511e870685b49be7d5248edbe8a0b5dfe` byte-for-byte. The release-critical action references in `release.yml` are full commit SHAs with upstream-version comments. A separate non-production rehearsal remains unavailable by design because `release.yml` only triggers on `v*` tag pushes and publishes after validation.

