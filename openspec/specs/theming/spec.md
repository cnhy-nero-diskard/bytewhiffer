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

#### Scenario: Chrome shares the same elevation language as the treemap, scaled to its own size
- **WHEN** the toolbar or breadcrumb bar is rendered
- **THEN** its interactive elements (buttons, path field, breadcrumb links)
  use the same gradient/shadow/radius treatment as treemap blocks, but with
  shadow blur and offset tuned to chrome's own (smaller) element scale
  rather than reusing the block-scale shadow values unscaled, so the shadow
  reads as a subtle lift rather than a distinct, doubled-looking shape at
  chrome's typical element size
