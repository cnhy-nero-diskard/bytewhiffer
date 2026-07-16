## Why

On a dense tree, the squarified layout already nests every directory that clears a pixel-size gate (`MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`), which means block count today is an accidental side effect of geometry, not something the user can choose. There's no way to ask for either "show me more of the structure at once, even small stuff" or "show me fewer, bigger chunks and let me peek before committing to a drill-down." This proposal is a first draft from an explore session — the exact abstraction mechanism and whether the mode is manual or auto-triggered are still open (see `design.md`) and expected to be revised before implementation starts.

## What Changes

- Add a user-facing render posture with two ends: a **detail** end (today's behavior — deep recursive squarify down to the existing pixel-size gates) and an **abstract** end (fewer, larger top-level blocks; a directory's contents are hidden behind its block sooner than the current gates would hide them).
- In abstract mode, hovering a directory block that is currently collapsed (not recursively expanded) shows a temporary, non-committal preview of that directory's contents — inset or floating over the block — without changing the current focus/breadcrumb trail. Moving the pointer away discards the preview.
- Clicking a block keeps today's exact behavior (`treemap-navigation`'s existing click-to-drill) regardless of render posture — the preview is hover-only and never substitutes for a real drill-down.

## Capabilities

### New Capabilities
- `treemap-abstraction`: the detail/abstract render posture — what controls it, and how it changes which directories render as collapsed single blocks versus recursively expanded ones.

### Modified Capabilities
- `treemap-navigation`: adds a hover-preview requirement — hovering a collapsed directory block (abstract mode) shows its contents without changing focus, on top of the existing hover-highlight and click-to-drill requirements.

## Impact

- `src/app.rs`: `draw_children` (the recursion/collapse decision currently gated by `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`), the hover hit-testing path (`hovered_path`/`hovered_size`, `hits`) built for the tooltip in `fix-hover-and-chrome-rendering`, and app UI state (a new posture toggle/control).
- No changes anticipated to `src/scanner/` or the pure geometry in `src/treemap.rs` — `squarify` itself doesn't need to change, only how often/where it's invoked recursively.
- Depends on (but does not duplicate) the hover plumbing landing in the in-progress `fix-hover-and-chrome-rendering` change — this proposal assumes that change's `hovered_path`/hit-rect/tooltip infrastructure exists rather than rebuilding it.
