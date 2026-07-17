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
gradient fill, and rounded corners on the raised cards floating within a
directory — rather than a lightness shift alone or additional clashing
hues, so structure reads clearly without visual noise. Directory blocks
SHALL render as a frame: a border stroke and a faint fill tint derived from
a hash of the directory's own name, constrained to a muted saturation/value
band distinct from the file color band, with a header strip carrying its
name; their children SHALL render as raised cards packed flush against that
frame, so elevation and per-directory color together communicate
container-versus-content structure rather than being applied uniformly with
no meaning.

#### Scenario: Nested blocks are distinguishable from their parent by depth
- **WHEN** a folder block contains nested child blocks at a deeper level
- **THEN** the child blocks render as raised cards (shadow, gradient,
  rounded corners) packed flush within their parent directory's frame, not
  merely a lighter flat fill

#### Scenario: Directories read as containers, not just differently-colored blocks
- **WHEN** a directory block is rendered
- **THEN** it shows a header strip carrying its name and a bordered frame,
  tinted with a muted hue derived from its own name, distinct from the
  raised appearance of its children

#### Scenario: Sibling directories are distinguishable by their frame color
- **WHEN** two sibling directory blocks have different names
- **THEN** their frame border/tint hues differ, while both stay within the
  same muted saturation/value band reserved for directories

#### Scenario: Chrome shares the same elevation language as the treemap, scaled to its own size
- **WHEN** the toolbar or breadcrumb bar is rendered
- **THEN** its interactive elements (buttons, path field, breadcrumb links)
  use the same gradient/shadow/radius treatment as treemap blocks, but with
  shadow blur and offset tuned to chrome's own (smaller) element scale
  rather than reusing the block-scale shadow values unscaled, so the shadow
  reads as a subtle lift rather than a distinct, doubled-looking shape at
  chrome's typical element size

### Requirement: Single-child directory chains collapse into one header
The system SHALL, when rendering a run of consecutive directories each
having exactly one child directory, combine their headers into a single
header showing the full joined path, and render the frame around the first
directory in the run that has zero children, more than one child, or is a
file — rather than rendering one stacked full-width header per level.

#### Scenario: A chain of single-child directories renders as one header
- **WHEN** a directory has exactly one child, which itself has exactly one
  child, continuing until reaching a directory with more than one child (or
  a leaf)
- **THEN** the treemap shows a single combined header naming every
  directory in that chain, followed immediately by the frame and contents
  of the first directory that actually branches

#### Scenario: A directory with more than one child does not collapse
- **WHEN** a directory has more than one child, even if one of those
  children is much smaller than the other
- **THEN** that directory renders its own header and frame rather than
  being absorbed into a combined header

#### Scenario: Clicking a combined header drills through the whole chain
- **WHEN** the user clicks anywhere on a combined header representing a
  chain of single-child directories
- **THEN** the view drills directly to the first directory in that chain
  that actually branches, in one click, rather than requiring one click per
  collapsed level

#### Scenario: Nesting depth counts the collapsed chain once
- **WHEN** a run of single-child directories is collapsed into one combined
  header
- **THEN** the nesting depth used for elevation/lightness purposes advances
  by one level for the whole chain, not once per collapsed directory

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
