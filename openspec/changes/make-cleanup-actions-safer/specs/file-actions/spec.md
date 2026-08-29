## MODIFIED Requirements

### Requirement: Delete action
The system SHALL let the user request deletion of the file or folder represented by an entry, SHALL show a confirmation naming the exact target before invoking the recycle-bin operation, and SHALL surface an error rather than silently failing. The visible tree SHALL change only after filesystem deletion succeeds.

#### Scenario: Delete request asks for confirmation
- **WHEN** the user chooses Delete for a treemap entry or cleanup candidate
- **THEN** a confirmation identifies the exact file or folder and no filesystem operation occurs yet

#### Scenario: Cancelling confirmation preserves the item
- **WHEN** the user cancels or dismisses delete confirmation
- **THEN** the filesystem and visible tree remain unchanged

#### Scenario: Deleting a block removes it from the treemap
- **WHEN** the user confirms Delete and the recycle-bin operation succeeds
- **THEN** the item is removed from the filesystem and visible tree, ancestor accounted sizes are updated, and focus moves to a valid ancestor if necessary

#### Scenario: A failed delete is surfaced to the user
- **WHEN** the confirmed filesystem operation fails, such as because the file is in use or access is denied
- **THEN** the system shows an error and the item remains in the visible tree

#### Scenario: Delete confirmation is unavailable while tree state is provisional
- **WHEN** a scan generation is active or the authoritative tree is still being assembled, including after a confirmation has been opened
- **THEN** Delete cannot be invoked or confirmed, the UI explains that deletion is unavailable until the tree is stable, and no filesystem or visible-tree mutation occurs

