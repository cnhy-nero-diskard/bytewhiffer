## Context

Bytewhiffer is a greenfield Rust desktop app: a Windows-only disk-space treemap
visualizer. `rust-space-sniffer-overview.md` (repo root) is the planning seed
this design formalizes. Two modules already exist and are committed
(`scanner.rs`, `treemap.rs`) — a sequential recursive directory walker with
atomic cancel/progress counters, and a squarified-treemap layout algorithm, both
GUI-independent and unit-tested. Nothing else exists yet: no `Cargo.toml`, no
Rust toolchain on this machine (WSL or Windows side), no UI code.

Constraint worth naming up front: this repo is being developed inside WSL2.
WSLg is confirmed running here (`DISPLAY=:0`, Weston compositor active) and
Windows interop is enabled (`cmd.exe`/`powershell.exe` reachable from WSL bash).
`egui`/`eframe`/`rfd` are cross-platform crates, so the Windows-only constraint
is a *product scope* decision (the overview doc, §1), not a technical one —
meaning the scanning engine, treemap rendering, hit-testing/interactivity,
navigation, and theming can all be built and visually/interactively verified
right here via WSLg. Only two MVP pieces are genuinely Windows-native and need a
real Windows session to confirm: the "Reveal in Explorer" action and real
recycle-bin delete semantics. The NTFS `$MFT` turbo-mode engine (v2, deferred)
will be Windows-native end to end, but is out of scope for this change.

## Goals / Non-Goals

**Goals:**
- Ship a working MVP: live-filling treemap, click-to-drill navigation with
  breadcrumb, right-click Delete/Open/Reveal actions, deterministic themed
  coloring — per the overview doc's §5 MVP list.
- Shape the scanning engine as a trait (`ScanEngine`) now, even though only one
  implementation (the parallel/sequential walker) ships in this change, so a v2
  NTFS `$MFT` engine can be added later without reworking the UI orchestration
  layer that consumes it.
- Reuse and extend the existing `scanner.rs`/`treemap.rs` modules rather than
  rewriting them; reorganize them into the `src/` module layout from the
  overview doc (§6).
- Keep `scanner/` and `treemap.rs` free of any `egui` dependency, so both stay
  unit-testable without a display — this matters more here than in most GUI
  apps, given how much of the rest of this domain (MFT access, real Explorer
  integration) can't be validated outside real Windows hardware.

**Non-Goals:**
- The NTFS `$MFT` "turbo mode" engine itself (v2, per overview doc §5) — this
  change only shapes the trait boundary it will eventually implement.
- Filtering, tagging, and export (all explicitly deferred to v2).
- A directory tree/list side panel — this stays a pure graphical map, per the
  overview doc's explicit out-of-scope call.
- Pinning exact `eframe`/`egui` versions or exact theme hex values here — the
  overview doc deliberately leaves both to implementation time (§2, §4).

## Decisions

### UI stack: `eframe`/`egui`, native folder picker via `rfd`
Carried over unchanged from the overview doc (§2): egui's immediate-mode model
fits "draw thousands of custom-computed rectangles every frame with custom
hit-testing" better than Slint's declarative chrome-oriented markup, and native
Rust keeps scan results and renderer in the same memory space, which a
Tauri+webview split would undercut by serializing a potentially huge tree
across an IPC boundary.

### `ScanEngine` as a trait, not a free function
The existing `scanner.rs` exposes a blocking free function
(`scan(root, cancel, progress) -> Entry`). This change turns that into a trait
so a second engine (v2's MFT reader) can be added later without the UI
orchestration layer changing. Three things are added to the contract now,
specifically *because* they're cheap to add before a second implementation
exists and expensive to retrofit after:

1. **Capability/fallback check** — `fn is_available(&self, target: &Path) ->
   Availability` where `Availability` is one of `Available`,
   `RequiresElevation`, `UnsupportedFilesystem`, or `NotApplicable`. Without
   this, "a second engine slots in later without reworking the UI layer" (the
   overview doc's own stated goal, §2) doesn't actually hold — something has to
   decide which engine to use per target and what to do when the preferred one
   can't run there. The walker always reports `Available`; this only becomes
   meaningful once a second engine exists, but the shape needs to be right now.
2. **`Result<Entry, ScanError>` instead of a bare `Entry`** — distinguishes
   "partial success, some entries silently skipped" (the walker's current
   behavior for unreadable entries/permission errors on individual files) from
   "this engine categorically cannot run against this target, caller should
   fall back to another engine" (not a case the walker hits today, but exactly
   what a declined-elevation or non-NTFS-volume MFT read would need to signal).
3. **Progress contract: monotonic done-ness + a final complete tree**, not "the
   engine streams continuously-growing partial subtrees." The walker naturally
   gives smooth incremental fill-in — a subtree completes and gets attributed
   the moment its recursion returns. An MFT engine is structurally two-phase: a
   single linear read of the raw `$MFT` (records reference a parent file ID,
   not children, so nothing can be attributed to a folder until enough of the
   table is read) followed by an in-memory bottom-up reconstruction. That means
   MFT mode likely can't offer the walker's smooth live-fill animation —
   more realistically a coarse "record N / M" bar followed by the tree popping
   in close to all at once, which is consistent with how WizTree's actual UI
   behaves. Defining the contract as "some done-ness signal + a final tree"
   lets both engines satisfy it honestly; live-fill-in becomes a walker-mode
   visual property, not a trait guarantee.

Target-scope note (not a trait-shape concern, but worth recording): a user can
scan any subfolder, not just a volume root. The walker scans exactly that path
naturally. An MFT engine physically cannot selectively read partial records —
reading a volume's `$MFT` returns the whole volume's records regardless of the
requested path — so an MFT engine would need to read the whole volume,
reconstruct globally, then extract the requested subtree internally. This stays
contained inside that (not-yet-built) engine's implementation as long as the
trait's contract remains "given this path, return its Entry tree," so it does
not affect the trait signature designed in this change.

### Squarified treemap stays a pure, reused module
`treemap.rs`'s `squarify(sizes, rect) -> Vec<Rect>` already implements the
Bruls/Huizing/van Wijk algorithm correctly (area-conserving, order-preserving,
degenerate-input-safe, tested against naive slice-and-dice sliver cases). This
change wires it into the UI as-is; no algorithmic changes anticipated.

### Module layout
Reorganize the existing root-level `scanner.rs`/`treemap.rs` into the overview
doc's suggested layout (§6):
```
src/
  main.rs        — eframe::run_native setup
  app.rs         — eframe::App impl, panel layout, scan orchestration,
                   navigation state (focus path / breadcrumb)
  scanner/
    mod.rs       — Entry tree type, ScanEngine trait, Availability/ScanError,
                   ScanProgress counters
    walker.rs    — the walker engine (existing scanner.rs logic, moved),
                   implementing ScanEngine
  treemap.rs     — existing pure squarified-layout module, moved as-is
  theme.rs       — color palette + deterministic color-from-extension logic
  util.rs        — byte-size formatting (existing format_size) + small helpers
```
`scanner/mft.rs` is intentionally not created in this change — it's a v2
concern; the trait boundary is what needs to exist now, not a stub
implementation.

### Theming approach
Dark base (`#0d1117`–`#0f1420` family), hash-derived hue per file extension
constrained to a fixed saturation/lightness band (so the palette reads as
curated rather than random RGB noise), depth communicated via elevation/
lightness shift rather than additional hues, and one reserved accent color for
hover/breadcrumb/selection state. Exact hex values are an implementation-time
decision (overview doc §4); this design fixes the direction and the mechanism
(hash-derived + banded), not the palette.

### Dev-loop split between WSL and Windows
Given the WSLg finding above: pure-logic modules (`scanner`, `treemap`) and the
full interactive UI (rendering, hit-testing, click/hover/zoom, navigation,
theming, folder picker) get built and verified under WSL via WSLg — this
directly exercises the overview doc's Known Risk #2 (treemap render
interactivity, §7) without needing Windows at all. Only Reveal-in-Explorer and
real delete/recycle-bin behavior need a checkpoint on an actual Windows
session. Toolchain setup (`rustup`) is needed on whichever side(s) development
happens on; neither side currently has one.

## Risks / Trade-offs

- **[Risk]** Treemap render interactivity (per-rectangle hit-testing, hover,
  click-to-zoom at potentially thousands of visible rectangles) is named in the
  overview doc as a top risk (§7). → **Mitigation**: WSLg lets this be tested
  interactively throughout development rather than only at Windows checkpoints,
  catching problems far earlier than the overview doc's original assumption
  that this could only be validated on real Windows hardware.
- **[Risk]** Designing `ScanEngine` for a v2 engine that doesn't exist yet risks
  guessing wrong about what MFT mode needs. → **Mitigation**: the three additions
  above (capability check, Result-based fallback signal, done-ness-only
  progress contract) were chosen specifically because they're structural
  properties of how MFT reading *must* work (elevation/filesystem gating, whole-
  volume single-pass reads, two-phase reconstruction) rather than guesses about
  MFT-specific data shapes — lower confidence details (e.g. exact progress
  phase labels) are deliberately left open rather than baked in now.
- **[Risk]** Reveal-in-Explorer and recycle-bin delete semantics can't be
  verified under WSLg (no real Explorer shell, no real recycle bin). →
  **Mitigation**: isolate these as their own action-handler code paths and flag
  them explicitly as Windows-checkpoint items in tasks.md rather than assuming
  they're covered by WSL-side testing.
- **[Trade-off]** Whether the walker is sequential (as currently committed) or
  parallelized (`rayon`/`jwalk`, per the overview doc's Phase 1 description) is
  left open — see Open Questions. Shipping sequential is simpler and already
  tested; parallelizing later is a fast-follow, not a rewrite, since the
  `ScanEngine` trait boundary doesn't care how a given engine achieves its
  result internally.

## Migration Plan

Not applicable — greenfield change, nothing in production to migrate or roll
back. Build order should follow the overview doc's own risk-driven sequencing
(§7): pure logic first (scanner/treemap, already drafted), then the UI shell
and treemap rendering/interactivity (the named render-interactivity risk, now
testable via WSLg), then navigation chrome, then actions, then theming polish —
deferring anything MFT-related entirely.

## Open Questions

- Sequential walker (current, committed) vs. parallelizing with `rayon`/`jwalk`
  now: ship sequential for MVP and fast-follow, or parallelize before calling
  Phase 1 done? The overview doc's own scan-time ballparks (§2) assumed the
  parallel version. Left for tasks.md to resolve explicitly rather than
  pre-deciding here.
- Exact `eframe`/`egui` version to pin — overview doc explicitly defers this to
  implementation time (§2) rather than trusting a hardcoded number.
- Exact theme hex values — overview doc explicitly defers this to
  implementation time (§4).
- Exact `ScanProgress` shape: keep it as two flat atomics (files/bytes scanned,
  as already implemented) that an MFT engine drives coarsely, or introduce a
  richer phase-aware progress type. Leaning toward reusing the existing simple
  counters (lower complexity now, MFT mode just updates them in bigger jumps),
  but not fixed here.
