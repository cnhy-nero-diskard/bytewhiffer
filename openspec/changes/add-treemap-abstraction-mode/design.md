## Context

`draw_children` (`src/app.rs`) already decides, per block, whether to recurse into a directory's children or render it as one flat/card block — gated by `MIN_NEST_AREA`, `MIN_NEST_SIDE`, and `MAX_DEPTH`. That decision is purely geometric (pixel size of the block this frame) and not user-controllable. Separately, `render_dense`/`DENSE_RENDER_THRESHOLD` auto-computes a *rendering cost* tier (flat-fill vs. shadowed card) from the focused subtree's descendant count — it never changes which blocks exist, only how cheaply they're painted.

The in-progress `fix-hover-and-chrome-rendering` change is building the hover plumbing this design leans on: pointer→hit-rect lookup (`hits: Vec<HitRect>`), `hovered_path`/`hovered_size` state, and tooltip positioning. This design assumes that lands first.

This document is a first-pass sketch from an explore session, not a locked design — several decisions below are marked tentative and are expected to change once the user has spent more time with the idea (see Open Questions).

## Goals / Non-Goals

**Goals:**
- Let block count/nesting depth be influenced by something other than pure per-frame pixel geometry.
- Give a hover-only, non-committal way to see what's inside a collapsed directory block without drilling into it.
- Reuse the existing click-to-drill behavior unchanged — this feature only affects what renders before a click, not navigation semantics.

**Non-Goals:**
- Not building a fisheye/lens distortion effect that resizes neighboring blocks on hover (rejected during exploration — see Decisions).
- Not changing `treemap::squarify` itself; this is entirely about when/how often the existing layout function is invoked recursively.
- Not solving the "huge flat directory with hundreds of same-level files" case in this change unless the user confirms that's the actual motivating scenario (see Open Questions) — top-K+aggregate-remainder is a different, larger piece of work than a depth/size threshold change.

## Decisions

**Hover reveals real content, not a magnify effect.** Between the two interaction models discussed (peek-preview: render a real squarify of the hovered block's children inset/floating on top, vs. pop/magnify: grow the block visually without revealing children), peek-preview is the working choice, since the entire cost of "abstract mode" is hidden information, and only a content-revealing preview actually answers "what's in here" cheaply. Pop/magnify was set aside as solving a different problem (emphasis, not information).

**Preview never touches `self.focus`.** The preview is computed and rendered as an overlay keyed off the currently-hovered `HitRect`, entirely separate from the `focus`/breadcrumb trail state that click-to-drill mutates. This keeps the existing `treemap-navigation` click contract untouched — abstract mode only changes what's visible before a click, never what a click does.

## Risks / Trade-offs

- [Peek-preview requires computing a live `squarify` on hover, potentially every frame the pointer sits over a new block] → Mitigate the same way `refresh_density`/`refresh_insights` already do: key the computed layout on `(hovered path, tree_rev)` and cache it, so it's recomputed only when the hovered block or the tree changes, not per frame.
- [Two independent thresholds — the existing pixel-geometry gate and a new user-facing posture — could interact confusingly, e.g. abstract mode "collapsing" a block the geometry gate would've expanded anyway] → Needs a concrete decision (see Open Questions) on whether the posture control replaces or composes with `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`.
- [Scope creep risk: "fewer blocks" could mean depth cutoff, size threshold, or top-K+aggregate-remainder, and only one of those actually helps a flat dense directory] → Explicitly deferred to Open Questions rather than guessed at, since the wrong pick means rebuilding this later.

## Open Questions

- **Which abstraction mechanism reduces block count?** Candidates: (a) a shallower user-adjustable depth cutoff, (b) user-adjustable size thresholds (raising today's `MIN_NEST_AREA`/`MIN_NEST_SIDE`), (c) top-K children shown per directory with the remainder folded into a synthetic aggregate block. (a) and (b) only help deeply-nested trees; (c) is the one that helps a single flat directory with hundreds of same-level files (e.g. a DLL-heavy system dir) — need to confirm which scenario is actually motivating this before picking.
- **Manual toggle or auto-triggered?** Whether detail/abstract is a posture the user explicitly sets (e.g. a chrome control), or auto-engages past a density threshold (mirroring `render_dense`'s existing auto-trigger) with manual override on top.
- **Does the posture control replace the existing pixel-geometry gate, or layer on top of it?** i.e. is "detail mode" just today's `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH` values, with "abstract mode" a different set of the same constants — or is abstraction a logically separate cutoff applied after the geometry gate already decided a block *could* nest?
- **Preview visual treatment**: inset within the collapsed block's own rect, or a floating overlay that can exceed the block's bounds (more legible for a small block, but needs z-order/clipping work against neighboring blocks)?
