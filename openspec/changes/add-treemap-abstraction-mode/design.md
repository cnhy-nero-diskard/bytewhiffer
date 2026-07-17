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

**Abstraction mechanism: a manual slider driving a combined depth-cap + size-scale nesting gate, not a new aggregate-block scheme.** The slider (`abstraction`, 0.0 detail → 1.0 abstract) resolves per frame into a `NestGate { max_depth, min_side, min_area }`:

- **Depth cap** — full `MAX_DEPTH` (10) at detail, dropping to a floor of 1 at abstract via a concave `(1 - a)^2` map. This is what makes "fully abstract" reliably mean "only top-level blocks show interior", *uniformly* across every branch regardless of on-screen size. The concave curve front-loads the drop because real trees rarely render deeper than ~5-6 levels, so a linear ramp would leave the slider's left half inert.
- **Size scale** — `MIN_NEST_SIDE`/`MIN_NEST_AREA` scaled by up to `1 + ABSTRACTION_SIDE_GAIN` (kept absolute and modest, not viewport-relative). The depth cap only bites once a branch is deeper than the cap, so on a shallow tree it can do nothing until near the abstract end; the size scale fills that gap by thinning small/medium directories continuously at every slider position.

At `abstraction == 0.0` both reduce to today's exact constants, so the detail end preserves prior behavior.

*Revised after real-use testing (see below).* The first cut was a pure size-threshold scale (the original decision here). Testing on a real dense scan showed it never reached a "top-level only" view and felt no different between detail and full abstract — a fixed pixel scale topped out far too low on a large window, and a viewport-relative absolute threshold collapsed narrow top-level blocks while leaving wide siblings fully detailed (arbitrary, not abstract). A depth cap gives the uniform endpoint; the residual size scale keeps the mid-range continuous. Both remaining candidates from the Open Questions stay rejected: **top-K + aggregate-remainder** is still deferred — it is the only thing that thins a *single flat directory with hundreds of same-level children* (each renders as one solid block once collapsed, and only aggregation reduces that further), and it needs a synthetic node type, so it is its own future change.

**Manual only, no auto-trigger.** The slider is a chrome control the user drags; there is no density-based auto-engage (unlike `render_dense`'s auto tier). Keeps the control's behavior fully predictable — moving the slider is the only thing that changes block count.

**Preview is inset, never a floating overlay.** The peek-preview renders a squarify of the hovered block's children entirely within that block's own rect. No content escapes the collapsed block's bounds. This matches SpaceSniffer's own visual language — its treemap never draws outside a rectangle's own geometry, only tooltips float — and it sidesteps the z-order/clipping work a bounds-exceeding overlay would need against neighboring blocks. Trade-off accepted: a very small collapsed block gets a cramped, less legible preview; that's judged acceptable since the preview is a hint, not a replacement for drilling in.

## Risks / Trade-offs

- [Peek-preview requires computing a live `squarify` on hover, potentially every frame the pointer sits over a new block] → Mitigate the same way `refresh_density`/`refresh_insights` already do: key the computed layout on `(hovered path, tree_rev)` and cache it, so it's recomputed only when the hovered block or the tree changes, not per frame.
- [Two independent thresholds — the existing pixel-geometry gate and a new user-facing posture — could interact confusingly, e.g. abstract mode "collapsing" a block the geometry gate would've expanded anyway] → Needs a concrete decision (see Open Questions) on whether the posture control replaces or composes with `MIN_NEST_AREA`/`MIN_NEST_SIDE`/`MAX_DEPTH`.
- [Scope creep risk: "fewer blocks" could mean depth cutoff, size threshold, or top-K+aggregate-remainder, and only one of those actually helps a flat dense directory] → Explicitly deferred to Open Questions rather than guessed at, since the wrong pick means rebuilding this later.

## Open Questions

Resolved (see Decisions above):
- ~~Which abstraction mechanism reduces block count?~~ → A combined depth-cap + size-scale gate driven by a manual slider (revised from the initial pure size-threshold pick after testing). Top-K+aggregate still deferred to a future change.
- ~~Manual toggle or auto-triggered?~~ → Manual only, no density-based auto-engage.
- ~~Does the posture control replace the existing pixel-geometry gate, or layer on top of it?~~ → Layers on top — the slider tightens the existing depth cap (`MAX_DEPTH`) and size thresholds (`MIN_NEST_AREA`/`MIN_NEST_SIDE`) rather than introducing a separate cutoff.

- ~~Preview visual treatment~~ → Inset within the collapsed block's own rect, never a floating overlay.

All open questions resolved.
