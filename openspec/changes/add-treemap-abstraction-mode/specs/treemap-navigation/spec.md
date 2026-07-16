## ADDED Requirements

### Requirement: Hover preview for collapsed directory blocks
The system SHALL show a temporary, non-committal preview of a collapsed directory block's contents when the pointer hovers over it while the render posture (see `treemap-abstraction`) is set to abstract, without changing the current focus or breadcrumb trail.

#### Scenario: Hovering a collapsed block previews its contents
- **WHEN** the render posture is set to abstract and the pointer hovers over a directory block that is currently rendered as a collapsed single block
- **THEN** a preview of that directory's contents appears without changing the current focus or breadcrumb trail

#### Scenario: Moving the pointer away discards the preview
- **WHEN** the pointer moves off a block whose contents are being previewed
- **THEN** the preview is removed and the block returns to its collapsed rendering

#### Scenario: Preview does not appear in detail posture
- **WHEN** the render posture is set to detail
- **THEN** hovering a directory block does not show the content preview, since directories already expand structurally under detail posture
