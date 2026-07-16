## ADDED Requirements

### Requirement: Detail/abstract render posture
The system SHALL provide a render posture setting with a detail end and an abstract end that the user can control, where the detail end matches today's behavior (a directory recursively expands into its own squarified children whenever its on-screen pixel size clears the existing nesting thresholds) and the abstract end causes directories to render as a single collapsed block more readily, showing fewer, larger blocks overall for the same tree.

#### Scenario: Abstract posture shows fewer blocks than detail posture for the same tree
- **WHEN** the same scanned tree is rendered once under the detail posture and once under the abstract posture
- **THEN** the abstract posture renders fewer total visible blocks than the detail posture

#### Scenario: Detail posture preserves today's nesting behavior
- **WHEN** the render posture is set to its detail end
- **THEN** a directory nests its children exactly as it does today, gated by the existing pixel-size/depth thresholds

### Requirement: Click-to-drill is unaffected by render posture
The system SHALL apply the same click-to-drill behavior regardless of render posture — clicking a directory block always drills into it exactly as the `treemap-navigation` capability's click-to-drill requirement specifies.

#### Scenario: Clicking a collapsed block in abstract mode drills in normally
- **WHEN** the render posture is set to abstract and the user clicks a directory block that is currently rendered as a collapsed single block
- **THEN** the view drills into that directory's contents exactly as it would under the detail posture
