## ADDED Requirements

### Requirement: Tooltip content stays legible at any anchor position
The system SHALL bound the hover tooltip's displayed path text to a fixed-width, single-line representation — eliding the middle of long paths rather than allowing them to wrap — so the tooltip remains legible regardless of where the popup is anchored on screen.

#### Scenario: A long path does not wrap into an illegible column
- **WHEN** the hovered block's full trail (joined path) exceeds the tooltip's display width
- **THEN** the tooltip shows a single-line, middle-elided version of the trail rather than wrapping it across multiple narrow lines

#### Scenario: Tooltip stays single-line near a viewport edge
- **WHEN** the hovered block is near a screen edge and the popup is repositioned to stay within the viewport
- **THEN** the tooltip's text still renders as a single legible line, not a narrow multi-line column

#### Scenario: The full path remains available elsewhere
- **WHEN** the tooltip shows an elided version of a long path
- **THEN** the complete, unelided path is still available via the persistent status bar's hover readout

### Requirement: Tooltip tracks the pointer responsively regardless of scene density
The system SHALL keep the hover tooltip's on-screen position visually synchronized with the pointer during ordinary pointer movement, including over directories with many simultaneously visible blocks, rather than allowing per-frame rendering cost to grow unbounded with the number of visible blocks.

#### Scenario: Tooltip follows the pointer over a dense directory
- **WHEN** the user hovers over blocks while moving the pointer smoothly across a directory with many simultaneously visible card-eligible blocks (e.g. a directory with hundreds of similarly-sized files)
- **THEN** the tooltip's displayed position tracks the pointer without perceptible lag

#### Scenario: Tooltip follows the pointer over a sparse directory
- **WHEN** the user hovers over blocks in a directory with few visible blocks
- **THEN** the tooltip tracks the pointer at least as responsively as in the dense case, since the sparse case is never more expensive to render
