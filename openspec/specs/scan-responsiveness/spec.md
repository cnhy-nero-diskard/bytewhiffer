# scan-responsiveness

## Purpose
Defines how the app keeps the UI responsive while processing scan-discovery
events and assembling the authoritative tree, by pacing that work within a
bounded per-frame time budget instead of draining it synchronously, so a
large or file-dense scan target never freezes rendering.

## Requirements

### Requirement: Time-boxed live event draining
The system SHALL process queued scan-discovery events within a bounded
per-frame time slice rather than draining every currently-queued event
synchronously, so that a discovery backlog built up on a large or
file-dense directory cannot block a single frame for an unbounded
duration.

#### Scenario: A small backlog drains in one frame
- **WHEN** the number of currently-queued discovery events fits within
  the per-frame time budget
- **THEN** all of them are processed in that frame, same as today

#### Scenario: A large backlog spreads across multiple frames
- **WHEN** the currently-queued discovery events would take longer than
  the per-frame time budget to process
- **THEN** only the portion that fits the budget is processed this frame,
  and the remainder is processed on subsequent frames without blocking
  rendering for longer than the budget in any single frame

### Requirement: Resumable authoritative-tree assembly on scan completion
The system SHALL assemble the UI tree from the scan engine's authoritative
result across multiple frames, within the same per-frame time budget as
live event draining, rather than performing one uninterrupted synchronous
rebuild, so that completing a scan of a large or file-dense directory does
not freeze the UI.

#### Scenario: A small completed tree assembles in one frame
- **WHEN** the authoritative tree returned by the scan engine is small
  enough to assemble within the per-frame time budget
- **THEN** it is assembled and swapped in during that same frame, same as
  today

#### Scenario: A large completed tree assembles across multiple frames
- **WHEN** the authoritative tree returned by the scan engine would take
  longer than the per-frame time budget to assemble
- **THEN** assembly is spread across multiple frames, and the previously-
  displayed live tree remains visible and interactive until assembly
  finishes

### Requirement: Atomic handoff from live tree to authoritative tree
The system SHALL swap the displayed tree from the live, incrementally-
built one to the newly-assembled authoritative one only once assembly is
fully complete, never showing a partially-assembled authoritative tree to
the user.

#### Scenario: Assembly completes
- **WHEN** the paced authoritative-tree assembly finishes
- **THEN** the displayed tree is replaced by the fully-assembled
  authoritative tree in a single, atomic swap

#### Scenario: Assembly is still in progress
- **WHEN** the paced authoritative-tree assembly has not yet finished
- **THEN** the displayed tree remains the live, incrementally-built one,
  not a partially-assembled authoritative tree

### Requirement: Bounded best-effort live discovery
The system SHALL bound the number of live discovery events retained for UI
preview independently of total filesystem entry count. Producers MUST use
non-blocking delivery and MAY drop preview events when capacity is exhausted.

#### Scenario: Producer outruns the UI
- **WHEN** scanners discover entries faster than the UI drains live events
- **THEN** queued live events remain within the configured capacity and scanner
  progress does not block on queue space

#### Scenario: Live events are dropped under pressure
- **WHEN** the bounded live-event channel is full
- **THEN** the event may be omitted from the provisional tree without
  affecting progress counters or the authoritative final tree

#### Scenario: Final tree replaces an incomplete preview
- **WHEN** most live discovery events were dropped but the scan completes
  successfully
- **THEN** the complete authoritative tree is assembled and atomically
  replaces the provisional tree
