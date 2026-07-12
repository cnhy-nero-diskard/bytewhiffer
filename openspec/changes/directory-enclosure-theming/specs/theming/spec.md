## MODIFIED Requirements

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

#### Scenario: Chrome shares the same elevation language as the treemap
- **WHEN** the toolbar or breadcrumb bar is rendered
- **THEN** its interactive elements (buttons, path field, breadcrumb links)
  use the same gradient/shadow/radius treatment as treemap blocks, scaled
  appropriately, rather than stock unstyled widget visuals

## ADDED Requirements

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
