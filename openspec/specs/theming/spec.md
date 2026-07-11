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
The system SHALL communicate nesting depth in the treemap and in the app's
chrome via real elevation — a drop shadow, a top-lighter/bottom-darker
gradient fill, and rounded corners — rather than a lightness shift alone or
additional clashing hues, so structure reads clearly without visual noise.
Directory blocks SHALL render as a recessed tray (an inset shadow) with a
header strip carrying their name; their children SHALL render as raised
cards floating above that tray, so elevation communicates container-versus-
content structure rather than being applied uniformly with no meaning.

#### Scenario: Nested blocks are distinguishable from their parent by depth
- **WHEN** a folder block contains nested child blocks at a deeper level
- **THEN** the child blocks render as raised cards (shadow, gradient,
  rounded corners) sitting above their parent directory's recessed tray,
  not merely a lighter flat fill

#### Scenario: Directories read as containers, not just differently-colored blocks
- **WHEN** a directory block is rendered
- **THEN** it shows a header strip carrying its name and a visibly recessed
  (inset-shadowed) body region distinct from the raised appearance of its
  children

#### Scenario: Chrome shares the same elevation language as the treemap
- **WHEN** the toolbar or breadcrumb bar is rendered
- **THEN** its interactive elements (buttons, path field, breadcrumb links)
  use the same gradient/shadow/radius treatment as treemap blocks, scaled
  appropriately, rather than stock unstyled widget visuals

### Requirement: Minimum-size flat fallback
The system SHALL render blocks and chrome elements below a defined minimum
on-screen size using flat fill only — no drop shadow, no gradient, no
rounded corners, and no inter-block gap — identical to the pre-elevation
rendering, so that dense clusters of small elements remain legible and
inexpensive to render rather than degrading into visual noise.

#### Scenario: A dense cluster of small file blocks stays legible
- **WHEN** a directory contains many sibling files small enough on screen
  that each block falls below the minimum-size threshold
- **THEN** those blocks render with flat fill and a plain border, without
  shadow, gradient, or rounding, rather than each carrying its own
  elevation treatment

#### Scenario: A block crossing the threshold as it's resized gains or loses elevation
- **WHEN** a block's on-screen size changes (e.g. via window resize or
  navigating to a different focus depth) so that it crosses the minimum-size
  threshold
- **THEN** its rendering switches between the flat fallback and the full
  elevation treatment accordingly, with no intermediate or broken state

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
