# block-labels

## Purpose
Defines how the treemap paints name and size labels directly on blocks and
directory tray headers, including the fit gates that decide when a label
would be legible versus clipped, so a glance at a large-enough box reveals
both what it is and how much space it occupies without crowding or overlap.

## Requirements

### Requirement: Name label on file-card blocks
The system SHALL paint a file or collapsed-directory block's name in its
top-left corner when the block's on-screen size clears a minimum
readability threshold, and SHALL omit the name label entirely below that
threshold rather than paint clipped, illegible text.

#### Scenario: A block large enough to read shows its name
- **WHEN** a rendered block's width and height both clear the minimum
  label-readability threshold
- **THEN** its name is painted in the block's top-left corner

#### Scenario: A block too small to read omits its name
- **WHEN** a rendered block's width or height falls below the minimum
  label-readability threshold
- **THEN** no name label is painted for that block

### Requirement: Name label on directory tray headers
The system SHALL paint a directory tray's name — or, for a collapsed chain
of single-child directories, the full joined chain name — in its header
strip whenever the tray itself is rendered.

#### Scenario: Every rendered tray shows its name
- **WHEN** a directory is rendered as a tray (a bordered frame with a
  header strip)
- **THEN** its header strip shows that directory's name, or the joined
  chain name if it represents a collapsed run of single-child directories

### Requirement: Size label on a rendered block
The system SHALL paint a block's or directory tray's total size, formatted
with the app's standard auto-scaling byte-size format (the largest unit
that keeps the value at least 1, e.g. bytes, KB, MB, GB), in its top-right
corner, so that a glance at a large-enough box reveals both what it is and
how much space it occupies.

#### Scenario: A large-enough block shows its size
- **WHEN** a rendered block or directory tray clears the size-label fit
  gate
- **THEN** its total size is painted in the top-right corner, formatted
  with the same auto-scaling unit rule used elsewhere in the app rather
  than a single fixed unit

### Requirement: Size-label fit gating avoids overlap and clipping
The system SHALL gate the size label independently from the name label, so
that a block only gains a size label once it is large enough for the size
text to render without clipping or overlapping the name label already
painted in the opposite corner. A block that fails that gate SHALL still
show its name label if it clears the (separate, lower) name-label
threshold, rather than show both labels crowded together or a clipped size
string.

#### Scenario: A comfortably large block shows both labels without collision
- **WHEN** a block is large enough for both its name and its formatted
  size to render without overlapping
- **THEN** both the name (top-left) and size (top-right) labels are
  painted

#### Scenario: A small block shows only its name
- **WHEN** a block clears the name-label threshold but not the size-label
  threshold
- **THEN** only the name label is painted; no size label is shown

#### Scenario: A tiny block shows neither label
- **WHEN** a block falls below the name-label threshold
- **THEN** neither a name label nor a size label is painted

### Requirement: Directory tray size-label gating accounts for chain-label width
Because a collapsed chain of single-child directories can already occupy
most of a tray header's width with its joined name, the system SHALL
determine whether a tray header has room for a size label using that
header's actual rendered label width — including any collapsed chain name
— rather than a fixed threshold that assumes a single short name.

#### Scenario: A short directory name leaves room for a size label
- **WHEN** a directory tray's header shows a short name (not a long
  collapsed chain) and the header is wide enough for both
- **THEN** the header shows both the name (or chain) and the size

#### Scenario: A long collapsed chain crowds out the size label
- **WHEN** a directory tray's header shows a long collapsed chain name
  that alone consumes most of the header's width
- **THEN** the size label is omitted from that header rather than
  rendered overlapping or clipped
