# treemap-navigation

## Purpose
Defines how the treemap view behaves as a live, interactive UI: rendering
progressively as a scan runs, letting the user drill into folders and
navigate back out via breadcrumbs, and giving hover feedback.

## Requirements

### Requirement: Live scan rendering
The system SHALL render the treemap so that it fills in and grows while a
background scan is still in progress, rather than showing a blank screen
until the scan reaches 100%.

#### Scenario: Treemap updates while a scan is still running
- **WHEN** a scan is in progress and has partial results available
- **THEN** the rendered treemap reflects the partial tree scanned so far and
  continues to update as more of the tree is discovered

#### Scenario: Treemap reflects a stable final layout once scanning completes
- **WHEN** a scan finishes
- **THEN** the treemap renders the complete, final tree and stops changing
  due to scan progress

### Requirement: Click-to-drill navigation
The system SHALL let a user click a folder block to zoom into just that
folder's contents, replacing the current view with a treemap of that
folder's children.

#### Scenario: Clicking a folder block drills into it
- **WHEN** the user clicks a block representing a folder
- **THEN** the view changes to show a treemap of that folder's direct
  contents, sized relative to each other

#### Scenario: Clicking a file block does not drill in
- **WHEN** the user clicks a block representing a file (not a folder)
- **THEN** the current view does not change to a drilled-in state, since a
  file has no children to zoom into

### Requirement: Breadcrumb / back navigation
The system SHALL provide a way to navigate back out of a drilled-in view,
either to the immediate parent or to any ancestor in the current navigation
path, via a breadcrumb or back control.

#### Scenario: Back control returns to the parent view
- **WHEN** the user is in a drilled-in view and activates the back control
- **THEN** the view returns to showing the immediate parent folder's treemap

#### Scenario: Breadcrumb allows jumping to any ancestor
- **WHEN** the user is several levels deep in drilled-in navigation and
  selects an ancestor from the breadcrumb
- **THEN** the view jumps directly to that ancestor's treemap, skipping
  intermediate levels

### Requirement: Hover feedback
The system SHALL visually indicate which block is currently under the
pointer.

#### Scenario: Hovering a block highlights it
- **WHEN** the pointer moves over a treemap block
- **THEN** that block is visually distinguished (e.g. via the reserved accent
  color) from non-hovered blocks
