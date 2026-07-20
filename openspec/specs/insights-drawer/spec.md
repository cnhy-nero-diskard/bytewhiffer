# insights-drawer Specification

## Purpose

The Insights drawer is a left-side panel that surfaces derived, at-a-glance analysis of the currently focused subtree — extension color legend, size breakdowns, leaderboards, and heuristic flags — so users can understand what's consuming space without manually exploring the treemap block by block.

## Requirements

### Requirement: Insights drawer toggle
The system SHALL provide a toolbar control that opens and closes a left-side Insights drawer. The drawer SHALL be closed by default so the treemap occupies the full available width until the user opens it.

#### Scenario: Drawer is closed by default
- **WHEN** the app starts
- **THEN** the Insights drawer is not shown and the treemap occupies the full central area

#### Scenario: Opening the drawer narrows the treemap, not vice versa
- **WHEN** the user activates the toolbar toggle while the drawer is closed
- **THEN** the Insights drawer appears on the left and the treemap's available width shrinks to make room

#### Scenario: Closing the drawer restores full width
- **WHEN** the user activates the toolbar toggle while the drawer is open
- **THEN** the Insights drawer disappears and the treemap returns to occupying the full central area

#### Scenario: No scan yet
- **WHEN** the drawer is open and no scan has ever completed
- **THEN** the drawer shows a neutral placeholder instead of empty or stale insight sections

### Requirement: Insights scope to the focused subtree
Every insight in the drawer SHALL describe the currently focused node (the same node the treemap is currently rendering the children of), not always the whole scan root, and SHALL recompute when the focus changes or the tree itself changes.

#### Scenario: Drilling into a directory updates the drawer
- **WHEN** the user drills into a directory (via block click or breadcrumb) while the drawer is open
- **THEN** every insight section recomputes to describe that directory's subtree

#### Scenario: A completing or live-updating scan refreshes insights
- **WHEN** the drawer is open and the underlying tree changes (new entries discovered during a live scan, or a scan completes)
- **THEN** the insight sections reflect the updated tree rather than a stale snapshot

### Requirement: Extension color legend
The system SHALL display, for every distinct file extension present in the focused subtree, the same color the treemap assigns that extension's blocks.

#### Scenario: Legend colors match treemap blocks
- **WHEN** the drawer is open and showing the legend
- **THEN** each listed extension's swatch color is identical to the color rendered on that extension's blocks in the treemap

#### Scenario: Extensionless files get a legend entry
- **WHEN** the focused subtree contains files with no extension
- **THEN** the legend includes a single entry for them using their shared fallback color

### Requirement: Top-extensions-by-size breakdown
The system SHALL display the total size contributed by each distinct file extension across the focused subtree, ordered largest first, using each extension's legend color, and SHALL render a proportional fill bar behind each entry, sized to that extension's share of the focused subtree's total size.

#### Scenario: Breakdown reflects aggregate size per extension
- **WHEN** the focused subtree contains files of multiple extensions
- **THEN** the breakdown shows one entry per extension with its combined size across all matching files in the subtree, sorted largest to smallest

#### Scenario: Fill bar length reflects share of the focused subtree
- **WHEN** an extension's total size is some fraction of the focused subtree's total size
- **THEN** that entry's fill bar spans a proportional fraction of the row's width, rendered in a flat neutral fill color rather than the extension's own swatch color

#### Scenario: Fill bars rescale when the focus changes
- **WHEN** the user drills into or out of a directory, changing the focused subtree
- **THEN** every fill bar's length recomputes against the newly focused subtree's total size, rather than continuing to reflect the previous focus

### Requirement: Biggest files/folders leaderboard
The system SHALL display a ranked list of the largest files and folders (by size) within the focused subtree. Activating an entry SHALL focus the treemap on that entry's path. The system SHALL render a proportional fill bar behind each entry, sized to that entry's share of the focused subtree's total size.

#### Scenario: Leaderboard is ranked by size
- **WHEN** the drawer is open and showing the leaderboard
- **THEN** entries are ordered largest to smallest by size

#### Scenario: Activating a leaderboard entry navigates the treemap
- **WHEN** the user activates a leaderboard entry
- **THEN** the treemap's focus changes to that entry's path, the same way clicking the corresponding block would

#### Scenario: Fill bar length reflects share of the focused subtree
- **WHEN** a leaderboard entry's size is some fraction of the focused subtree's total size
- **THEN** that entry's fill bar spans a proportional fraction of the row's width, using the same flat neutral fill color as the File types section's bars

#### Scenario: Fill bars rescale when the focus changes
- **WHEN** the user drills into or out of a directory, changing the focused subtree
- **THEN** every leaderboard fill bar's length recomputes against the newly focused subtree's total size

### Requirement: Small-file-blizzard flag
The system SHALL flag directories within the focused subtree that have a high child count but a low average child size, surfacing them as a distinct list in the drawer.

#### Scenario: A directory with many small children is flagged
- **WHEN** the focused subtree contains a directory whose child count is high and whose average child size is low relative to other directories in the subtree
- **THEN** that directory appears in the blizzard-flag list

#### Scenario: Directories without that pattern are not flagged
- **WHEN** the focused subtree contains directories with few children or large average child size
- **THEN** those directories do not appear in the blizzard-flag list

### Requirement: Known-junk suggestions
The system SHALL flag files and directories within the focused subtree whose names match common junk patterns (e.g. installers, build caches, `node_modules`, browser cache directories), and SHALL make the existing Delete/Open/Reveal actions available for a flagged entry without introducing a new action mechanism.

#### Scenario: A recognized junk directory is flagged
- **WHEN** the focused subtree contains a directory whose name matches a known junk pattern
- **THEN** that directory appears in the junk-suggestions list

#### Scenario: Acting on a flagged entry reuses existing actions
- **WHEN** the user chooses to act on a junk-suggestion entry
- **THEN** the same Delete, Open, and Reveal-in-Explorer actions available from the treemap's context menu are available for that entry

#### Scenario: Junk suggestions are advisory, not automatic
- **WHEN** entries are flagged as junk suggestions
- **THEN** no file or directory is deleted or modified without the user explicitly choosing an action on that entry
