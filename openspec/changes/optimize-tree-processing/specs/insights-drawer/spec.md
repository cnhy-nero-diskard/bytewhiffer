## ADDED Requirements

### Requirement: Insights aggregation is bounded and revision-aware
The system SHALL compute extension totals, leaderboard entries, blizzard flags, and cleanup candidates from one traversal of the focused tree per relevant structural/focus revision. Leaderboard selection MUST retain at most bounded top-N candidate state rather than every descendant.

#### Scenario: Multiple Insight sections refresh
- **WHEN** the focused tree revision changes
- **THEN** all Insight sections are recomputed from one traversal and remain equivalent to their specified results

#### Scenario: Pointer-only frame
- **WHEN** pointer movement causes a repaint without focus or tree revision changes
- **THEN** the cached Insights result is reused without a subtree traversal

#### Scenario: Large leaderboard population
- **WHEN** the focused subtree has substantially more descendants than the leaderboard limit
- **THEN** the correct largest entries are returned with bounded top-N retention and deterministic tie ordering

