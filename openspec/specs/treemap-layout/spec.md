# treemap-layout

## Purpose
Defines the pure, GUI-agnostic squarified treemap layout algorithm that turns
a list of item sizes into proportionally-sized, near-square rectangles within
a container, including degenerate-input handling.

## Requirements

### Requirement: Squarified treemap layout
The system SHALL compute a squarified treemap layout (Bruls, Huizing & van
Wijk, 1999) that, given a list of item sizes and a container rectangle,
produces one output rectangle per item with area proportional to that item's
size, favoring near-square rectangles over the thin slivers a naive
slice-and-dice layout would produce. This computation SHALL have no
dependency on any GUI toolkit, file, or pixel-specific type, so it can be
unit-tested in isolation.

#### Scenario: Output area is proportional to input size
- **WHEN** `squarify` is given a list of item sizes and a container rectangle
- **THEN** each output rectangle's area is proportional to its corresponding
  input size, and the sum of all output rectangle areas equals the
  container's area (within floating-point tolerance)

#### Scenario: Output order matches input order
- **WHEN** `squarify` is given a list of sizes in a specific order
- **THEN** the returned rectangles are in that same order, so a caller can
  zip results back up with the original items by index

#### Scenario: No output rectangle escapes the container
- **WHEN** `squarify` lays out any non-empty list of positive sizes into a
  container rectangle
- **THEN** every output rectangle's bounds are fully contained within the
  container rectangle's bounds

#### Scenario: Rectangles favor square-ish aspect ratios
- **WHEN** `squarify` lays out a set of sizes that a naive slice-and-dice
  layout would render as thin slivers
- **THEN** the resulting rectangles have a meaningfully better (lower) worst-
  case aspect ratio than slice-and-dice would produce

### Requirement: Degenerate input handling
The system SHALL handle empty input, all-zero-sized items, and zero-area
container rectangles without panicking, producing sensible fallback output in
each case.

#### Scenario: Empty size list produces empty output
- **WHEN** `squarify` is called with an empty list of sizes
- **THEN** it returns an empty list of rectangles

#### Scenario: All-zero-sized items still produce visible, clickable slots
- **WHEN** every item in the input list has size zero
- **THEN** `squarify` still returns one non-zero-area rectangle per item
  rather than collapsing all of them to nothing

#### Scenario: A zero-width or zero-height container does not panic
- **WHEN** `squarify` is given a container rectangle with zero width or zero
  height
- **THEN** it returns one rectangle per input item, each with zero area,
  without panicking
