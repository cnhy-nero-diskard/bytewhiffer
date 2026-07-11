# theming

## Purpose
Defines Bytewhiffer's visual language: a dark base theme, deterministic
per-extension block coloring, depth communicated via elevation rather than
hue, and a single reserved accent color for interactive/selection state.

## Requirements

### Requirement: Dark base theme
The system SHALL use a dark, near-black/charcoal-navy base background (in the
`#0d1117`-`#0f1420` family) rather than pure black, for the app's overall
chrome and treemap background.

#### Scenario: App background reads as dark, not pure black
- **WHEN** the app is displayed
- **THEN** its background color is a dark charcoal/navy tone rather than pure
  black (`#000000`) or a light theme color

### Requirement: Deterministic color-from-extension mapping
The system SHALL assign each treemap block a color derived deterministically
from its file extension (or a defined fallback for extensionless/directory
entries), using a hash-derived hue constrained to a fixed saturation/
lightness band, so that the same extension always renders the same color and
the overall palette feels curated rather than random.

#### Scenario: The same extension always yields the same color
- **WHEN** two different blocks represent files with the same extension
- **THEN** both blocks are rendered with the same color

#### Scenario: Different extensions are visually distinguishable
- **WHEN** blocks represent files with different extensions
- **THEN** their assigned colors differ in hue while staying within the same
  constrained saturation/lightness band

### Requirement: Depth communicated via elevation shift
The system SHALL communicate nesting depth in the treemap via a subtle
elevation or lightness shift rather than introducing additional clashing
hues, so structure reads clearly without visual noise.

#### Scenario: Nested blocks are distinguishable from their parent by depth
- **WHEN** a folder block contains nested child blocks at a deeper level
- **THEN** the child blocks' rendering reflects their depth via a lightness/
  elevation shift, not a hue change unrelated to their file type

### Requirement: Reserved accent color
The system SHALL reserve a single vivid accent color, used only for hover
state, the current breadcrumb entry, and selection, so that it stands out as
the one visually "alive" element in the UI.

#### Scenario: Accent color appears only on interactive/selection state
- **WHEN** no block is hovered, selected, or part of the active breadcrumb
- **THEN** the accent color is not applied anywhere else in the treemap
  rendering

#### Scenario: Hover, breadcrumb, and selection all use the same accent color
- **WHEN** a block is hovered, is the active breadcrumb entry, or is
  selected
- **THEN** it is rendered using the same single reserved accent color in each
  case
