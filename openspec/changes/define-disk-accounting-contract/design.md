## Context

Walker uses logical metadata length and sees directory entries independently. Turbo reads one unnamed `$DATA` real size and chooses one MFT name. Sparse, compressed, and hard-linked files can therefore produce totals that neither represent physical allocation consistently nor agree between engines.

## Goals / Non-Goals

**Goals:**

- Make unique physical allocation the authoritative treemap metric.
- Preserve logical length as clearly secondary metadata.
- Deduplicate by stable file identity with deterministic directory attribution.
- Give Walker and Turbo one shared finalization contract and comparable fixtures.

**Non-Goals:**

- Account for filesystem metadata, journal, free-space fragmentation, or alternate data streams in the first version.
- Promise a transactionally consistent snapshot while the filesystem is changing.
- Support non-Windows allocation APIs beyond the existing compile/test stubs.

## Decisions

**Use typed size metrics.** Scanner entries carry at least `logical_bytes` and `allocated_bytes`; aggregate `size` ambiguity is removed or wrapped in a named accounting type. Treemap area, directory rollups, scan totals, and Insights use `allocated_bytes`. Tooltips may show both values with explicit labels.

**Define identity independently of path.** On Windows, Walker derives a file identity from volume identity plus file index obtained without following reparse targets. Turbo uses the MFT file reference with its sequence number and volume identity. Identity is never inferred from name or path.

**Retain every discovered hard-link path but charge allocation once.** A shared finalization pass groups entries by identity and assigns the file's allocated bytes to the lexicographically smallest normalized relative path, compared case-insensitively with a stable case-sensitive tie break. Other links remain aliases with zero accounted bytes and retain logical-size/alias metadata. This yields deterministic totals regardless of Rayon scheduling or MFT attribute ordering.

**Use Windows allocated-size semantics.** Walker obtains the file's actual allocated byte count through the appropriate Windows file-information API, including sparse/compressed behavior. Turbo reads the unnamed `$DATA` allocated-size field rather than real/logical size. Invalid or unavailable allocation metadata produces a typed engine limitation/failure rather than silently substituting logical length.

**Centralize rollup after deduplication.** Both engines produce identity-bearing leaf records and pass them through the same deterministic deduplication and directory-rollup logic. This avoids duplicating accounting policy in Walker and MFT reconstruction.

## Risks / Trade-offs

- **Alias entries with zero accounted area may not render as blocks** → Preserve alias metadata for details/testing and document that the treemap visualizes unique allocation, not every directory entry.
- **Filesystem mutation can change identity or size between probes** → Treat scans as best-effort snapshots and compare engines on quiescent fixtures.
- **Windows APIs may differ across filesystem types** → Gate the strict contract on supported local Windows filesystems and return explicit limitations elsewhere.
- **Existing users see changed totals** → Update wording and release notes; never label old/new values identically.

## Migration Plan

Add typed metrics and identity to `Entry`, implement the shared finalization pass, update Walker and Turbo producers, then switch UI consumers from ambiguous size fields. Keep compatibility helper methods temporarily while compilation guides remaining call sites. No persisted data migration exists; rollback restores prior runtime semantics.

## Open Questions

None blocking. Exact Windows API selection is an implementation lookup and must be proven by sparse/compressed fixture tests.
