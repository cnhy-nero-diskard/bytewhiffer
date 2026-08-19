## ADDED Requirements

### Requirement: Required CI fails closed
Every pull request and main-branch commit SHALL reach an explicit required quality-gate result. Commit-policy rejection or failure of a required check MUST fail the gate and MUST NOT be represented as successful by skipping Test or Build jobs.

#### Scenario: Commit policy accepts the revision
- **WHEN** the revision satisfies configured commit policy
- **THEN** all required validation jobs run and the aggregate gate reflects their results

#### Scenario: Commit policy rejects a main revision
- **WHEN** a main-branch revision violates configured commit policy
- **THEN** CI reports a failing required check rather than skipped validation

#### Scenario: Required dependency is skipped unexpectedly
- **WHEN** any required validation job does not run for an unexpected reason
- **THEN** the aggregate quality gate fails

### Requirement: Toolchain and Windows target are reproducible
The repository SHALL pin an exact Rust toolchain with required components and SHALL use an explicitly documented Windows target consistently in CI and release builds.

#### Scenario: CI initializes Rust
- **WHEN** a quality job starts
- **THEN** it installs the committed toolchain, rustfmt, clippy, and intended Windows target rather than floating `stable`

#### Scenario: Release binary is built
- **WHEN** the release workflow compiles Bytewhiffer
- **THEN** it uses the same explicit toolchain and target validated by the quality gate

### Requirement: Quality gate covers formatting, lint, tests, and build
The required quality gate SHALL run `cargo fmt --all -- --check`, clippy for all targets with warnings denied, debug tests, release tests, and an explicit release build.

#### Scenario: Any baseline command fails
- **WHEN** formatting, clippy, debug tests, release tests, or release build exits unsuccessfully
- **THEN** the aggregate quality gate fails and release publication is blocked

### Requirement: Dependency policy is enforced
CI SHALL enforce committed advisory, license, banned-dependency, and source policies with pinned tooling. Any exception MUST identify its narrow scope and rationale.

#### Scenario: Disallowed dependency condition is detected
- **WHEN** the dependency-policy tool finds an unapproved advisory, license, ban, or source violation
- **THEN** the quality gate fails

#### Scenario: Approved temporary exception exists
- **WHEN** a narrowly documented exception matches the detected condition
- **THEN** policy evaluation applies only that exception and continues enforcing all other rules

### Requirement: Release validates the exact tag commit before publication
The release workflow SHALL run or depend on the trusted quality gate for the exact tag commit and SHALL preserve the existing tag-to-`Cargo.toml` version guard.

#### Scenario: Tag commit passes all checks
- **WHEN** the exact tag commit passes version alignment and the full quality gate
- **THEN** release artifact creation may proceed

#### Scenario: Tag commit lacks a passing gate
- **WHEN** any required check for the exact tag commit fails or is absent
- **THEN** no GitHub Release artifact is published

### Requirement: Published artifacts include integrity metadata
The release workflow SHALL publish a SHA-256 checksum for each executable and SHALL pin release-critical third-party actions to immutable commit SHAs.

#### Scenario: Executable is published
- **WHEN** a release uploads the Bytewhiffer executable
- **THEN** it also uploads a checksum file whose digest matches the exact executable bytes

#### Scenario: Release workflow references third-party action
- **WHEN** checkout, toolchain, cache, or publishing actions are used on the release path
- **THEN** each release-critical reference uses a full immutable commit SHA with its upstream version documented

