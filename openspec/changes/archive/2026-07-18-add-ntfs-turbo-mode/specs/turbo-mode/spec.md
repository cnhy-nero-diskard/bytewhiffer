## ADDED Requirements

### Requirement: Turbo toggle reflects engine availability
The system SHALL render a Turbo toggle in the toolbar whose visual state
tracks the MFT engine's capability check for the current scan target:
disabled/greyed out when unavailable due to filesystem, normal/promptable
when available but requiring elevation, and active when already elevated.
Before any scan has run — so no capability check has happened yet — the
toggle SHALL NOT render disabled; it assumes the common case (an NTFS
target) and renders promptable, or active if the process is already
elevated. The toggle only becomes disabled/greyed out once a scan target is
actually checked and found not to be NTFS.

#### Scenario: No scan yet does not disable the toggle
- **WHEN** the application has just started and no scan target has been
  chosen, so the capability check has not run
- **THEN** the Turbo toggle renders promptable (or active, if the process is
  already elevated) rather than disabled/greyed out

#### Scenario: Non-NTFS target disables the toggle
- **WHEN** the current scan target's capability check reports
  `UnsupportedFilesystem`
- **THEN** the Turbo toggle renders disabled/greyed out and does not open
  any dialog if clicked

#### Scenario: NTFS target, not yet elevated, is promptable
- **WHEN** the current scan target's capability check reports
  `RequiresElevation`
- **THEN** the Turbo toggle renders enabled and clicking it begins the
  warning-dialog-then-elevation flow

#### Scenario: NTFS target, already elevated, is active
- **WHEN** the current scan target's capability check reports `Available`
- **THEN** the Turbo toggle renders in its active state and scanning uses
  the MFT engine without any further prompt

### Requirement: Warning dialog precedes elevation
The system SHALL show a warning dialog explaining that turbo mode requires
administrator privileges before triggering the OS elevation (UAC) prompt,
and SHALL NOT trigger UAC directly from a toggle click without that
intermediate confirmation.

#### Scenario: Clicking a promptable toggle shows the warning first
- **WHEN** the user clicks the Turbo toggle while it is in its promptable
  (not yet elevated) state, and a scan target already exists (something has
  been scanned or typed into the path field)
- **THEN** a warning dialog appears explaining the administrator
  requirement, and the OS elevation prompt is not triggered until the user
  confirms that dialog

#### Scenario: Clicking a promptable toggle with no target picks a folder first
- **WHEN** the user clicks the Turbo toggle while it is in its promptable
  state and no scan target exists yet (nothing scanned, nothing typed into
  the path field)
- **THEN** a folder picker opens; if the user picks a folder, that folder is
  recorded as the elevation root and the warning dialog appears, and no scan
  is started before elevation (the elevated relaunch performs the one real
  scan — no throwaway walker scan and no closing the window mid-scan); if the
  user cancels the picker, nothing else happens and no error is shown

#### Scenario: Dismissing the warning dialog does not elevate
- **WHEN** the user dismisses or cancels the warning dialog instead of
  confirming
- **THEN** no OS elevation prompt appears and the toggle remains in its
  promptable state

### Requirement: Elevation relaunch starts clean at the same scan root
The system SHALL, upon the user accepting the OS elevation prompt, relaunch
the application with an elevated process token and the current scan root
passed through, starting that new process fresh at that root rather than
restoring prior navigation state (breadcrumb depth, focus, abstraction
slider position). Declining the elevation prompt SHALL leave the original,
unelevated process running with the toggle back in its promptable state.

#### Scenario: Accepting elevation relaunches at the scan root
- **WHEN** the user accepts the OS elevation prompt
- **THEN** the application relaunches as a new elevated process that begins
  scanning the same root path the prior process was scanning, without
  restoring the prior process's navigation state

#### Scenario: Declining elevation leaves the app unelevated
- **WHEN** the user declines or cancels the OS elevation prompt
- **THEN** the original process keeps running unelevated, and the Turbo
  toggle returns to its promptable state rather than showing an error

### Requirement: Turbo stays on for the elevated process's lifetime
The system SHALL treat a successful elevation as applying for the remaining
lifetime of that elevated process: subsequent NTFS scan targets in the same
run use the MFT engine automatically, with no repeated warning dialog or UAC
prompt. This state SHALL NOT persist across separate application launches.

#### Scenario: A second NTFS target in the same session needs no re-prompt
- **WHEN** an elevated process, already running with turbo active, has its
  scan target changed to a different NTFS volume
- **THEN** the MFT engine is used for the new target with no warning dialog
  and no new UAC prompt

#### Scenario: A fresh launch does not inherit prior elevation
- **WHEN** the application is closed and started again
- **THEN** the new process starts unelevated, with the Turbo toggle back in
  whatever state the current target's capability check reports

### Requirement: Non-NTFS target after elevation warns instead of silently falling back
The system SHALL, when an already-elevated process's scan target is on a
non-NTFS volume, render the Turbo toggle in a distinct warning state and
show a dialog stating that turbo mode does not work for this drive, while
still completing the scan via the directory-walker fallback.

#### Scenario: Switching to a non-NTFS target while elevated warns the user
- **WHEN** an already-elevated process's scan target changes to a volume
  that is not NTFS
- **THEN** the Turbo toggle renders in its warning (red) state, a dialog
  informs the user turbo mode does not work for this drive, and the scan
  proceeds using the directory-walker engine for that target
