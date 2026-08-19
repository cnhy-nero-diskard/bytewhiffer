## ADDED Requirements

### Requirement: Stable treemap work is reused
The system SHALL reuse child ordering and layout results while node structure, focus, viewport geometry, and resolved abstraction settings remain unchanged.

#### Scenario: Repaint without layout input change
- **WHEN** the treemap repaints due only to hover, tooltip, or unrelated chrome state
- **THEN** visible nodes do not re-sort children or recompute identical squarified rectangles

#### Scenario: Child size changes
- **WHEN** a visible node's child set or accounted sizes change
- **THEN** cached ordering/layout for the affected branch is invalidated and recomputed

#### Scenario: Viewport or abstraction changes
- **WHEN** viewport dimensions or resolved abstraction/nesting settings change
- **THEN** layout is recomputed using the new inputs and remains compliant with squarified layout requirements

