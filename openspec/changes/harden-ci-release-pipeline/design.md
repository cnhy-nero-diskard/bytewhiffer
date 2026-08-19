## Context

`ci.yml` computes a `proceed` output and skips Test/Build when a main commit is neither a direct push nor verified merge, which can leave an unacceptable commit without a failing quality signal. Both CI and release install a GNU target but invoke Cargo without `--target`, effectively using the runner default. Release builds and publishes without the full test/lint gate or checksums.

## Goals / Non-Goals

**Goals:**

- Make every required CI path end in explicit success or failure.
- Pin and use one intended Windows toolchain/target consistently.
- Reuse the exact quality gate before release publication.
- Publish checksums and pin release-critical actions immutably.
- Enforce an explicit dependency advisory/license/source policy.

**Non-Goals:**

- Add code signing, SBOM attestation, or multi-architecture binaries in this change.
- Change semver/tag scripts or the tag-to-Cargo-version guard.
- Require tests that need interactive UAC or raw-volume hardware in hosted CI.

## Decisions

**Use explicit Windows MSVC builds.** Pin an exact Rust stable release in `rust-toolchain.toml` with `rustfmt`, `clippy`, and `x86_64-pc-windows-msvc`. Every build/test command passes or inherits that documented target consistently; remove the unused GNU target unless a separate job genuinely builds it.

**Replace skip gating with fail-closed jobs.** If commit-policy validation remains, an unacceptable main commit exits non-zero. A final required aggregate job depends on formatting, lint, dependency policy, debug tests, release tests, and release build, and fails when any required dependency fails or is skipped unexpectedly.

**Factor a reusable quality workflow.** A `workflow_call` quality workflow checks out the requested exact SHA and runs the same pinned commands for PR/push CI and releases. Release calls it for the tag commit and publishes only after it succeeds.

**Use `cargo-deny` with pinned tooling/config.** Policy covers advisories, licenses, bans, and sources. Exceptions require narrow identifiers, rationale, and review dates in configuration. Tool installation/action versions are pinned.

**Pin actions and emit integrity metadata.** Release-critical actions use full commit SHAs with comments noting upstream versions. The workflow computes a SHA-256 file next to the executable and uploads both. Existing tag/version validation runs before publication.

## Risks / Trade-offs

- **More checks increase CI time** → Use Rust caching and parallel jobs while keeping the aggregate gate authoritative.
- **Pinned versions need maintenance** → Dependabot/Renovate or scheduled review can propose controlled updates.
- **Advisory feeds can create urgent failures** → Document narrow temporary exceptions; never silently disable the policy.
- **Reusable-workflow permissions can drift** → Declare least permissions per job and test pull-request and tag paths.

## Migration Plan

Add toolchain/policy files, introduce reusable quality jobs, make CI call them, then gate release on the same workflow and add checksums/action pins. Validate workflow syntax and exercise a non-publishing tag-like path before the first real release. Rollback is reverting workflow/config files; existing scripts remain intact.

## Open Questions

None blocking. Pin the newest compiler/tool versions that pass the repository at implementation time and record them in committed configuration.
