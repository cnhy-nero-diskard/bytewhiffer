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

**Abstraction mechanism: a manual slider that scales the existing pixel-size gate, not a new cutoff or aggregate-block scheme.** TLDR: `MIN_NEST_AREA`/`MIN_NEST_SIDE` become slider-scaled instead of fixed constants — detail end of the slider is today's exact defaults, abstract end raises the thresholds so more blocks fail the nest-worthy check and render flat. No new depth math, no synthetic aggregate node. This is a deliberate rejection of both the depth-cutoff and top-K+aggregate-remainder candidates from the Open Questions below: depth cutoffs and raised size thresholds only help deeply-nested trees, and top-K+aggregate is real new machinery (a synthetic node type) that solves a different problem (a single flat directory with hundreds of same-level files) — worth doing later as its own change, not conflated with this one. The slider composes with the geometry gate rather than replacing it, since it *is* the geometry gate, just user-adjustable.

**Manual only, no auto-trigger.** The slider is a chrome control the user drags; there is no density-based auto-engage (unlike `render_dense`'s auto tier). Keeps the control's behavior fully predictable — moving the slider is the only thing that changes block count.

**Preview is inset, never a floating overlay.** The peek-preview renders a squarify of the hovered block's children entirely within that block's own rect. No content escapes the collapsed block's bounds. This matches SpaceSniffer's own visual language — its treemap never draws outside a rectangle's own geometry, only tooltips float — and it sidesteps the z-order/clipping work a bounds-exceeding overlay would need against neighboring blocks. Trade-off accepted: a very small collapsed block gets a cramped, less legible preview; that's judged acceptable since the preview is a hint, not a replacement for drilling in.

## Risks / Trade-offs

- [Peek-preview requires computing a live `squarify` on hover, potentially every frame the pointer sits over a new block] → Mitigate the same way `refresh_density`/`refresh_insights` already do: key the computed layout on `(hovered path, tree_rev)` and cache it, so it's recomputed only when the hovered block or the tree changes, not per frame.
- [Two independent thresholds — the existing pixel-geometry gate and a new user-facing posture — could interact confusingly, e.g. abstract mode "collapsing" a block the geometry gate would've expanded anyway] → Needs a concrete decision (see Open Questions) on whether the posture control replaces or composes with `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`.
- [Scope creep risk: "fewer blocks" could mean depth cutoff, size threshold, or top-K+aggregate-remainder, and only one of those actually helps a flat dense directory] → Explicitly deferred to Open Questions rather than guessed at, since the wrong pick means rebuilding this later.

## Open Questions

Resolved (see Decisions above):
- ~~Which abstraction mechanism reduces block count?~~ → (b), made user-adjustable via a manual slider, rather than a new depth cutoff or top-K+aggregate scheme. The top-K+aggregate case (flat DLL-heavy directories) is deferred to a future change.
- ~~Manual toggle or auto-triggered?~~ → Manual only, no density-based auto-engage.
- ~~Does the posture control replace the existing pixel-geometry gate, or layer on top of it?~~ → Layers on top — the slider scales `MIN_NEST_AREA`/`MIN_NEST_SIDE` directly rather than introducing a separate cutoff.

- ~~Preview visual treatment~~ → Inset within the collapsed block's own rect, never a floating overlay.

All open questions resolved.
