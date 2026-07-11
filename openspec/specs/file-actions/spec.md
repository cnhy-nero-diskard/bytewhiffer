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
The system SHALL let the user delete the file or folder represented by a
block from its context menu, and SHALL surface an error to the user rather
than silently failing if the deletion cannot complete.

#### Scenario: Deleting a block removes it from the treemap
- **WHEN** the user chooses Delete for a block and the deletion succeeds
- **THEN** the corresponding file or folder is removed from the filesystem
  and the treemap no longer shows that block

#### Scenario: A failed delete is surfaced to the user
- **WHEN** the user chooses Delete for a block and the underlying filesystem
  operation fails (e.g. the file is in use or access is denied)
- **THEN** the system shows an error to the user and the block remains in the
  treemap

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
