## MODIFIED Requirements

### Requirement: In-flight scan HUD
The system SHALL display, while a scan is in progress or its authoritative tree is still being assembled, an indeterminate progress indicator and live metrics: files scanned, directories scanned, bytes scanned, scan rate, elapsed time, and the largest top-level item discovered so far. Elapsed time SHALL continue to update live for as long as either phase is still active. Once the walk phase finishes and only tree assembly remains, the HUD SHALL switch to a distinct "finishing up" state showing genuine completion progress (assembly's total item count is known up front, unlike the open-ended walk phase).

#### Scenario: HUD becomes visible when a scan starts
- **WHEN** a scan starts
- **THEN** the HUD row is displayed above the treemap

#### Scenario: HUD remains visible while tree assembly is still in progress
- **WHEN** the scan engine has finished walking and returned its result, but the authoritative tree is still being assembled into the displayed tree
- **THEN** the HUD row remains displayed, showing the assembly-phase state rather than disappearing

#### Scenario: HUD disappears only once assembly also completes
- **WHEN** a scan completes and the authoritative tree has finished assembling and swapped in, or the scan is cancelled
- **THEN** the HUD row is no longer displayed

#### Scenario: Progress indicator does not claim a completion percentage during the walk phase
- **WHEN** the scan engine is still walking (discovering entries)
- **THEN** the progress indicator animates to show activity without displaying or implying any percentage of completion

#### Scenario: Progress indicator shows real completion progress during the assembly phase
- **WHEN** the walk phase has finished and the authoritative tree is being assembled
- **THEN** the HUD shows a completion fraction or percentage reflecting how much of the known total assembly work remains, rather than an indeterminate animation

#### Scenario: Live metrics update as the scan streams
- **WHEN** a scan is in progress and new entries are discovered
- **THEN** the displayed files-scanned, directories-scanned, and bytes-scanned figures reflect the latest counts without waiting for the scan to finish

#### Scenario: Elapsed time keeps ticking live through both phases
- **WHEN** a scan is walking or its authoritative tree is still assembling
- **THEN** the displayed elapsed time keeps advancing live, at sub-second precision, rather than freezing once the walk phase finishes

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

#### Scenario: Scan summary is shown only once the scan is fully done, and persists
- **WHEN** a scan's walk phase completes and its authoritative tree finishes assembling and swaps in
- **THEN** the status bar displays the completed scan's file count, directory count, byte total, and elapsed time — where the elapsed time reflects the full duration including assembly, not just the walk — and continues to display them afterward rather than only momentarily

#### Scenario: Scan summary is quiet while a scan or its tree assembly is in progress
- **WHEN** a scan is walking, or its authoritative tree is still assembling
- **THEN** the status bar's scan-summary section does not display a final-looking summary or the running live counts already shown in the HUD, so the same figures are never shown in two places at once and nothing is presented as finished before it is

#### Scenario: Active engine name is displayed
- **WHEN** a scan has run or is running
- **THEN** the status bar displays the name of the scan engine that produced, or is producing, the result
