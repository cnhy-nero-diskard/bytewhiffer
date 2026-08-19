## ADDED Requirements

### Requirement: User file actions remain unelevated
Open, Reveal in Explorer, and Delete SHALL execute only from the normal-privilege UI process and SHALL NOT be forwarded to or implemented by the elevated Turbo helper.

#### Scenario: File action while helper is active
- **WHEN** the user invokes Open, Reveal, or confirmed Delete during an established Turbo helper session
- **THEN** the normal-privilege UI process performs the action under the user's ordinary token

