## MODIFIED Requirements

### Requirement: Top-extensions-by-size breakdown
The system SHALL display the total size contributed by each distinct file
extension across the focused subtree, ordered largest first, using each
extension's legend color, and SHALL render a proportional fill bar behind
each entry, sized to that extension's share of the focused subtree's total
size.

#### Scenario: Breakdown reflects aggregate size per extension
- **WHEN** the focused subtree contains files of multiple extensions
- **THEN** the breakdown shows one entry per extension with its combined
  size across all matching files in the subtree, sorted largest to
  smallest

#### Scenario: Fill bar length reflects share of the focused subtree
- **WHEN** an extension's total size is some fraction of the focused
  subtree's total size
- **THEN** that entry's fill bar spans a proportional fraction of the
  row's width, rendered in a flat neutral fill color rather than the
  extension's own swatch color

#### Scenario: Fill bars rescale when the focus changes
- **WHEN** the user drills into or out of a directory, changing the
  focused subtree
- **THEN** every fill bar's length recomputes against the newly focused
  subtree's total size, rather than continuing to reflect the previous
  focus

### Requirement: Biggest files/folders leaderboard
The system SHALL display a ranked list of the largest files and folders
(by size) within the focused subtree. Activating an entry SHALL focus the
treemap on that entry's path. The system SHALL render a proportional fill
bar behind each entry, sized to that entry's share of the focused
subtree's total size.

#### Scenario: Leaderboard is ranked by size
- **WHEN** the drawer is open and showing the leaderboard
- **THEN** entries are ordered largest to smallest by size

#### Scenario: Activating a leaderboard entry navigates the treemap
- **WHEN** the user activates a leaderboard entry
- **THEN** the treemap's focus changes to that entry's path, the same way
  clicking the corresponding block would

#### Scenario: Fill bar length reflects share of the focused subtree
- **WHEN** a leaderboard entry's size is some fraction of the focused
  subtree's total size
- **THEN** that entry's fill bar spans a proportional fraction of the
  row's width, using the same flat neutral fill color as the File types
  section's bars

#### Scenario: Fill bars rescale when the focus changes
- **WHEN** the user drills into or out of a directory, changing the
  focused subtree
- **THEN** every leaderboard fill bar's length recomputes against the
  newly focused subtree's total size
