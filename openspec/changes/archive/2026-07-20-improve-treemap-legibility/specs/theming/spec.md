## ADDED Requirements

### Requirement: On-block label text remains legible against the block's own fill
The system SHALL render on-block label text — a block's name label, a
directory tray header's name label, and the size label — using a color
that darkens proportionally against whatever color and lightness is
beneath it, rather than a single fixed light color, so labels stay legible
across the full range of block hues, lightness bands, and the elevation
gradient's lightened top edge, without being tuned per color.

#### Scenario: A label over a lightened gradient top stays legible
- **WHEN** a label is painted over the lightened top portion of a raised
  card's gradient fill
- **THEN** the label text darkens enough relative to that lightened
  surface to remain readable, rather than rendering as a fixed light color
  that washes out against it

#### Scenario: The same darkening rule applies to every on-block label
- **WHEN** the file-card name label, the directory tray header label, or
  the block/tray size label is painted
- **THEN** each uses the same proportional-darken text-color rule, rather
  than each having its own independently-tuned color
