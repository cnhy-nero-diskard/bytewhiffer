# file-actions

## Purpose
Defines the right-click context menu on treemap blocks and the file-system
actions it exposes: deleting, opening, and revealing the represented file or
folder.

## Requirements

### Requirement: Right-click context menu
The system SHALL show a context menu on right-click of a treemap block,
offering Delete, Open, and Reveal in Explorer actions for the file or folder
that block represents.

#### Scenario: Right-clicking a block opens its context menu
- **WHEN** the user right-clicks a treemap block
- **THEN** a context menu appears listing Delete, Open, and Reveal in
  Explorer for that block's file or folder

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

#### Scenario: Delete is unavailable while tree state is provisional
- **WHEN** the user opens actions for an entry while a scan generation is active or the authoritative tree is still being assembled
- **THEN** Delete cannot be invoked, the UI explains that deletion is unavailable until the tree is stable, and no filesystem or visible-tree mutation occurs

#### Scenario: An open delete confirmation is cancelled when tree state becomes provisional
- **WHEN** a scan generation starts or authoritative-tree assembly begins after a delete confirmation has been opened
- **THEN** the confirmation is cancelled without invoking Delete, the user must initiate deletion again once the tree is stable, and no filesystem or visible-tree mutation occurs

### Requirement: Open action
The system SHALL let the user open the file or folder represented by a block
using the operating system's default handler for it.

#### Scenario: Opening a file block launches its default handler
- **WHEN** the user chooses Open for a block representing a file
- **THEN** the file is opened with the OS's default application for that file
  type

#### Scenario: Opening a folder block opens it in the file explorer
- **WHEN** the user chooses Open for a block representing a folder
- **THEN** that folder is opened in the system's file explorer

### Requirement: Reveal in Explorer action
The system SHALL let the user reveal the file or folder represented by a
block in Windows Explorer, with it selected/highlighted in its containing
folder.

#### Scenario: Revealing a block opens Explorer with the item selected
- **WHEN** the user chooses Reveal in Explorer for a block
- **THEN** Windows Explorer opens showing that item's containing folder with
  the item itself selected

### Requirement: Delete is unavailable while tree state is provisional
The system SHALL make Delete unavailable while a scan generation is active or
while its authoritative tree is still being assembled.

#### Scenario: Delete during active scan
- **WHEN** the user opens actions for an entry while a scan is active
- **THEN** Delete cannot be invoked and the UI explains that deletion is
  available after scanning finishes

#### Scenario: Delete during authoritative assembly
- **WHEN** scanning has finished but authoritative assembly remains active
- **THEN** Delete remains unavailable until the authoritative tree is
  installed

#### Scenario: Delete after stable completion
- **WHEN** no scan or assembly is active
- **THEN** Delete is available subject to the normal confirmation and
  filesystem rules
