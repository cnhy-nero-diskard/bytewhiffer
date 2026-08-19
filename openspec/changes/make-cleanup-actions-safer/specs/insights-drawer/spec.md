## REMOVED Requirements

### Requirement: Known-junk suggestions
**Reason**: The “junk” label overstates heuristic certainty and does not explain why broad names such as `build`, `dist`, or installers matched.

**Migration**: Replace this section and terminology with the structured Cleanup candidates requirement below; existing Open/Reveal/Delete entry points remain available subject to delete confirmation.

## ADDED Requirements

### Requirement: Cleanup candidates are advisory and explained
The system SHALL list heuristic cleanup candidates within the focused subtree under “Cleanup candidates.” Every candidate SHALL show a match reason and a confidence classification, and the UI SHALL NOT claim that a match is safe to delete.

#### Scenario: High-confidence disposable cache matches
- **WHEN** an entry matches a narrowly recognized disposable cache pattern
- **THEN** it appears with the specific reason and a high-confidence advisory label

#### Scenario: Context-dependent build output matches
- **WHEN** an entry has a generic name such as `build`, `dist`, `out`, or an installer-like filename
- **THEN** it appears as context-dependent with a reason that does not imply automatic safety

#### Scenario: Candidate reuses existing actions
- **WHEN** the user opens actions for a cleanup candidate
- **THEN** Open, Reveal, and confirmed Delete use the same action mechanisms as a treemap entry

#### Scenario: Candidate is never modified automatically
- **WHEN** an entry is classified or displayed as a cleanup candidate
- **THEN** no filesystem mutation occurs without the user explicitly initiating and confirming Delete

