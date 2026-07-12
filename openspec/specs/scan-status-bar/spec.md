# scan-status-bar

## Purpose
Defines the toolbar's Rescan control, the in-flight scan HUD (indeterminate progress plus live metrics), and the persistent bottom status bar — the chrome that keeps the user informed while a scan runs and after it finishes, without duplicating figures between the two.

## Requirements

### Requirement: Rescan control
The system SHALL provide a toolbar control that re-runs a scan against the currently scanned root path, without requiring the user to re-select or re-type it.

#### Scenario: Rescan re-runs against the current root
- **WHEN** a scan has completed (or been cancelled) for a given root path and the user activates the Rescan control
- **THEN** a new scan starts against that same root path, without the user re-entering it

#### Scenario: Rescan is unavailable before any scan has run
- **WHEN** no scan has ever completed or started in the current session
- **THEN** the Rescan control is not available, since there is no root path yet to rescan

### Requirement: In-flight scan HUD
The system SHALL display, only while a scan is in progress, an indeterminate progress indicator and live metrics: files scanned, directories scanned, bytes scanned, scan rate, elapsed time, and the largest top-level item discovered so far.

#### Scenario: HUD becomes visible when a scan starts
- **WHEN** a scan starts
- **THEN** the HUD row is displayed above the treemap

#### Scenario: HUD disappears when a scan ends
- **WHEN** a scan completes or is cancelled
- **THEN** the HUD row is no longer displayed

#### Scenario: Progress indicator does not claim a completion percentage
- **WHEN** a scan is in progress
- **THEN** the progress indicator animates to show activity without displaying or implying any percentage of completion

#### Scenario: Live metrics update as the scan streams
- **WHEN** a scan is in progress and new entries are discovered
- **THEN** the displayed files-scanned, directories-scanned, and bytes-scanned figures reflect the latest counts without waiting for the scan to finish

#### Scenario: Biggest top-level item updates as larger items are found
- **WHEN** a scan is in progress and a top-level child of the scan root grows to exceed the previously displayed largest item
- **THEN** the HUD updates to show that child's name and size

### Requirement: Persistent bottom status bar
The system SHALL display a status bar, always present regardless of scan state, showing hover information on one side and scan/engine information on the other, with the scan information surviving past scan completion.

#### Scenario: Hovering a block shows its path and size in the status bar
- **WHEN** the pointer hovers a treemap block
- **THEN** the status bar displays that block's full path and size

#### Scenario: Status bar shows a neutral placeholder when nothing is hovered
- **WHEN** no block is currently hovered
- **THEN** the status bar's hover section shows a neutral placeholder rather than stale information from a previous hover

#### Scenario: Scan summary is shown after a scan completes and persists
- **WHEN** a scan completes
- **THEN** the status bar displays the completed scan's file count, directory count, byte total, and elapsed time, and continues to display them afterward rather than only momentarily

#### Scenario: Scan summary is quiet while a scan is in progress
- **WHEN** a scan is in progress
- **THEN** the status bar's scan-summary section does not display the running live counts already shown in the HUD, showing a neutral in-progress indicator instead, so the same figures are never shown in two places at once

#### Scenario: Active engine name is displayed
- **WHEN** a scan has run or is running
- **THEN** the status bar displays the name of the scan engine that produced, or is producing, the result
