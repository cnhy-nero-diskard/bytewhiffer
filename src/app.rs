//! eframe::App implementation: UI state, panel layout, background-scan
//! orchestration, and navigation state (focus path / breadcrumb).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::insights;
use crate::scan_controller::{PreparedOutcome, ScanCompletion, ScanController};
use crate::scanner::{
    mft::{self, MftEngine},
    walker::WalkerEngine,
    Availability, Entry, ScanEngine, ScanError, ScanEvent, ScanId,
};
use crate::theme;
use crate::treemap;
use crate::util::{
    elide_middle, format_duration, format_duration_live, format_size, format_size_precise,
};

/// Stop nesting once a block is this small; below it nothing inside would
/// be readable or clickable anyway.
const MIN_NEST_AREA: f32 = 1200.0;
const MIN_NEST_SIDE: f32 = 24.0;
/// At the abstract end of the render-posture slider the nesting gate's minimum
/// side is multiplied by up to `1.0 + ABSTRACTION_SIDE_GAIN` (and its square
/// for area), so small/medium blocks collapse before the depth cap alone would
/// reach them. Kept absolute (not viewport-relative) and modest so it thins
/// blocks uniformly by pixel size without ever leaving one whole pane detailed
/// while a sibling collapses. See `BytewhifferApp::nest_gate`.
const ABSTRACTION_SIDE_GAIN: f32 = 4.0;
/// Padding between a collapsed block's edge and the hover-preview squarify
/// laid out inside it, so the accent frame and a rim of the block still read.
const PREVIEW_INSET: f32 = 3.0;
/// Hard depth cap as a backstop against pathological trees.
const MAX_DEPTH: usize = 10;
/// How many entries the biggest-files/folders leaderboard shows.
const LEADERBOARD_N: usize = 15;
/// Vertical space reserved for a directory's name strip when nesting into it.
const DIR_LABEL_H: f32 = 16.0;
const BLOCK_PAD: f32 = 2.0;
/// Below this on-screen side length a block (or chrome element) renders with
/// flat fill only — no shadow, gradient, corner radius, or gap — matching the
/// pre-elevation look. Sits alongside `MIN_NEST_AREA`/`MIN_NEST_SIDE` as a
/// legibility/perf floor: dense clusters of tiny blocks would otherwise turn a
/// blurred shadow + rounded gradient on every one of them into visual mush.
const MIN_CARD_SIDE: f32 = 22.0;
/// Gap inset applied to a raised card so its neighbours' drop shadows show
/// through. Flat-fallback blocks below `MIN_CARD_SIDE` skip this (no gap).
const CARD_GAP: f32 = 1.5;
/// Character budget for the hover tooltip's path line before it is
/// middle-elided (see `util::elide_middle`). Sized to a comfortable single
/// line; the bottom status bar carries the full, unelided path.
const TOOLTIP_MAX_CHARS: usize = 64;
/// Once the focused subtree holds more than this many descendant entries, the
/// treemap paints card-eligible blocks with a cheaper flat-rounded fill (no
/// blurred shadow, no gradient mesh) for the whole frame, so hover/pointer
/// tracking stays responsive on dense views. See `BytewhifferApp::render_dense`.
/// Tuned against the `--debug-perf` spike; a global per-view switch (not
/// per-block) so a view never mixes elevated and flat cards inconsistently.
const DENSE_RENDER_THRESHOLD: usize = 1500;
/// Font size for the size label painted in a block's or tray header's
/// top-right corner — matches the existing name-label font size so both
/// sit on the same baseline.
const LABEL_FONT_SIZE: f32 = 11.0;
/// Horizontal inset from a block's/header's edge to a corner label; mirrors
/// the offsets already used when painting the name label.
const LABEL_H_PAD: f32 = 6.0;
/// Minimum width reserved for the name-label column before a size label is
/// allowed to claim a file-card block's opposite corner, so the two labels
/// never crowd each other even when the name itself renders short.
const SIZE_LABEL_NAME_RESERVE: f32 = 44.0;
/// Horizontal gap kept between a tray header's name (or collapsed-chain)
/// label and its size label, so the two never sit flush against each other.
const TRAY_LABEL_GAP: f32 = 10.0;
/// Wall-clock time budget for one frame's live scan-event processing. Bounds
/// the actual frame-blocking duration directly so a discovery burst spreads
/// its UI insertion cost across multiple frames instead of stalling one.
const SCAN_FRAME_BUDGET: Duration = Duration::from_millis(8);
/// Smoothing factor for the scan-rate EMA (`rate = rate*(1-α) + instant*α`),
/// applied at the same ~1s cadence as `rate_sample`. Chosen by feel against a
/// large scan target — high enough to track a real trend within a couple
/// samples, low enough that a single noisy per-second delta doesn't dominate.
const RATE_EMA_ALPHA: f64 = 0.3;

/// The UI-side tree is shared with the egui-free display-tree preparation
/// stage. Keeping the local alias preserves the existing rendering code while
/// making descendant metadata, structural revisions, and deterministic child
/// order part of the real tree model rather than app-only helpers.
type Node = crate::display_tree::DisplayNode;

/// The Turbo toggle's rendered state, derived from the MFT engine's capability
/// check for the current scan target plus whether this process is already
/// elevated. Drives both the toggle's look and what a click does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TurboState {
    /// A non-NTFS target on an unelevated process — greyed out, clicking does
    /// nothing.
    Disabled,
    /// NTFS target, not yet elevated — clicking begins the warn-then-UAC flow.
    Promptable,
    /// NTFS target on an already-elevated process — turbo is on; scans use the
    /// MFT engine with no further prompt.
    Active,
    /// An already-elevated process pointed at a non-NTFS target — turbo can't
    /// apply here; clicking explains why (the scan already used the walker).
    WarnUnsupported,
}

/// A warning red in the GitHub-dark family, used only for the Turbo toggle's
/// `WarnUnsupported` state (an elevated process on a non-NTFS drive). Not a
/// general palette color — turbo is the one place a "this won't work" signal
/// is surfaced on a control that is otherwise interactive.
const TURBO_WARN_RED: egui::Color32 = egui::Color32::from_rgb(0xda, 0x36, 0x33);

/// The render posture's resolved nesting gate for one frame: a directory
/// subdivides only if it is shallower than `max_depth` *and* clears the pixel
/// thresholds. Both tighten together as the abstraction slider moves toward
/// abstract — the depth cap gives a uniform "top-level only" endpoint, the
/// size scale gives continuous thinning at every slider position regardless of
/// how deep the tree happens to be. See `BytewhifferApp::nest_gate`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct NestGate {
    max_depth: usize,
    min_side: f32,
    min_area: f32,
}

/// Resolves an `abstraction` slider value (0.0 detail .. 1.0 abstract) into a
/// `NestGate`. Combines two levers that both tighten toward the abstract end:
///
/// - **Depth cap** — full `MAX_DEPTH` at detail, dropping to a floor of 1 at
///   abstract, where only the focused node's direct children nest a single
///   level and everything deeper collapses. This is what makes "fully
///   abstract" reliably mean "only the top-level blocks show interior",
///   *uniformly* across every branch regardless of pixel size. The mapping is
///   concave (`(1 - a)^2`) because real trees rarely render deeper than ~5-6
///   levels, so a linear 10→1 ramp would leave the slider's left half doing
///   nothing; squaring front-loads the drop.
/// - **Size scale** — multiplies `MIN_NEST_SIDE`/`MIN_NEST_AREA` by up to
///   `1.0 + ABSTRACTION_SIDE_GAIN`. The depth cap only bites once a branch is
///   deeper than the cap, so on a shallow tree it can do nothing until near
///   the abstract end; the size scale fills that gap by thinning small/medium
///   blocks continuously at every slider position.
///
/// At `abstraction == 0.0` both reduce to today's exact constants, so the
/// detail end preserves prior behavior. A free function (not a method) so it
/// can be unit-tested and reused by the `--debug-perf` bench without needing a
/// live `BytewhifferApp`.
fn resolve_nest_gate(abstraction: f32) -> NestGate {
    let a = abstraction.clamp(0.0, 1.0);
    let depth_span = (MAX_DEPTH - 1) as f32;
    let max_depth = (1.0 + depth_span * (1.0 - a).powi(2)).round() as usize;
    let side_scale = 1.0 + a * ABSTRACTION_SIDE_GAIN;
    NestGate {
        max_depth,
        min_side: MIN_NEST_SIDE * side_scale,
        min_area: MIN_NEST_AREA * side_scale * side_scale,
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RectKey([u32; 4]);

impl RectKey {
    fn from_rect(rect: Rect) -> Self {
        Self([
            rect.left().to_bits(),
            rect.top().to_bits(),
            rect.width().to_bits(),
            rect.height().to_bits(),
        ])
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GateKey {
    max_depth: usize,
    min_side: u32,
    min_area: u32,
}

impl From<NestGate> for GateKey {
    fn from(gate: NestGate) -> Self {
        Self {
            max_depth: gate.max_depth,
            min_side: gate.min_side.to_bits(),
            min_area: gate.min_area.to_bits(),
        }
    }
}

/// Inputs shared by all layouts in one visible treemap frame. A context
/// change clears the cache because the focused root or viewport can change
/// which branches are visible and where they are placed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LayoutContextKey {
    focus: Vec<String>,
    viewport: RectKey,
    gate: GateKey,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LayoutKey {
    node_path: PathBuf,
    structural_rev: u64,
    rect: RectKey,
    depth: usize,
    gate: GateKey,
}

struct LayoutResult {
    order: Vec<usize>,
    rects: Vec<treemap::Rect>,
}

struct CachedLayout {
    result: Rc<LayoutResult>,
    last_used_generation: u64,
}

/// Reuses the expensive child ordering and squarified rectangles for the
/// current visible treemap. Entries are keyed by the node's structural
/// revision and exact geometry, then pruned after each frame so a long live
/// scan cannot accumulate one layout for every historical tree revision.
#[derive(Default)]
struct TreemapLayoutCache {
    context: Option<LayoutContextKey>,
    generation: u64,
    entries: HashMap<LayoutKey, CachedLayout>,
    #[cfg(test)]
    hits: usize,
    #[cfg(test)]
    misses: usize,
}

impl TreemapLayoutCache {
    fn begin_frame(&mut self, focus: &[String], viewport: Rect, gate: NestGate) {
        let context = LayoutContextKey {
            focus: focus.to_vec(),
            viewport: RectKey::from_rect(viewport),
            gate: gate.into(),
        };
        if self.context.as_ref() != Some(&context) {
            self.entries.clear();
            self.context = Some(context);
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.entries.clear();
            self.generation = 1;
        }
    }

    fn layout_for(
        &mut self,
        node: &Node,
        rect: Rect,
        depth: usize,
        gate: NestGate,
    ) -> Rc<LayoutResult> {
        let key = LayoutKey {
            node_path: node.path.clone(),
            structural_rev: node.structural_rev(),
            rect: RectKey::from_rect(rect),
            depth,
            gate: gate.into(),
        };
        if let Some(cached) = self.entries.get_mut(&key) {
            cached.last_used_generation = self.generation;
            #[cfg(test)]
            {
                self.hits += 1;
            }
            return Rc::clone(&cached.result);
        }

        let mut order: Vec<usize> = (0..node.children.len()).collect();
        order.sort_unstable_by(|&a, &b| {
            let left = &node.children[a];
            let right = &node.children[b];
            right
                .size
                .cmp(&left.size)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.path.cmp(&right.path))
        });
        let sizes: Vec<u64> = order.iter().map(|&i| node.children[i].size).collect();
        let rects = treemap::squarify(
            &sizes,
            treemap::Rect::new(rect.left(), rect.top(), rect.width(), rect.height()),
        );
        let result = Rc::new(LayoutResult { order, rects });
        self.entries.insert(
            key,
            CachedLayout {
                result: Rc::clone(&result),
                last_used_generation: self.generation,
            },
        );
        #[cfg(test)]
        {
            self.misses += 1;
        }
        result
    }

    fn finish_frame(&mut self) {
        let generation = self.generation;
        self.entries
            .retain(|_, cached| cached.last_used_generation == generation);
    }

    fn clear(&mut self) {
        self.context = None;
        self.entries.clear();
    }

    #[cfg(test)]
    fn stats(&self) -> (usize, usize, usize) {
        (self.hits, self.misses, self.entries.len())
    }
}

/// One rendered treemap block that can be hovered/clicked, with the trail
/// of names leading to it from the focus node.
struct HitRect {
    rect: Rect,
    trail: Vec<String>,
    fs_path: PathBuf,
    is_dir: bool,
    size: u64,
    /// True only for a directory rendered as a single collapsed block (not a
    /// tray with its children nested in). These are the blocks the abstract
    /// posture's hover preview peeks into; files and expanded trays are false.
    collapsed: bool,
}

/// A cached hover-preview overlay: the pre-tessellated child shapes for the
/// collapsed directory block currently being peeked into, plus the key they
/// were built for. Rebuilt only when the hovered path, the tree revision, or
/// the block's on-screen rect changes — never per frame, mirroring the
/// `refresh_density`/`refresh_insights` caching discipline. `None` whenever
/// nothing eligible is hovered, so the preview naturally clears on pointer-out.
struct PreviewOverlay {
    /// (previewed dir path, tree revision, block rect rounded to whole pixels).
    key: (PathBuf, u64, [i32; 4]),
    shapes: Vec<egui::Shape>,
}

/// What moment the hidden `--debug-screenshot*` mode should capture.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DebugShotMode {
    /// After the scan completes and the final tree has rendered.
    Final,
    /// Mid-scan, while the map is still filling in live.
    Live,
    /// After completion, drilled into the root's largest directory child.
    Drill,
}

/// Drives the hidden `--debug-screenshot` mode: auto-scan a path, wait for
/// the chosen moment, capture one frame to a PNG, and exit. Exists so the
/// rendered UI can be verified in environments with no screen-capture tool.
pub struct DebugShot {
    pub out: PathBuf,
    pub scan: PathBuf,
    mode: DebugShotMode,
    started: bool,
    drilled: bool,
    frames_after_done: u32,
    requested: bool,
}

impl DebugShot {
    pub fn new(out: PathBuf, scan: PathBuf, mode: DebugShotMode) -> Self {
        Self {
            out,
            scan,
            mode,
            started: false,
            drilled: false,
            frames_after_done: 0,
            requested: false,
        }
    }
}

/// Snapshot of a finished scan's counters, copied out of `ScanProgress`
/// immediately before `ActiveScan` (and its atomics) are dropped, so the
/// bottom status bar can keep displaying them indefinitely afterward.
struct ScanSummary {
    files: u64,
    dirs: u64,
    bytes: u64,
    elapsed: Duration,
}

/// The Insights drawer's computed analytics for one focused structural revision.
/// Cached so the whole-subtree aggregations run once per change — not every
/// frame (see the change's design doc) — and cloned cheaply for rendering.
/// One exact filesystem target shared by treemap and Insights actions. The
/// trail is relative to the scan root, while `path` is the path passed to
/// Open, Reveal, and the recycle-bin operation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActionTarget {
    trail: Vec<String>,
    path: PathBuf,
    is_dir: bool,
    display_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InsightsKey {
    focus: Vec<String>,
    tree_identity: u64,
    focused_structural_rev: u64,
}

/// A delete request staged by either UI entry point. Staging this value does
/// not touch the filesystem; only the confirmation dialog can consume it.
type PendingDelete = ActionTarget;

#[derive(Clone, Default)]
struct InsightsData {
    ext_totals: Vec<(String, u64)>,
    leaderboard: Vec<insights::LeaderboardEntry>,
    blizzard: Vec<insights::BlizzardEntry>,
    cleanup_candidates: Vec<insights::CleanupCandidate>,
    /// The focused subtree's total size (`view.size`) — the denominator for
    /// each row's proportional fill bar, so bars read as "% of what I'm
    /// currently looking at" and rescale for free as focus changes.
    total_size: u64,
}

#[derive(Default)]
pub struct BytewhifferApp {
    path_input: String,
    root: Option<Node>,
    scan_controller: ScanController,
    /// Names from the root node down to the focused directory.
    focus: Vec<String>,
    /// Block or cleanup candidate the open context menu refers to.
    context_target: Option<ActionTarget>,
    /// Delete request awaiting explicit confirmation. No filesystem operation
    /// occurs while this is merely staged.
    pending_delete: Option<PendingDelete>,
    hovered_path: Option<PathBuf>,
    hovered_size: Option<u64>,
    error: Option<String>,
    debug_shot: Option<DebugShot>,
    /// Root path of the most recently started scan, kept after the scan
    /// completes (or fails) so Rescan can re-run it without retyping.
    last_scanned_path: Option<PathBuf>,
    /// The single target selected for the next scan action. This is updated
    /// when a folder is picked or when free-form text is resolved at action
    /// time; historical `last_scanned_path` never takes precedence over it.
    requested_target: Option<PathBuf>,
    /// Generation currently being scanned or prepared. The controller owns
    /// worker-side conversion before publishing completion, so this ID rejects
    /// stale completion messages.
    scan_generation: Option<ScanId>,
    /// Name of the engine that produced, or is producing, the current scan.
    engine_name: Option<&'static str>,
    scan_started_at: Option<Instant>,
    /// Last (time, bytes) sample used to derive `scan_rate_bps`, refreshed
    /// roughly once a second so the rate doesn't jitter between repaints.
    rate_sample: Option<(Instant, u64)>,
    scan_rate_bps: f64,
    /// Exponential moving average of `scan_rate_bps`, updated at the same
    /// ~1s cadence. Displayed instead of the raw per-second delta so the
    /// HUD's added rate precision (see `format_size_precise`) shows real
    /// trend rather than per-second sampling jitter. `None` until the first
    /// sample of a scan.
    smoothed_rate_bps: Option<f64>,
    /// Running per-top-level-child byte totals, updated once per discovery
    /// event so the largest child of the scan root can be tracked without
    /// re-walking the live tree.
    top_level_sizes: HashMap<String, u64>,
    biggest_top_level: Option<(String, u64)>,
    last_summary: Option<ScanSummary>,
    /// Whether the left-side Insights drawer is open. Closed by default so
    /// the treemap stays full-width until the user summons it.
    insights_open: bool,
    /// Bumped whenever `root` changes (scan start, live discovery, scan
    /// completion, deletion) so layout and density caches can tell their state
    /// is stale. Insights uses the focused node's structural revision instead
    /// of this global revision.
    tree_rev: u64,
    /// Monotonic identity for the currently installed root generation. A fresh
    /// authoritative tree can start its node revisions at the same values as
    /// the previous tree, so Insights must include this identity in its key.
    tree_identity: u64,
    /// Cached drawer analytics plus the focus, root identity, and focused-node
    /// structural revision they describe; recomputed only when that key changes.
    insights_cache: Option<InsightsData>,
    insights_key: Option<InsightsKey>,
    #[cfg(test)]
    insights_refreshes: usize,
    /// Cached "is the focused subtree dense enough for the cheap render tier?"
    /// decision, plus the (focus, tree_rev) it describes — recomputed only when
    /// that key changes, exactly like `insights_cache`. Keeps the descendant
    /// count off the per-frame path so pointer/hover tracking stays responsive.
    render_dense: bool,
    density_key: Option<(Vec<String>, u64)>,
    /// Render posture: 0.0 = detail (today's full nesting), rising toward 1.0 =
    /// abstract (fewer, larger blocks). Drives the frame's `NestGate` — a depth
    /// cap dropping toward 1 plus a rising size threshold — so branches collapse
    /// after fewer levels and small blocks fold away. Manual: the user drags the
    /// toolbar slider; there is no density-based auto-engage. `derive(Default)`
    /// would put this at 0.0, so every constructor below explicitly starts it at
    /// `1.0` (max abstraction) instead — the app opens on the collapsed overview
    /// rather than full detail. See `BytewhifferApp::nest_gate`.
    abstraction: f32,
    /// Cached hover-preview overlay for the collapsed directory block under
    /// the pointer in abstract mode; `None` when nothing eligible is hovered.
    /// Purely presentational — never touches `focus`/breadcrumb state.
    preview: Option<PreviewOverlay>,
    /// Reuses ordering and squarified rectangles across pointer-only repaints.
    /// Keys include each node's structural revision and the current focus,
    /// viewport, and resolved nesting gate, so mutations and posture changes
    /// invalidate only the layouts that can no longer be trusted.
    layout_cache: TreemapLayoutCache,
    /// Whether this process holds an elevated token. Detected once at startup
    /// (the UAC self-relaunch produces such a process); once true, turbo stays
    /// on for the rest of this process's lifetime and never re-prompts. Never
    /// persisted — a fresh launch re-detects from scratch. See the turbo-mode
    /// spec's "stays on for the elevated process's lifetime" requirement.
    turbo_elevated: bool,
    /// The MFT turbo engine's capability for the current scan target, recomputed
    /// on every scan start (i.e. every target change). `None` before any scan —
    /// `turbo_state` treats that as "assume NTFS" rather than checking eagerly,
    /// so the toggle isn't greyed out before a target even exists.
    turbo_availability: Option<Availability>,
    /// A scan root the elevated self-relaunch asked us to resume. Started on the
    /// first frame (scanning needs the running app), then cleared. Clean slate:
    /// only the root carries over, not navigation state.
    pending_scan: Option<PathBuf>,
    /// Whether the pre-UAC "Turbo needs administrator" confirmation dialog is
    /// open. Gates the elevation prompt on explicit user confirmation.
    turbo_warning_open: bool,
    /// Whether the "Turbo does not work for this drive" dialog is open (raised
    /// when an already-elevated process's target is non-NTFS).
    turbo_unsupported_open: bool,
}

impl BytewhifferApp {
    /// A normal launch. Detects the process's elevation once so a user who
    /// started Bytewhiffer from an elevated shell gets turbo without a relaunch.
    pub fn new() -> Self {
        Self {
            turbo_elevated: mft::process_is_elevated(),
            abstraction: 1.0,
            ..Self::default()
        }
    }

    pub fn with_debug_shot(shot: DebugShot) -> Self {
        Self {
            debug_shot: Some(shot),
            turbo_elevated: mft::process_is_elevated(),
            abstraction: 1.0,
            ..Self::default()
        }
    }

    /// The elevated relaunch's landing constructor: this process is elevated and
    /// resumes scanning `root` on the first frame, starting fresh (no restored
    /// navigation state).
    pub fn with_elevated_scan(root: PathBuf) -> Self {
        Self {
            pending_scan: Some(root),
            turbo_elevated: mft::process_is_elevated(),
            abstraction: 1.0,
            ..Self::default()
        }
    }
}

impl BytewhifferApp {
    fn start_scan(&mut self, target: PathBuf) {
        // A new scan makes any previously staged action stale; discard it
        // before provisional tree state starts changing again.
        self.context_target = None;
        self.cancel_pending_delete();
        self.requested_target = Some(target.clone());
        self.path_input = target.display().to_string();

        // Re-derive turbo capability for this target — the spec requires the
        // check be re-evaluated on every target change, never cached.
        let turbo_avail = MftEngine.is_available(&target);
        self.turbo_availability = Some(turbo_avail);

        // The single engine-selection point (task 7.1): an elevated process
        // uses the MFT turbo engine on NTFS targets and the walker everywhere
        // else. An elevated process pointed at a non-NTFS target also raises the
        // "turbo doesn't work here" warning rather than silently falling back.
        let engine: Box<dyn ScanEngine> =
            if self.turbo_elevated && turbo_avail == Availability::Available {
                Box::new(MftEngine)
            } else {
                if self.turbo_elevated && turbo_avail == Availability::UnsupportedFilesystem {
                    self.turbo_unsupported_open = true;
                }
                Box::new(WalkerEngine)
            };

        match engine.is_available(&target) {
            Availability::Available => {}
            other => {
                // The walker is always available, so this only guards a
                // misconfigured engine choice; surface it rather than scanning.
                self.error = Some(format!(
                    "The {} engine cannot scan this target: {:?}",
                    engine.name(),
                    other
                ));
                return;
            }
        }

        let engine_name = engine.name();
        // The UI keeps its own handles to the same cancel/progress state,
        // but not to the event sender — otherwise the channel would never
        // disconnect when the scan thread finishes.

        let root_name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.to_string_lossy().into_owned());
        self.replace_root(Some(Node::new(root_name, target.clone(), 0, true)));
        self.focus.clear();
        self.hovered_path = None;
        self.hovered_size = None;
        self.last_scanned_path = Some(target.clone());
        self.engine_name = Some(engine_name);
        self.scan_started_at = Some(Instant::now());
        self.rate_sample = None;
        self.scan_rate_bps = 0.0;
        self.smoothed_rate_bps = None;
        self.top_level_sizes.clear();
        self.biggest_top_level = None;
        self.layout_cache.clear();
        self.tree_rev = self.tree_rev.wrapping_add(1);

        let id = self.scan_controller.start(target, engine);
        self.scan_generation = Some(id);
    }

    fn drain_scan(&mut self) {
        let events = self.scan_controller.take_events(SCAN_FRAME_BUDGET);
        let current_id = self.scan_controller.current_id();
        let mut discovered_any = false;
        if let Some(root) = &mut self.root {
            let base = root.path.clone();
            for event in events {
                let ScanEvent::Discovered {
                    scan_id,
                    path,
                    size,
                    is_dir,
                } = event;
                if Some(scan_id) != current_id {
                    continue;
                }
                discovered_any = true;
                if let Ok(rel) = path.strip_prefix(&base) {
                    if let Some(first) = rel.components().next() {
                        let top_name = first.as_os_str().to_string_lossy().into_owned();
                        let entry = self.top_level_sizes.entry(top_name.clone()).or_insert(0);
                        *entry += size;
                        let total = *entry;
                        let is_new_max = self
                            .biggest_top_level
                            .as_ref()
                            .is_none_or(|(_, max)| total > *max);
                        if is_new_max {
                            self.biggest_top_level = Some((top_name, total));
                        }
                    }
                    root.insert(rel, size, is_dir);
                }
            }
        }
        if discovered_any {
            self.tree_rev = self.tree_rev.wrapping_add(1);
        }

        let now = Instant::now();
        if let Some(progress) = self.scan_controller.current_progress() {
            let bytes_now = progress.bytes_scanned.load(Ordering::Relaxed);
            match self.rate_sample {
                None => self.rate_sample = Some((now, bytes_now)),
                Some((t, b)) => {
                    let dt = now.duration_since(t).as_secs_f64();
                    if dt >= 1.0 {
                        let raw = bytes_now.saturating_sub(b) as f64 / dt;
                        self.scan_rate_bps = raw;
                        self.smoothed_rate_bps = Some(match self.smoothed_rate_bps {
                            Some(prev) => prev * (1.0 - RATE_EMA_ALPHA) + raw * RATE_EMA_ALPHA,
                            None => raw,
                        });
                        self.rate_sample = Some((now, bytes_now));
                    }
                }
            }
        }

        if let Some(completion) = self.scan_controller.poll_completion() {
            self.finish_scan(completion);
        }
    }

    fn finish_scan(&mut self, completion: ScanCompletion) {
        if self.scan_generation != Some(completion.id) {
            return;
        }
        let files = completion.progress.files_scanned.load(Ordering::Relaxed);
        let dirs = completion.progress.dirs_scanned.load(Ordering::Relaxed);
        let bytes = completion.progress.bytes_scanned.load(Ordering::Relaxed);
        let elapsed = self
            .scan_started_at
            .map(|t| t.elapsed())
            .unwrap_or_default();
        self.engine_name = Some(completion.engine_name);
        self.layout_cache.clear();

        match completion.outcome {
            PreparedOutcome::Success(root) => {
                // The controller prepares the complete display tree before
                // publishing this message. Install it atomically so the
                // provisional live tree is never replaced by a partial one.
                self.replace_root(Some(root));
                if let Some(root) = &self.root {
                    if root.find(&self.focus).is_none() {
                        self.focus.clear();
                    }
                }
                self.last_summary = Some(ScanSummary {
                    files,
                    dirs,
                    bytes,
                    elapsed,
                });
                self.tree_rev = self.tree_rev.wrapping_add(1);
            }
            PreparedOutcome::Cancelled => {
                self.replace_root(None);
                self.focus.clear();
                self.tree_rev = self.tree_rev.wrapping_add(1);
            }
            PreparedOutcome::Failed(err) => {
                self.error = Some(match err {
                    ScanError::Unavailable(a) => {
                        format!("Scan engine unavailable for this target: {a:?}")
                    }
                    ScanError::RootUnreadable(e) => format!("Cannot read that folder: {e}"),
                });
                self.replace_root(None);
                self.tree_rev = self.tree_rev.wrapping_add(1);
                self.last_summary = Some(ScanSummary {
                    files,
                    dirs,
                    bytes,
                    elapsed,
                });
            }
            PreparedOutcome::Panicked => {
                self.error = Some("The scan thread panicked.".to_owned());
                self.replace_root(None);
                self.tree_rev = self.tree_rev.wrapping_add(1);
                self.last_summary = Some(ScanSummary {
                    files,
                    dirs,
                    bytes,
                    elapsed,
                });
            }
        }
    }

    /// Whether the HUD-visible "still working" state should be considered
    /// active. The controller keeps a generation current through both the
    /// scanner walk and worker-side display-tree preparation, so completion
    /// is the single point at which the HUD can disappear.
    fn scan_active(&self) -> bool {
        self.scan_controller.is_active()
    }

    fn replace_root(&mut self, root: Option<Node>) {
        self.root = root;
        self.tree_identity = self.tree_identity.wrapping_add(1);
        if self.tree_identity == 0 {
            self.tree_identity = 1;
        }
    }

    fn delete_available(&self) -> bool {
        !self.scan_active()
    }

    /// Cancels a staged delete without touching the filesystem or visible tree.
    fn cancel_pending_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Drops a staged delete when a scan or authoritative-tree assembly has
    /// made the target stale. Returns whether a pending request was cleared.
    fn clear_pending_delete_if_unavailable(&mut self) -> bool {
        if self.pending_delete.is_some() && !self.delete_available() {
            self.cancel_pending_delete();
            true
        } else {
            false
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;

            if chrome_button(ui, "📁 Pick folder…", true).clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.path_input = folder.to_string_lossy().into_owned();
                    self.start_scan(folder);
                }
            }

            // Path field: a recessed (darkened) card background with a
            // frameless text edit placed on top, so it wears the same
            // radius/shadow language as the buttons and the map.
            let (field_rect, _) = ui.allocate_exact_size(Vec2::new(320.0, 28.0), Sense::hover());
            paint_surface(
                ui.painter(),
                field_rect,
                theme::CHROME_BASE.lerp_to_gamma(egui::Color32::BLACK, 0.35),
            );
            let inner = field_rect.shrink2(Vec2::new(8.0, 4.0));
            let edit = egui::TextEdit::singleline(&mut self.path_input)
                .hint_text("…or type a path")
                .frame(egui::Frame::NONE);
            let path_response = ui.put(inner, edit);
            let submitted =
                path_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (chrome_button(ui, "Scan", true).clicked() || submitted)
                && !self.path_input.trim().is_empty()
            {
                if let Some(path) = self.resolve_requested_target() {
                    self.start_scan(path);
                }
            }

            let rescan_available =
                self.requested_target.is_some() || !self.path_input.trim().is_empty();
            if chrome_button(ui, "Rescan", rescan_available).clicked() {
                if let Some(path) = self.resolve_requested_target() {
                    self.start_scan(path);
                }
            }

            // Turbo toggle: its look and click behavior both come from
            // `turbo_state` (greyed / promptable / active / warning-red).
            let state = self.turbo_state();
            let label = match state {
                TurboState::Active => "⚡ Turbo ✓",
                TurboState::WarnUnsupported => "⚡ Turbo ⚠",
                _ => "⚡ Turbo",
            };
            let hover = match state {
                TurboState::Disabled => {
                    "Turbo mode needs a local NTFS drive (scan one to enable it)."
                }
                TurboState::Promptable => "Enable faster NTFS scanning (needs administrator).",
                TurboState::Active => "Turbo mode is on — scanning via the NTFS Master File Table.",
                TurboState::WarnUnsupported => "This drive isn't NTFS — turbo can't apply here.",
            };
            let resp = turbo_toggle(ui, label, state).on_hover_text(hover);
            if resp.clicked() {
                let _ = self.resolve_requested_target();
                match state {
                    TurboState::Promptable => {
                        // No target yet (nothing scanned, nothing typed): let the
                        // click double as picking a folder instead of dead-ending
                        // in a "pick a folder first" error once the warning
                        // dialog is confirmed. Deliberately do NOT kick off a
                        // walker scan here — the elevated relaunch is about to do
                        // the real MFT scan, so a throwaway scan (and closing the
                        // window mid-scan to relaunch) would just be jank. Only
                        // record the chosen path so `trigger_elevation` can pass
                        // it through, then open the warning dialog.
                        if self.requested_target.is_none() && self.path_input.trim().is_empty() {
                            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                                self.path_input = folder.display().to_string();
                                self.requested_target = Some(folder.clone());
                                self.turbo_availability = Some(MftEngine.is_available(&folder));
                                self.turbo_warning_open = true;
                            }
                        } else {
                            self.turbo_warning_open = true;
                        }
                    }
                    TurboState::WarnUnsupported => self.turbo_unsupported_open = true,
                    // Disabled never senses clicks; Active is already on.
                    TurboState::Disabled | TurboState::Active => {}
                }
            }

            let insights_label = if self.insights_open {
                "📊 Insights ◂"
            } else {
                "📊 Insights ▸"
            };
            if chrome_button(ui, insights_label, true).clicked() {
                self.insights_open = !self.insights_open;
            }

            // Render-posture slider: detail (left, today's nesting) → abstract
            // (right, fewer/larger blocks). Manual only; drives `nest_scale`.
            ui.colored_label(theme::TEXT_SUBTLE, "Detail");
            ui.add(
                egui::Slider::new(&mut self.abstraction, 0.0..=1.0)
                    .show_value(false)
                    .trailing_fill(true),
            );
            ui.colored_label(theme::TEXT_SUBTLE, "Abstract");

            if self.scan_controller.is_active() {
                if chrome_button(ui, "Cancel", true).clicked() {
                    self.scan_controller.cancel_current();
                }
                ui.spinner();
            }
        });
    }

    /// In-flight scan HUD. The controller remains active while either the
    /// scanner walk or worker-side display-tree preparation runs. Preparation
    /// publishes conversion progress through the shared progress state, so
    /// the UI never owns the conversion work or a partial authoritative tree.
    fn scan_hud(&mut self, ui: &mut egui::Ui) {
        if !self.scan_active() {
            return;
        }
        let elapsed = self
            .scan_started_at
            .map(|t| t.elapsed())
            .unwrap_or_default();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;

            let Some(progress) = self.scan_controller.current_progress() else {
                mono_label(ui, theme::TEXT_SUBTLE, format_duration_live(elapsed));
                return;
            };
            let files = progress.files_scanned.load(Ordering::Relaxed);
            let dirs = progress.dirs_scanned.load(Ordering::Relaxed);
            let bytes = progress.bytes_scanned.load(Ordering::Relaxed);

            if progress.conversion_started() {
                ui.add(
                    egui::ProgressBar::new(progress.conversion_progress())
                        .desired_width(110.0)
                        .desired_height(6.0)
                        .fill(theme::ACCENT),
                );
                ui.colored_label(theme::TEXT_SUBTLE, "Preparing…");
                mono_label(
                    ui,
                    theme::TEXT_SUBTLE,
                    format!(
                        "{} files · {} dirs · {}",
                        files,
                        dirs,
                        format_size_precise(bytes)
                    ),
                );
            } else {
                let rate = self.smoothed_rate_bps.unwrap_or(0.0);
                let biggest = self.biggest_top_level.clone();

                // Motion only — no fill level tied to completion, since the
                // parallel walker has no way to know a scan's total size
                // ahead of time. `0.5` is an arbitrary constant, not a
                // fraction of anything real.
                ui.add(
                    egui::ProgressBar::new(0.5)
                        .animate(true)
                        .desired_width(110.0)
                        .desired_height(6.0)
                        .fill(theme::ACCENT),
                );

                mono_label(
                    ui,
                    theme::TEXT_SUBTLE,
                    format!(
                        "{} files · {} dirs · {}",
                        files,
                        dirs,
                        format_size_precise(bytes)
                    ),
                );
                mono_label(
                    ui,
                    theme::TEXT_SUBTLE,
                    format!("{}/s", format_size_precise(rate as u64)),
                );
                if let Some((name, size)) = biggest {
                    ui.colored_label(
                        theme::TEXT_SUBTLE,
                        format!("Largest so far: {name} ({})", format_size(size)),
                    );
                }
            }

            mono_label(ui, theme::TEXT_SUBTLE, format_duration_live(elapsed));
        });
    }

    /// Persistent bottom status bar: a hover readout on the left (mirrors
    /// the block tooltip but never disappears), and on the right a scan
    /// summary that survives past scan completion plus the engine name.
    /// Goes quiet about live counts while a scan or worker-side tree
    /// preparation is in progress, since the HUD above already owns those.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            match (&self.hovered_path, self.hovered_size) {
                (Some(path), Some(size)) => {
                    ui.colored_label(theme::TEXT, path.display().to_string());
                    ui.colored_label(theme::TEXT_SUBTLE, format_size(size));
                }
                _ => {
                    ui.colored_label(theme::TEXT_SUBTLE, "Hover a block to inspect");
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(name) = self.engine_name {
                    ui.colored_label(theme::TEXT_SUBTLE, name);
                }
                if self.scan_controller.is_active() {
                    ui.colored_label(theme::TEXT_SUBTLE, "Scanning…");
                } else if let Some(summary) = &self.last_summary {
                    ui.colored_label(
                        theme::TEXT_SUBTLE,
                        format!(
                            "{} files · {} dirs · {} · {}",
                            summary.files,
                            summary.dirs,
                            format_size(summary.bytes),
                            format_duration(summary.elapsed)
                        ),
                    );
                }
            });
        });
    }

    fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        let Some(root) = &self.root else { return };
        let mut new_focus: Option<Vec<String>> = None;

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let back = chrome_button(ui, "⬅", !self.focus.is_empty());
            if back.clicked() {
                let mut f = self.focus.clone();
                f.pop();
                new_focus = Some(f);
            }

            // Root crumb, then one crumb per focused level, each an elevated
            // chip. The *current* level is the one place (besides hover and
            // selection) that wears the accent color.
            let at_root = self.focus.is_empty();
            if chrome_chip(ui, &root.name, at_root).clicked() {
                new_focus = Some(Vec::new());
            }

            for (i, name) in self.focus.iter().enumerate() {
                ui.colored_label(theme::TEXT_SUBTLE, "›");
                let is_current = i == self.focus.len() - 1;
                if chrome_chip(ui, name, is_current).clicked() {
                    new_focus = Some(self.focus[..=i].to_vec());
                }
            }
        });

        if let Some(f) = new_focus {
            self.focus = f;
            self.hovered_path = None;
            self.hovered_size = None;
        }
    }

    fn treemap_panel(&mut self, ui: &mut egui::Ui) {
        // Decide the render tier before borrowing `root`: on a dense focused
        // subtree, card-eligible blocks drop to a cheap flat-rounded fill for
        // the whole frame so per-frame tessellation (and thus hover/pointer
        // tracking) stays responsive. Cached, so this is a field read here.
        self.refresh_density();
        let dense = self.render_dense;

        let avail = ui.available_rect_before_wrap();
        let gate = self.nest_gate();
        let response = ui.allocate_rect(avail, Sense::click());
        let painter = ui.painter_at(avail);
        painter.rect_filled(avail, 0.0, theme::BG);

        let Some(root) = &self.root else {
            painter.text(
                avail.center(),
                Align2::CENTER_CENTER,
                "Pick a folder to see where your bytes went",
                FontId::proportional(16.0),
                theme::TEXT_SUBTLE,
            );
            return;
        };

        let focus_node = match root.find(&self.focus) {
            Some(node) => node,
            None => {
                self.focus.clear();
                root
            }
        };

        if focus_node.children.is_empty() {
            self.layout_cache.clear();
            let msg = if self.scan_controller.is_active() {
                "Scanning…"
            } else {
                "Nothing here"
            };
            painter.text(
                avail.center(),
                Align2::CENTER_CENTER,
                msg,
                FontId::proportional(16.0),
                theme::TEXT_SUBTLE,
            );
            return;
        }

        let mut hits: Vec<HitRect> = Vec::new();
        let treemap_rect = avail.shrink(BLOCK_PAD);
        self.layout_cache
            .begin_frame(&self.focus, treemap_rect, gate);
        draw_children(
            &painter,
            focus_node,
            treemap_rect,
            0,
            &mut Vec::new(),
            &mut hits,
            dense,
            gate,
            &mut self.layout_cache,
        );
        self.layout_cache.finish_frame();

        // Deepest block under the pointer wins: children are pushed after
        // their parents, so the last containing rect is the innermost.
        let hover_pos = response.hover_pos();
        let hovered = hover_pos.and_then(|pos| hits.iter().rev().find(|h| h.rect.contains(pos)));

        self.hovered_path = hovered.map(|h| h.fs_path.clone());
        self.hovered_size = hovered.map(|h| h.size);
        if let Some(hit) = hovered {
            // Abstract-posture hover preview: peek inside a collapsed directory
            // block without drilling in. Painted under the accent frame below so
            // that frame reads as the preview's border. Purely presentational —
            // it never mutates `self.focus`/breadcrumb, and clicking still drills
            // (handled unchanged further down). The overlay shapes are cached on
            // (path, tree_rev, block rect) so a stationary hover isn't re-laid
            // out every frame. Any non-eligible hover clears the cache.
            let preview_node = (self.abstraction > 0.0 && hit.collapsed)
                .then(|| focus_node.find(&hit.trail))
                .flatten()
                .filter(|n| !n.children.is_empty());
            if let Some(node) = preview_node {
                let outer = hit.rect;
                let key = (
                    hit.fs_path.clone(),
                    self.tree_rev,
                    [
                        outer.left() as i32,
                        outer.top() as i32,
                        outer.width() as i32,
                        outer.height() as i32,
                    ],
                );
                if self.preview.as_ref().map(|p| &p.key) != Some(&key) {
                    let mut shapes = Vec::new();
                    build_preview_shapes(node, outer.shrink(PREVIEW_INSET), 1, &mut shapes);
                    self.preview = Some(PreviewOverlay { key, shapes });
                }
                // Repaint the block's fill so the collapsed rendering beneath
                // doesn't bleed through, then the cached child shapes on top.
                painter.rect_filled(outer, theme::CARD_CORNER_RADIUS, theme::BG);
                if let Some(p) = &self.preview {
                    painter.extend(p.shapes.iter().cloned());
                }
            } else {
                self.preview = None;
            }

            painter.rect_stroke(
                hit.rect,
                theme::CARD_CORNER_RADIUS,
                Stroke::new(1.5, theme::ACCENT),
                StrokeKind::Inside,
            );
            egui::Tooltip::always_open(
                ui.ctx().clone(),
                ui.layer_id(),
                egui::Id::new("block_tooltip"),
                egui::PopupAnchor::Pointer,
            )
            .show(|ui| {
                // Elide the middle of long trails and force a single line: with
                // `PopupAnchor::Pointer` the popup gets squeezed against the
                // viewport edge, and a raw slash-joined path (no spaces to break
                // on) would otherwise hard-wrap into a one-glyph-per-line column.
                // The full, unelided path still shows in the bottom status bar.
                let trail = elide_middle(&hit.trail.join("/"), TOOLTIP_MAX_CHARS);
                ui.add(
                    egui::Label::new(egui::RichText::new(trail).strong())
                        .wrap_mode(egui::TextWrapMode::Extend),
                );
                ui.colored_label(theme::TEXT_SUBTLE, format_size(hit.size));
            });

            if response.clicked() && hit.is_dir {
                let mut focus = self.focus.clone();
                focus.extend(hit.trail.iter().cloned());
                self.focus = focus;
            }
            if response.secondary_clicked() {
                self.context_target = Some(action_target_from_treemap_hit(&self.focus, hit));
            }
        } else {
            // Pointer is over no block — discard any open preview.
            self.preview = None;
        }

        response.context_menu(|ui| self.context_menu_contents(ui));
    }

    fn context_menu_contents(&mut self, ui: &mut egui::Ui) {
        let Some(target) = self.context_target.clone() else {
            ui.close();
            return;
        };

        ui.label(egui::RichText::new(&target.display_name).color(theme::TEXT_SUBTLE));
        ui.colored_label(theme::TEXT_SUBTLE, target.path.display().to_string());
        ui.separator();

        if ui.button("Open").clicked() {
            if let Err(err) = open::that_detached(&target.path) {
                self.error = Some(format!("Could not open {}: {err}", target.path.display()));
            }
            ui.close();
        }

        if ui.button("Reveal in Explorer").clicked() {
            if let Err(err) = reveal_in_file_manager(&target.path) {
                self.error = Some(format!("Could not reveal {}: {err}", target.path.display()));
            }
            ui.close();
        }

        ui.separator();

        let delete_available = self.delete_available();
        let delete_button = ui.add_enabled(delete_available, egui::Button::new("🗑 Delete"));
        if !delete_available {
            ui.colored_label(
                theme::TEXT_SUBTLE,
                "Delete is available after the tree is stable.",
            );
        }
        if delete_button.clicked() {
            // Staging is deliberately the only effect of this menu action.
            // The confirmation window performs the filesystem operation only
            // after the user explicitly confirms this exact target.
            self.pending_delete = Some(target);
            self.context_target = None;
            ui.close();
        }
    }

    fn confirm_pending_delete(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending_delete.clone() else {
            return;
        };

        // A scan or authoritative assembly can begin while a modal is open
        // (for example through another UI action). Never confirm against a
        // provisional tree; cancel the stale request and make the user stage
        // it again after the lifecycle settles.
        if self.clear_pending_delete_if_unavailable() {
            return;
        }

        let response = egui::Modal::new(egui::Id::new("confirm_delete")).show(ctx, |ui| {
            ui.heading(if target.is_dir {
                "Send folder to the recycle bin?"
            } else {
                "Send file to the recycle bin?"
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(&target.display_name).strong());
            ui.colored_label(theme::TEXT_SUBTLE, target.path.display().to_string());
            ui.add_space(4.0);
            ui.label(
                "This sends the exact item to the Windows recycle bin. Review the path before confirming.",
            );
            ui.add_space(8.0);

            let mut confirmed = false;
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    confirmed = true;
                }
            });
            confirmed
        });

        if response.should_close() {
            self.pending_delete = None;
        } else if response.inner {
            if !self.delete_available() {
                self.pending_delete = None;
            } else if let Some(target) = self.pending_delete.take() {
                self.delete_confirmed(target);
            }
        }
    }

    fn delete_confirmed(&mut self, target: ActionTarget) {
        let result = trash::delete(&target.path).map_err(|err| err.to_string());
        self.apply_delete_result(&target, result);
    }

    /// Applies the result of the filesystem operation to the visible tree.
    /// Keeping this separate from `trash::delete` makes the success/failure
    /// contract deterministic to test without touching a real recycle bin.
    fn apply_delete_result(&mut self, target: &ActionTarget, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let removed = self
                    .root
                    .as_mut()
                    .map(|root| root.remove(&target.trail))
                    .unwrap_or(false);
                if removed {
                    self.tree_rev = self.tree_rev.wrapping_add(1);
                    self.repair_focus_after_removal(&target.trail);
                }
            }
            Err(err) => {
                self.error = Some(format!(
                    "Could not send {} to the recycle bin: {err}",
                    target.path.display()
                ));
            }
        }
    }

    fn repair_focus_after_removal(&mut self, removed_trail: &[String]) {
        if self.focus.starts_with(removed_trail) {
            self.focus.truncate(removed_trail.len().saturating_sub(1));
        }
        if let Some(root) = &self.root {
            while root.find(&self.focus).is_none() && !self.focus.is_empty() {
                self.focus.pop();
            }
        } else {
            self.focus.clear();
        }
        self.hovered_path = None;
        self.hovered_size = None;
    }

    /// Recomputes whether the focused subtree is dense enough to warrant the
    /// cheap (flat-rounded) render tier when focus or tree structure changes;
    /// a no-op otherwise. The descendant count is maintained by `DisplayNode`,
    /// so pointer-only frames read cached metadata without a subtree walk.
    fn refresh_density(&mut self) {
        let key = (self.focus.clone(), self.tree_rev);
        if self.density_key.as_ref() == Some(&key) {
            return;
        }
        let count = self
            .root
            .as_ref()
            .and_then(|root| root.find(&self.focus))
            .map(|node| node.descendant_count())
            .unwrap_or(0);
        self.render_dense = count > DENSE_RENDER_THRESHOLD;
        self.density_key = Some(key);
    }

    /// The render posture's resolved nesting gate for this frame. See
    /// `resolve_nest_gate` for the mapping.
    fn nest_gate(&self) -> NestGate {
        resolve_nest_gate(self.abstraction)
    }

    /// Recomputes the drawer's analytics if the focus, authoritative root, or
    /// focused subtree structure has changed since they were last computed; a
    /// no-op otherwise. Keeps the whole-subtree walks off the per-frame render
    /// path.
    fn refresh_insights(&mut self) {
        let focused_structural_rev = self
            .root
            .as_ref()
            .map(|root| root.find(&self.focus).unwrap_or(root).structural_rev())
            .unwrap_or(0);
        let key = InsightsKey {
            focus: self.focus.clone(),
            tree_identity: self.tree_identity,
            focused_structural_rev,
        };
        if self.insights_cache.is_some() && self.insights_key.as_ref() == Some(&key) {
            return;
        }
        #[cfg(test)]
        {
            self.insights_refreshes += 1;
        }
        let data = {
            let Some(root) = &self.root else {
                self.insights_cache = None;
                self.insights_key = Some(key);
                return;
            };
            // Describe whatever the treemap is currently showing — the same
            // node `treemap_panel` resolves via `root.find(&self.focus)`.
            // `aggregate` visits this borrowed tree once for every drawer
            // section and keeps only the bounded leaderboard candidates.
            let focus_node = root.find(&self.focus).unwrap_or(root);
            let summary = insights::aggregate(focus_node, LEADERBOARD_N);
            InsightsData {
                ext_totals: summary.ext_totals,
                leaderboard: summary.leaderboard,
                blizzard: summary.blizzard,
                cleanup_candidates: summary.cleanup_candidates,
                total_size: summary.total_size,
            }
        };
        self.insights_cache = Some(data);
        self.insights_key = Some(key);
    }

    /// Renders the Insights drawer: an extension legend + size breakdown, a
    /// biggest-items leaderboard, a small-file-blizzard flag list, and
    /// advisory cleanup candidates — all describing the focused subtree.
    /// Clicking a leaderboard/blizzard entry navigates the treemap;
    /// right-clicking a cleanup candidate opens the same Delete/Open/Reveal
    /// menu as a treemap block.
    fn insights_panel(&mut self, ui: &mut egui::Ui) {
        self.refresh_insights();

        ui.add_space(4.0);
        ui.heading("Insights");
        ui.add_space(2.0);

        // No scan has ever produced a tree: a neutral placeholder, mirroring
        // the treemap's own "Pick a folder…" empty state.
        let Some(data) = self.insights_cache.clone() else {
            ui.colored_label(theme::TEXT_SUBTLE, "Run a scan to see insights here.");
            return;
        };

        // Navigation/actions triggered this frame are staged and applied
        // after rendering so the loops keep reading a stable focus base.
        let base = self.focus.clone();
        let mut new_focus: Option<Vec<String>> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // --- File types: legend + size breakdown in one list. Each
                // row's swatch is the exact color the treemap paints that
                // extension, so the two can never drift apart. ---
                insights_header(ui, "File types");
                if data.ext_totals.is_empty() {
                    insights_empty(ui, "No files in view.");
                } else {
                    for (ext, size) in &data.ext_totals {
                        let fraction = if data.total_size > 0 {
                            *size as f64 / data.total_size as f64
                        } else {
                            0.0
                        };
                        insights_bar_row(ui, fraction, |ui| {
                            swatch(ui, theme::color_for_extension(ext));
                            let label = if ext.is_empty() {
                                "(no extension)".to_owned()
                            } else {
                                format!(".{ext}")
                            };
                            ui.colored_label(theme::TEXT, label);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| ui.colored_label(theme::TEXT_SUBTLE, format_size(*size)),
                            );
                        });
                    }
                }
                ui.add_space(10.0);

                // --- Biggest items leaderboard. Clicking focuses the map on
                // the entry (its parent, for a file). ---
                insights_header(ui, "Biggest items");
                if data.leaderboard.is_empty() {
                    insights_empty(ui, "Nothing to rank yet.");
                } else {
                    for entry in &data.leaderboard {
                        let fraction = if data.total_size > 0 {
                            entry.size as f64 / data.total_size as f64
                        } else {
                            0.0
                        };
                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let mut clicked = false;
                        insights_bar_row(ui, fraction, |ui| {
                            let resp = ui
                                .selectable_label(
                                    false,
                                    format!(
                                        "{icon} {}  ·  {}",
                                        entry.name,
                                        format_size(entry.size)
                                    ),
                                )
                                .on_hover_text(entry.path.display().to_string());
                            clicked = resp.clicked();
                        });
                        if clicked {
                            new_focus = Some(focus_for(&base, &entry.trail, entry.is_dir));
                        }
                    }
                }
                ui.add_space(10.0);

                // --- Small-file blizzard flags. Clicking focuses the dir. ---
                insights_header(ui, "Small-file clutter");
                if data.blizzard.is_empty() {
                    insights_empty(ui, "No dense small-file folders.");
                } else {
                    for entry in &data.blizzard {
                        let resp = ui.selectable_label(
                            false,
                            format!(
                                "📁 {}  ·  {} items, {} avg",
                                entry.name,
                                entry.child_count,
                                format_size(entry.avg_child_size)
                            ),
                        );
                        if resp.clicked() {
                            new_focus = Some(focus_for(&base, &entry.trail, true));
                        }
                    }
                }
                ui.add_space(10.0);

                // --- Cleanup candidates. These are name-based heuristics,
                // not proof that an item is safe to delete. Right-clicking
                // opens the same actions as a treemap block. ---
                insights_header(ui, "Cleanup candidates");
                if data.cleanup_candidates.is_empty() {
                    insights_empty(ui, "No cleanup candidates found.");
                } else {
                    ui.colored_label(
                        theme::TEXT_SUBTLE,
                        "Right-click for Open / Reveal / Delete.",
                    );
                    for entry in &data.cleanup_candidates {
                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let classification = entry.classification;
                        let resp = ui.selectable_label(
                            false,
                            format!(
                                "{icon} {}  ·  {} · {}",
                                entry.name,
                                classification.confidence.label(),
                                format_size(entry.size)
                            ),
                        );
                        ui.colored_label(
                            theme::TEXT_SUBTLE,
                            format!(
                                "{} — {}",
                                classification.category.label(),
                                classification.reason
                            ),
                        );
                        if resp.secondary_clicked() {
                            self.context_target =
                                Some(action_target_from_cleanup_candidate(&base, entry));
                        }
                        resp.context_menu(|ui| self.context_menu_contents(ui));
                    }
                }
            });

        if let Some(f) = new_focus {
            self.focus = f;
            self.hovered_path = None;
            self.hovered_size = None;
        }
    }

    fn error_window(&mut self, ctx: &egui::Context) {
        let Some(message) = self.error.clone() else {
            return;
        };
        let mut dismissed = false;
        egui::Window::new("Something went wrong")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(message);
                if ui.button("OK").clicked() {
                    dismissed = true;
                }
            });
        if dismissed {
            self.error = None;
        }
    }

    fn requested_target_candidate(&self) -> Option<PathBuf> {
        let typed = self.path_input.trim();
        if typed.is_empty() {
            return self.requested_target.clone();
        }
        if let Some(target) = &self.requested_target {
            // `path_input` still shows exactly the display text a prior
            // start_scan/picker/turbo-elevation action wrote for `target` —
            // no user edit has happened since. Return the stored PathBuf
            // instead of reparsing its (possibly lossy) display rendering,
            // so a picker/CLI path survives Rescan/Turbo unchanged.
            if target.display().to_string() == self.path_input {
                return Some(target.clone());
            }
        }
        Some(PathBuf::from(typed))
    }

    fn resolve_requested_target(&mut self) -> Option<PathBuf> {
        let target = self.requested_target_candidate()?;
        self.requested_target = Some(target.clone());
        self.turbo_availability = Some(MftEngine.is_available(&target));
        Some(target)
    }

    /// The Turbo toggle's current state, derived from the current requested
    /// target rather than historical scan output. Typed input is resolved for
    /// this action path so capability checks cannot stay stale when the user
    /// changes drives before elevating.
    fn turbo_state(&self) -> TurboState {
        let availability = self
            .requested_target_candidate()
            .map(|target| MftEngine.is_available(&target))
            .or(self.turbo_availability);
        match availability {
            // No scan has run yet, so the NTFS check hasn't happened at all —
            // assume the common case (NTFS) rather than greying the toggle out
            // pre-emptively. `start_scan` re-derives the real availability on
            // the first scan and flips this to `WarnUnsupported`/`Disabled` if
            // the target turns out not to be NTFS.
            None if self.turbo_elevated => TurboState::Active,
            None => TurboState::Promptable,
            Some(Availability::Available) => TurboState::Active,
            Some(Availability::RequiresElevation) => TurboState::Promptable,
            Some(Availability::UnsupportedFilesystem) | Some(Availability::NotApplicable) => {
                // An elevated process on a non-NTFS drive gets the distinct
                // warning state; an unelevated one just can't use turbo here.
                if self.turbo_elevated {
                    TurboState::WarnUnsupported
                } else {
                    TurboState::Disabled
                }
            }
        }
    }

    /// The pre-UAC confirmation dialog: turbo needs administrator rights, and
    /// accepting relaunches the app elevated. The OS elevation prompt is never
    /// triggered without this intermediate confirmation (turbo-mode spec).
    fn turbo_warning_window(&mut self, ctx: &egui::Context) {
        if !self.turbo_warning_open {
            return;
        }
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Enable Turbo mode")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Turbo mode reads the NTFS Master File Table directly, for a much faster \
                     scan on large drives.",
                );
                ui.add_space(4.0);
                ui.colored_label(
                    theme::TEXT_SUBTLE,
                    "It needs administrator privileges. Windows will ask you to confirm, then \
                     Bytewhiffer relaunches elevated and re-scans the current folder from scratch.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Continue").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        // Dismissing without confirming must not elevate (turbo-mode spec).
        if cancel {
            self.turbo_warning_open = false;
        }
        if confirm {
            self.turbo_warning_open = false;
            self.trigger_elevation(ctx);
        }
    }

    /// Fires the OS elevation prompt via a self-relaunch. Accepting closes this
    /// (unelevated) process so the fresh elevated one takes over; declining UAC
    /// leaves this process running unchanged (the toggle returns to promptable).
    fn trigger_elevation(&mut self, ctx: &egui::Context) {
        let root = self.resolve_requested_target();
        let Some(root) = root else {
            self.error = Some("Pick a folder to scan before enabling Turbo mode.".to_owned());
            return;
        };
        match mft::relaunch_elevated(&root) {
            Ok(true) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Ok(false) => {
                // User declined UAC: nothing to do — stay unelevated. The
                // toggle is still promptable, so they can try again.
            }
            Err(err) => {
                self.error = Some(format!("Could not start Turbo mode: {err}"));
            }
        }
    }

    /// The "Turbo does not work for this drive" dialog, shown when an
    /// already-elevated process's target is non-NTFS. The scan itself already
    /// completed via the walker fallback; this only explains why turbo didn't
    /// engage.
    fn turbo_unsupported_window(&mut self, ctx: &egui::Context) {
        if !self.turbo_unsupported_open {
            return;
        }
        let mut dismissed = false;
        egui::Window::new("Turbo mode unavailable for this drive")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    "Turbo mode only works on local NTFS volumes. This drive isn't NTFS, so \
                     Bytewhiffer scanned it with the standard directory walker instead.",
                );
                ui.add_space(6.0);
                if ui.button("OK").clicked() {
                    dismissed = true;
                }
            });
        if dismissed {
            self.turbo_unsupported_open = false;
        }
    }
}

impl BytewhifferApp {
    fn debug_shot_tick(&mut self, ctx: &egui::Context) {
        if self.debug_shot.is_none() {
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(50));

        let needs_start = matches!(&self.debug_shot, Some(s) if !s.started);
        if needs_start {
            let scan_path = self.debug_shot.as_ref().unwrap().scan.clone();
            self.debug_shot.as_mut().unwrap().started = true;
            self.path_input = scan_path.display().to_string();
            self.start_scan(scan_path);
            return;
        }

        let saved = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = saved {
            let shot = self.debug_shot.as_ref().unwrap();
            let [w, h] = image.size;
            let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Err(err) = image::save_buffer(
                &shot.out,
                &bytes,
                w as u32,
                h as u32,
                image::ColorType::Rgba8,
            ) {
                eprintln!("failed to save screenshot: {err}");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let mode = self.debug_shot.as_ref().unwrap().mode;

        // Live mode: capture while the scan is still in flight, once enough
        // has streamed in that the map is visibly partial-but-populated.
        if mode == DebugShotMode::Live {
            if let Some(progress) = self.scan_controller.current_progress() {
                let files = progress.files_scanned.load(Ordering::Relaxed);
                let shot = self.debug_shot.as_mut().unwrap();
                if files > 500 && !shot.requested {
                    shot.requested = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                return;
            }
            // Scan finished before the threshold; fall through and capture
            // the final frame rather than hanging forever.
        }

        // Also wait for worker-side display-tree preparation — the
        // "Final"/"Drill" captures should show the finished tree, not a
        // still-live one before the atomic handoff.
        if !self.scan_controller.is_active() && self.root.is_some() {
            if mode == DebugShotMode::Drill && !self.debug_shot.as_ref().unwrap().drilled {
                // Focus the root's largest directory child, as a click would.
                if let Some(root) = &self.root {
                    let largest_dir = root
                        .children
                        .iter()
                        .filter(|c| c.is_dir)
                        .max_by_key(|c| c.size)
                        .map(|c| c.name.clone());
                    if let Some(name) = largest_dir {
                        self.focus = vec![name];
                    }
                }
                self.debug_shot.as_mut().unwrap().drilled = true;
                return;
            }

            let shot = self.debug_shot.as_mut().unwrap();
            shot.frames_after_done += 1;
            // A few settle frames so the final tree has actually rendered.
            if shot.frames_after_done >= 3 && !shot.requested {
                shot.requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
        }
    }
}

impl eframe::App for BytewhifferApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.debug_shot_tick(&ctx);
        // The elevated relaunch lands here with a root to resume; kick it off
        // once, on the first frame, now that the app is running.
        if let Some(root) = self.pending_scan.take() {
            self.path_input = root.display().to_string();
            self.start_scan(root);
        }
        self.drain_scan();
        if self.scan_controller.is_active() {
            // Keep repainting while the scan streams events or the worker
            // prepares the authoritative display tree.
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::Panel::top(egui::Id::new("toolbar")).show(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui);
            ui.add_space(2.0);
            if self.scan_active() {
                self.scan_hud(ui);
                ui.add_space(2.0);
            }
            self.breadcrumb(ui);
            ui.add_space(4.0);
        });

        egui::Panel::bottom(egui::Id::new("status_bar")).show(ui, |ui| {
            ui.add_space(2.0);
            self.status_bar(ui);
            ui.add_space(2.0);
        });

        if self.insights_open {
            egui::Panel::left(egui::Id::new("insights_drawer"))
                .resizable(true)
                .min_size(240.0)
                .max_size(360.0)
                .default_size(300.0)
                .show(ui, |ui| {
                    self.insights_panel(ui);
                });
        }

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| {
                self.treemap_panel(ui);
            });

        self.error_window(&ctx);
        self.turbo_warning_window(&ctx);
        self.turbo_unsupported_window(&ctx);
        self.confirm_pending_delete(&ctx);
    }
}

/// The absolute focus trail for a drawer entry: the current focus `base`
/// plus the entry's relative `trail`. A file can't be focused, so it resolves
/// to its parent directory (the view that shows the file), matching how
/// click-to-drill only ever focuses directories.
fn focus_for(base: &[String], trail: &[String], is_dir: bool) -> Vec<String> {
    let mut f = base.to_vec();
    let take = if is_dir {
        trail.len()
    } else {
        trail.len().saturating_sub(1)
    };
    f.extend(trail[..take].iter().cloned());
    f
}

/// Builds the action payload used by treemap and Insights entries. Keeping the
/// display name separate from the filesystem path makes the confirmation text
/// stable even for paths whose final component is not Unicode-friendly.
fn make_action_target(trail: Vec<String>, path: PathBuf, is_dir: bool) -> ActionTarget {
    let display_name = trail
        .last()
        .cloned()
        .or_else(|| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| path.display().to_string());
    ActionTarget {
        trail,
        path,
        is_dir,
        display_name,
    }
}

/// Adapts a treemap hit into the shared action payload. Hit trails are
/// relative to the currently focused node, while the action target trail is
/// relative to the scan root.
fn action_target_from_treemap_hit(base: &[String], hit: &HitRect) -> ActionTarget {
    let mut trail = base.to_vec();
    trail.extend(hit.trail.iter().cloned());
    make_action_target(trail, hit.fs_path.clone(), hit.is_dir)
}

/// Adapts an Insights cleanup candidate into the shared action payload.
/// Candidate trails are relative to the currently focused subtree, just like
/// treemap hit trails.
fn action_target_from_cleanup_candidate(
    base: &[String],
    entry: &insights::CleanupCandidate,
) -> ActionTarget {
    let mut trail = base.to_vec();
    trail.extend(entry.trail.iter().cloned());
    make_action_target(trail, entry.path.clone(), entry.is_dir)
}

/// A drawer section header.
fn insights_header(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().color(theme::TEXT));
    ui.add_space(2.0);
}

/// A neutral per-section empty state, shown instead of an empty gap when a
/// section has nothing to report for the focused subtree.
fn insights_empty(ui: &mut egui::Ui, text: &str) {
    ui.colored_label(theme::TEXT_SUBTLE, text);
}

/// A small color swatch for a legend/breakdown row, painted in the exact
/// color the treemap assigns that extension.
fn swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, color);
}

/// Row height reserved for an Insights-drawer bar row — tall enough to hold
/// the row's swatch/label/size content (or a leaderboard entry) without
/// clipping.
const INSIGHTS_BAR_ROW_H: f32 = 22.0;

/// Reserves one Insights-drawer row's rect (mirroring `swatch()`'s own
/// `ui.allocate_exact_size` pattern), paints a proportional-width fill bar
/// into it — `theme::INSIGHTS_BAR`, scaled to `fraction` of the row's full
/// width — then lays the row's actual content (`add_content`) on top via a
/// child `Ui` scoped to that same rect, so the bar sits behind the row's
/// widgets rather than replacing them.
fn insights_bar_row(ui: &mut egui::Ui, fraction: f64, add_content: impl FnOnce(&mut egui::Ui)) {
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), INSIGHTS_BAR_ROW_H),
        Sense::hover(),
    );
    let bar_width = rect.width() * fraction.clamp(0.0, 1.0) as f32;
    if bar_width > 0.0 {
        let bar_rect = Rect::from_min_size(rect.left_top(), Vec2::new(bar_width, rect.height()));
        ui.painter()
            .rect_filled(bar_rect, 2.0, theme::INSIGHTS_BAR.linear_multiply(0.35));
    }
    let mut content_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add_content(&mut content_ui);
}

/// Walks down through a run of consecutive directories that each have
/// exactly one child which is itself a directory, joining their names into a
/// chain and returning the first directory that actually branches (zero
/// children, more than one child, or whose only child is a file) — the
/// "effective" node whose frame and contents get drawn. A directory with
/// more than one child never advances past itself, so an ordinary branching
/// directory returns a single-name chain unchanged.
fn collapse_chain(start: &Node) -> (Vec<&str>, &Node) {
    let mut names = vec![start.name.as_str()];
    let mut node = start;
    while node.children.len() == 1 && node.children[0].is_dir {
        node = &node.children[0];
        names.push(node.name.as_str());
    }
    (names, node)
}

/// Whether a file-card block has room for a size label in its top-right
/// corner without clipping or overlapping the name label already painted in
/// the top-left. Distinct from the plain width/height check that gates the
/// name label itself: a right-aligned size string can't rely on a clip rect
/// the way the name label does, since clipping it would visually collide
/// with the name rather than invisibly truncate — so this measures the size
/// string's actual rendered width via the same galley-measurement pattern
/// the chrome toggle buttons already use (`chrome_button`).
fn size_label_fits(painter: &egui::Painter, block: Rect, size_str: &str) -> bool {
    if block.height() <= DIR_LABEL_H + 2.0 {
        return false;
    }
    let galley = painter.layout_no_wrap(
        size_str.to_owned(),
        FontId::proportional(LABEL_FONT_SIZE),
        theme::TEXT,
    );
    let needed = SIZE_LABEL_NAME_RESERVE + galley.size().x + LABEL_H_PAD * 2.0;
    block.width() > needed
}

/// Whether a directory tray's header has room for a size label alongside its
/// name (or collapsed-chain) label — measuring that label's actual rendered
/// width rather than reusing `size_label_fits`'s fixed reserve. A collapsed
/// chain (`collapse_chain`) can produce a joined name long enough to consume
/// most of the header on its own, so the tray gate has to account for that
/// specific label's width rather than assume a short single name.
fn tray_size_label_fits(
    painter: &egui::Painter,
    header_width: f32,
    label: &str,
    size_str: &str,
) -> bool {
    let font = FontId::proportional(LABEL_FONT_SIZE);
    let label_width = painter
        .layout_no_wrap(label.to_owned(), font.clone(), theme::TEXT)
        .size()
        .x;
    let size_width = painter
        .layout_no_wrap(size_str.to_owned(), font, theme::TEXT)
        .size()
        .x;
    let needed = LABEL_H_PAD + label_width + TRAY_LABEL_GAP + size_width + LABEL_H_PAD;
    header_width > needed
}

/// Recursively draws `node`'s children into `rect`, collecting hit-test rects
/// along the way. Children are laid out largest-first by the squarified
/// algorithm. Blocks big enough to read (≥ `MIN_CARD_SIDE`) render as raised
/// cards — soft drop shadow, top-lighter/bottom-darker gradient, rounded
/// corners; directories large enough for a title bar render instead as a
/// recessed tray (dark well + header strip) whose children float above it as
/// cards. Everything below the threshold falls back to today's flat fill with
/// no shadow/gradient/radius/gap, so dense clusters stay legible and cheap.
///
/// When `dense` is set (the focused subtree is large — see
/// `BytewhifferApp::refresh_density`), card-eligible blocks keep their rounded
/// silhouette but drop the blurred shadow and gradient mesh — the two costly
/// tessellation steps — so a viewport packed with hundreds of cards stays cheap
/// enough that hover/pointer tracking doesn't fall behind the cursor. Trays are
/// already cheap (flat fill + stroke + header) and render the same either way.
#[allow(clippy::too_many_arguments)]
fn draw_children(
    painter: &egui::Painter,
    node: &Node,
    rect: Rect,
    depth: usize,
    #[allow(clippy::too_many_arguments)] trail: &mut Vec<String>,
    hits: &mut Vec<HitRect>,
    dense: bool,
    gate: NestGate,
    layout_cache: &mut TreemapLayoutCache,
) {
    if node.children.is_empty() || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }

    let layout = layout_cache.layout_for(node, rect, depth, gate);

    for (k, &i) in layout.order.iter().enumerate() {
        let child = &node.children[i];
        let r = layout.rects[k];
        if r.w <= 0.0 || r.h <= 0.0 {
            continue;
        }
        let raw = Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h));
        // Card-eligible blocks earn a gap so neighbours' shadows show; flat
        // fallbacks keep today's tight 0.5px seam. Sub-pixel slivers skip the
        // shrink entirely so it can't invert to a negative size and vanish —
        // a hairline is a truer picture of the tree than a silent hole.
        let card_eligible = raw.width() >= MIN_CARD_SIDE && raw.height() >= MIN_CARD_SIDE;
        let shrink = if card_eligible {
            CARD_GAP
        } else if raw.width() > 1.0 && raw.height() > 1.0 {
            0.5
        } else {
            0.0
        };
        let block = raw.shrink(shrink);

        // A directory renders as a frame (header + bordered well) only when
        // it will actually nest children into that well; a header over an
        // empty bordered body — which happened whenever a dir cleared the
        // header-height bar but not the stricter nesting-area/side gate —
        // reads as a hole, not a directory. Below that bar it's just a plain
        // labeled card, like a file.
        // The render posture supplies the whole gate: at the detail end its
        // fields are today's `MAX_DEPTH`/`MIN_NEST_*` constants; toward the
        // abstract end the depth cap drops and the size thresholds rise, so
        // branches stop nesting sooner and small blocks collapse (see
        // `nest_gate`).
        let would_nest = depth < gate.max_depth
            && block.area() > gate.min_area
            && block.width() > gate.min_side
            && block.height() > gate.min_side + DIR_LABEL_H;
        let tray = child.is_dir && card_eligible && would_nest;

        if tray {
            // Consecutive single-child directories (e.g. a Steam library's
            // `SteamLibrary/steamapps/common`) collapse into one combined
            // header instead of stacking a full-width bar per level; the
            // frame is drawn around the first directory that actually
            // branches, using its name for the frame's identity color.
            let (chain, effective) = collapse_chain(child);
            let label = chain.join(" / ");
            draw_tray_shell(
                painter,
                block,
                &label,
                &effective.name,
                depth,
                effective.size,
            );

            let chain_len = chain.len();
            for name in chain {
                trail.push(name.to_string());
            }
            hits.push(HitRect {
                rect: block,
                trail: trail.clone(),
                fs_path: effective.path.clone(),
                is_dir: true,
                size: effective.size,
                collapsed: false,
            });

            // Children pack flush against the frame's border line — depth
            // advances once for the whole collapsed chain, not once per
            // absorbed level, so elevation tracks visual containers shown
            // rather than raw filesystem depth.
            let inset = theme::DIR_FRAME_BORDER_WIDTH;
            let inner = Rect::from_min_max(
                Pos2::new(block.left() + inset, block.top() + DIR_LABEL_H + inset),
                Pos2::new(block.right() - inset, block.bottom() - inset),
            );
            draw_children(
                painter,
                effective,
                inner,
                depth + 1,
                trail,
                hits,
                dense,
                gate,
                layout_cache,
            );

            for _ in 0..chain_len {
                trail.pop();
            }
        } else {
            let base =
                theme::depth_shift(theme::base_block_color(&child.name, child.is_dir), depth);
            if card_eligible && !dense {
                paint_card(painter, block, base);
            } else if card_eligible {
                // Dense tier: keep the rounded card silhouette but skip the
                // blurred shadow and the gradient mesh — the two expensive
                // tessellation steps — so a view packed with cards stays cheap.
                painter.rect_filled(block, theme::CARD_CORNER_RADIUS, base);
                painter.rect_stroke(
                    block,
                    theme::CARD_CORNER_RADIUS,
                    Stroke::new(1.0, theme::BLOCK_BORDER),
                    StrokeKind::Inside,
                );
            } else {
                // Flat fallback: identical to the pre-elevation rendering, except
                // a near-black 1px border on a block only a few pixels wide would
                // swallow the fill entirely — at that scale the border reads as
                // a solid dark hole rather than an outline, so skip it and let
                // the fill color carry the tile.
                painter.rect_filled(block, 0.0, base);
                if block.width() >= 4.0 && block.height() >= 4.0 {
                    painter.rect_stroke(
                        block,
                        0.0,
                        Stroke::new(1.0, theme::BLOCK_BORDER),
                        StrokeKind::Inside,
                    );
                }
            }

            trail.push(child.name.clone());
            hits.push(HitRect {
                rect: block,
                trail: trail.clone(),
                fs_path: child.path.clone(),
                is_dir: child.is_dir,
                size: child.size,
                // A directory that reached the flat branch didn't nest, so it
                // is rendered as one collapsed block — the preview's target.
                collapsed: child.is_dir,
            });

            // Corner label when there's room. Threshold is lower than a full
            // label's natural width on purpose: clipped text ("app-releas...")
            // still identifies the block, which beats an anonymous color patch.
            let label_fits = block.width() > 30.0 && block.height() > DIR_LABEL_H + 2.0;
            if label_fits {
                let label_color = theme::label_text_color(base);
                let label_painter = painter.with_clip_rect(block);
                label_painter.text(
                    block.left_top() + Vec2::new(6.0, 3.0),
                    Align2::LEFT_TOP,
                    &child.name,
                    FontId::proportional(11.0),
                    label_color,
                );

                let size_str = format_size(child.size);
                if size_label_fits(painter, block, &size_str) {
                    label_painter.text(
                        block.right_top() + Vec2::new(-6.0, 3.0),
                        Align2::RIGHT_TOP,
                        size_str,
                        FontId::proportional(11.0),
                        label_color,
                    );
                }
            }

            trail.pop();
        }
    }
}

/// Builds the hover-preview overlay's shapes: a squarified peek at `node`'s
/// contents laid out inside `rect`, mirroring `draw_children`'s sort +
/// `squarify` + color rules but emitting `egui::Shape`s into `out` instead of
/// painting, so the caller can cache and re-paint them without recomputing.
/// The preview is non-committal (never hit-tested, never touches focus), so it
/// skips labels, elevation, and hit-rect bookkeeping — just enough structure
/// to answer "what's in here". Recursion uses the detail-posture nesting gate
/// (`nest_scale` = 1.0) so the peek shows the same structure a drill-down would.
fn build_preview_shapes(node: &Node, rect: Rect, depth: usize, out: &mut Vec<egui::Shape>) {
    if node.children.is_empty() || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let mut order: Vec<usize> = (0..node.children.len()).collect();
    order.sort_by(|&a, &b| node.children[b].size.cmp(&node.children[a].size));
    let sizes: Vec<u64> = order.iter().map(|&i| node.children[i].size).collect();
    let layout = treemap::squarify(
        &sizes,
        treemap::Rect::new(rect.left(), rect.top(), rect.width(), rect.height()),
    );
    for (k, &i) in order.iter().enumerate() {
        let child = &node.children[i];
        let r = layout[k];
        if r.w <= 1.0 || r.h <= 1.0 {
            continue;
        }
        let block = Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h)).shrink(0.5);
        let base = theme::depth_shift(theme::base_block_color(&child.name, child.is_dir), depth);
        let radius = if block.width() >= MIN_CARD_SIDE && block.height() >= MIN_CARD_SIDE {
            theme::CARD_CORNER_RADIUS
        } else {
            0.0
        };
        out.push(egui::Shape::rect_filled(block, radius, base));
        if block.width() >= 4.0 && block.height() >= 4.0 {
            out.push(egui::Shape::rect_stroke(
                block,
                radius,
                Stroke::new(1.0, theme::BLOCK_BORDER),
                StrokeKind::Inside,
            ));
        }
        let nestable = child.is_dir
            && depth + 1 < MAX_DEPTH
            && block.area() > MIN_NEST_AREA
            && block.width() > MIN_NEST_SIDE
            && block.height() > MIN_NEST_SIDE + DIR_LABEL_H;
        if nestable {
            let inset = theme::DIR_FRAME_BORDER_WIDTH;
            let inner = Rect::from_min_max(
                Pos2::new(block.left() + inset, block.top() + DIR_LABEL_H + inset),
                Pos2::new(block.right() - inset, block.bottom() - inset),
            );
            build_preview_shapes(child, inner, depth + 1, out);
        }
    }
}

/// Draws a directory's frame: a bordered well tinted with a faint hash-of-
/// name hue, and a header strip carrying `label` (a single name, or a
/// collapsed chain's joined path). `color_name` is the effective (terminal)
/// node's own name — the frame's identity color always comes from the actual
/// container being drawn, not from any collapsed intermediate level. Children
/// (raised cards) are drawn afterward, on top, packed flush against the
/// border.
fn draw_tray_shell(
    painter: &egui::Painter,
    block: Rect,
    label: &str,
    color_name: &str,
    depth: usize,
    size: u64,
) {
    let border = theme::dir_frame_border_color(color_name, depth);
    let fill = theme::dir_frame_fill_color(border);
    painter.rect_filled(block, theme::TRAY_CORNER_RADIUS, fill);
    painter.rect_stroke(
        block,
        theme::TRAY_CORNER_RADIUS,
        Stroke::new(theme::DIR_FRAME_BORDER_WIDTH, border),
        StrokeKind::Inside,
    );

    let header = Rect::from_min_max(
        block.left_top(),
        Pos2::new(block.right(), block.top() + DIR_LABEL_H),
    );
    let header_color = theme::tray_header_color(color_name, depth);
    painter.rect_filled(header, theme::TRAY_CORNER_RADIUS, header_color);
    let label_color = theme::label_text_color(header_color);
    let label_painter = painter.with_clip_rect(header);
    label_painter.text(
        header.left_top() + Vec2::new(6.0, 2.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(11.0),
        label_color,
    );

    let size_str = format_size(size);
    if tray_size_label_fits(painter, header.width(), label, &size_str) {
        label_painter.text(
            header.right_top() + Vec2::new(-6.0, 2.0),
            Align2::RIGHT_TOP,
            size_str,
            FontId::proportional(11.0),
            label_color,
        );
    }
}

/// Builds a rounded rectangle filled with a vertical top→bottom colour
/// gradient. egui has no gradient-fill primitive, so this hand-rolls a
/// triangle fan over the rounded-rect perimeter (via epaint's own path
/// helper) with per-vertex colour interpolated by height — the renderer
/// interpolates between vertices, giving a smooth sheen with real rounded
/// corners. `top` is used at `rect.top()`, `bottom` at `rect.bottom()`.
fn gradient_mesh(rect: Rect, radius: f32, top: egui::Color32, bottom: egui::Color32) -> egui::Mesh {
    use egui::epaint::{tessellator::path, CornerRadiusF32};

    let mut perimeter: Vec<Pos2> = Vec::new();
    path::rounded_rectangle(&mut perimeter, rect, CornerRadiusF32::same(radius));

    let mut mesh = egui::Mesh::default();
    if perimeter.len() < 3 {
        return mesh;
    }
    let height = rect.height().max(1.0);
    let color_at = |y: f32| top.lerp_to_gamma(bottom, ((y - rect.top()) / height).clamp(0.0, 1.0));

    // Center vertex (index 0), then the perimeter, fan-triangulated. A
    // rounded rect is convex, so a center fan tiles it with no overlap.
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center.y));
    for p in &perimeter {
        mesh.colored_vertex(*p, color_at(p.y));
    }
    let n = perimeter.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    mesh
}

/// Draws one raised surface: `shadow` drop shadow, gradient fill, and a
/// hairline rounded outline for crispness. `base` is the (already
/// depth-shifted) fill colour. The shadow is a parameter so treemap cards and
/// chrome can each pass a shadow scaled to their own element size while sharing
/// the identical gradient/radius/outline treatment.
fn paint_elevated(
    painter: &egui::Painter,
    rect: Rect,
    base: egui::Color32,
    shadow: egui::epaint::Shadow,
) {
    painter.add(shadow.as_shape(rect, theme::CARD_CORNER_RADIUS));
    let (top, bottom) = theme::gradient_stops(base);
    painter.add(egui::Shape::mesh(gradient_mesh(
        rect,
        theme::CARD_CORNER_RADIUS,
        top,
        bottom,
    )));
    painter.rect_stroke(
        rect,
        theme::CARD_CORNER_RADIUS,
        Stroke::new(1.0, theme::BLOCK_BORDER),
        StrokeKind::Inside,
    );
}

/// Draws one raised treemap card, using the block-scale drop shadow.
fn paint_card(painter: &egui::Painter, rect: Rect, base: egui::Color32) {
    paint_elevated(painter, rect, base, theme::card_shadow());
}

/// Paints a raised surface for a chrome element, honouring the same size
/// floor as treemap blocks: elevated (shadow + gradient + rounded) at normal
/// sizes, flat below `MIN_CARD_SIDE`. Chrome is unlikely to hit the floor in
/// practice, but the rule is applied for consistency. Uses the tighter
/// `theme::chrome_shadow()` scaled to chrome's small element size, not the
/// block-scale card shadow, so the shadow reads as a subtle lift rather than a
/// doubled, offset rectangle at ~26–34px tall.
fn paint_surface(painter: &egui::Painter, rect: Rect, base: egui::Color32) {
    if rect.width() >= MIN_CARD_SIDE && rect.height() >= MIN_CARD_SIDE {
        paint_elevated(painter, rect, base, theme::chrome_shadow());
    } else {
        painter.rect_filled(rect, 0.0, base);
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, theme::BLOCK_BORDER),
            StrokeKind::Inside,
        );
    }
}

/// A toolbar button drawn with the treemap's elevation language: a raised
/// gradient/shadow card that leans to the accent colour on hover and presses
/// darker while held. Returns the click response.
fn chrome_button(ui: &mut egui::Ui, text: &str, enabled: bool) -> egui::Response {
    let font = FontId::proportional(13.0);
    let pad = Vec2::new(12.0, 6.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT);
    let size = galley.size() + pad * 2.0;
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let hot = enabled && response.hovered();
        let held = enabled && response.is_pointer_button_down_on();
        let (base, text_color) = if !enabled {
            (theme::CHROME_BASE.gamma_multiply(0.5), theme::TEXT_SUBTLE)
        } else if held {
            (
                theme::ACCENT.lerp_to_gamma(egui::Color32::BLACK, 0.2),
                theme::BG,
            )
        } else if hot {
            (theme::ACCENT, theme::BG)
        } else {
            (theme::CHROME_BASE, theme::TEXT)
        };
        paint_surface(ui.painter(), rect, base);
        let tg = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font, text_color);
        let pos = rect.center() - tg.size() / 2.0;
        ui.painter().galley(pos, tg, text_color);
    }
    response
}

/// The Turbo toggle, drawn in the same elevation language as `chrome_button`
/// but colored by [`TurboState`]: greyed when disabled, muted chrome when
/// promptable (leaning accent on hover), solid accent when active, and warning
/// red when an elevated process is on a non-NTFS drive. Only the non-disabled
/// states sense clicks.
fn turbo_toggle(ui: &mut egui::Ui, text: &str, state: TurboState) -> egui::Response {
    let font = FontId::proportional(13.0);
    let pad = Vec2::new(12.0, 6.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT);
    let size = galley.size() + pad * 2.0;
    let clickable = !matches!(state, TurboState::Disabled);
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let hot = clickable && response.hovered();
        let (base, text_color) = match state {
            TurboState::Disabled => (theme::CHROME_BASE.gamma_multiply(0.5), theme::TEXT_SUBTLE),
            TurboState::Promptable => {
                if hot {
                    (theme::ACCENT, theme::BG)
                } else {
                    (theme::CHROME_BASE, theme::TEXT)
                }
            }
            TurboState::Active => (theme::ACCENT, theme::BG),
            TurboState::WarnUnsupported => {
                let fill = if hot {
                    TURBO_WARN_RED.lerp_to_gamma(egui::Color32::WHITE, 0.12)
                } else {
                    TURBO_WARN_RED
                };
                (fill, theme::TEXT)
            }
        };
        paint_surface(ui.painter(), rect, base);
        let tg = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font, text_color);
        let pos = rect.center() - tg.size() / 2.0;
        ui.painter().galley(pos, tg, text_color);
    }
    response
}

/// A breadcrumb crumb drawn as a small elevated chip in the same language as
/// `chrome_button`. `active` (the current focus level) wears the accent, as
/// does a hovered crumb; other crumbs use the muted chrome base.
fn chrome_chip(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    let font = FontId::proportional(12.0);
    let pad = Vec2::new(8.0, 4.0);
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font.clone(), theme::TEXT);
    let size = galley.size() + pad * 2.0;
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if ui.is_rect_visible(rect) {
        let accent = active || response.hovered();
        let base = if accent {
            theme::ACCENT
        } else {
            theme::CHROME_BASE
        };
        let text_color = if accent {
            theme::BG
        } else {
            theme::TEXT_SUBTLE
        };
        paint_surface(ui.painter(), rect, base);
        let tg = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font, text_color);
        let pos = rect.center() - tg.size() / 2.0;
        ui.painter().galley(pos, tg, text_color);
    }
    response
}

/// Renders `text` in `color` with a monospace font, for the HUD's ticking
/// elapsed-time and byte/rate labels — a fixed-width font keeps digit-count
/// changes (`"9s"` → `"10s"`) from reflowing neighboring HUD labels every
/// tick, unlike the proportional font used elsewhere in the toolbar.
fn mono_label(ui: &mut egui::Ui, color: egui::Color32, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).monospace().color(color));
}

/// Runs the hidden `--debug-perf` tessellation spike: builds a synthetic
/// dense tree shaped like the motivating screenshot (a big DLL-heavy system
/// dir, an installers dir, a dense file mosaic, plus nested app dirs), lays
/// it out at a typical window size, then tessellates the flat-fill baseline
/// and the shadow+gradient elevation treatment many times, reporting triangle
/// counts and per-frame CPU time for each. Headless: no GUI, no display.
pub fn run_perf_bench() {
    println!("=== soft-elevation tessellation spike (1280x760) ===");
    // The motivating scene: a big DLL-heavy dir + installers + a dense mosaic.
    bench_scene("dense (motivating screenshot)", synth_dense_tree());
    // Adversarial worst case for the elevation cost: hundreds of similarly
    // sized blocks all above the card threshold, so almost nothing falls back
    // to flat and the shadow/gradient cost is paid on every block.
    bench_scene(
        "all-cards (400 equal mid-size files)",
        synth_all_cards_tree(),
    );
}

/// Lays out one scene, then tessellates the flat baseline and the elevation
/// treatment many times, reporting triangle counts and per-frame CPU time.
fn bench_scene(label: &str, tree: Entry) {
    use egui::epaint::{ClippedShape, Primitive, TessellationOptions, Tessellator};
    use std::time::Instant;

    let root = Node::from_entry(&tree);
    let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1280.0, 760.0));
    let mut blocks: Vec<BenchBlock> = Vec::new();
    collect_bench_blocks(
        &root,
        viewport.shrink(BLOCK_PAD),
        0,
        resolve_nest_gate(0.0),
        &mut blocks,
    );

    let cards = blocks
        .iter()
        .filter(|b| b.rect.width() >= MIN_CARD_SIDE && b.rect.height() >= MIN_CARD_SIDE)
        .count();
    let flat = blocks.len() - cards;

    let baseline = build_baseline_shapes(&blocks, viewport);
    let elevated = build_elevated_shapes(&blocks, viewport);

    let tessellate = |shapes: &[ClippedShape]| -> (usize, Vec<f64>) {
        let iters = 200;
        let mut tris = 0usize;
        let mut times = Vec::with_capacity(iters);
        for _ in 0..iters {
            let input = shapes.to_vec();
            let mut tess = Tessellator::new(1.0, TessellationOptions::default(), [1, 1], vec![]);
            let t0 = Instant::now();
            let prims = tess.tessellate_shapes(input);
            times.push(t0.elapsed().as_secs_f64() * 1000.0);
            tris = prims
                .iter()
                .map(|p| match &p.primitive {
                    Primitive::Mesh(m) => m.indices.len() / 3,
                    _ => 0,
                })
                .sum();
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (tris, times)
    };

    let stat = |times: &[f64]| (times[times.len() / 2], times[0], times[times.len() - 1]);

    let (base_tris, base_t) = tessellate(&baseline);
    let (elev_tris, elev_t) = tessellate(&elevated);
    let (bmed, bmin, bmax) = stat(&base_t);
    let (emed, emin, emax) = stat(&elev_t);

    println!("\n-- {label} --");
    println!(
        "layout: {} visible blocks ({cards} card-eligible, {flat} flat-fallback)",
        blocks.len()
    );
    println!(
        "baseline (flat fill + stroke):     {base_tris:>7} tris   {bmed:6.3} ms median  ({bmin:.3}..{bmax:.3})"
    );
    println!(
        "elevated (shadow + gradient card): {elev_tris:>7} tris   {emed:6.3} ms median  ({emin:.3}..{emax:.3})"
    );
    println!(
        "delta: {:.2}x triangles, {:.2}x median frame tessellation",
        elev_tris as f64 / base_tris.max(1) as f64,
        emed / bmed.max(f64::MIN_POSITIVE)
    );
}

/// One laid-out block for the perf spike.
struct BenchBlock {
    rect: Rect,
    is_dir: bool,
    depth: usize,
    nestable: bool,
}

/// Mirrors `draw_children`'s layout rules (sort, squarify, nest condition) to
/// collect the set of blocks that would be painted, without touching a
/// `Painter`. `gate` is the render posture's resolved `NestGate` (see
/// `resolve_nest_gate`) — the `--debug-perf` bench always passes the detail
/// gate (`resolve_nest_gate(0.0)`) since it measures today's default posture;
/// unit tests pass both detail and abstract gates to compare block counts.
fn collect_bench_blocks(
    node: &Node,
    rect: Rect,
    depth: usize,
    gate: NestGate,
    out: &mut Vec<BenchBlock>,
) {
    if node.children.is_empty() || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }
    let mut order: Vec<usize> = (0..node.children.len()).collect();
    order.sort_by(|&a, &b| node.children[b].size.cmp(&node.children[a].size));
    let sizes: Vec<u64> = order.iter().map(|&i| node.children[i].size).collect();
    let layout = treemap::squarify(
        &sizes,
        treemap::Rect::new(rect.left(), rect.top(), rect.width(), rect.height()),
    );
    for (k, &i) in order.iter().enumerate() {
        let child = &node.children[i];
        let r = layout[k];
        if r.w < 2.0 || r.h < 2.0 {
            continue;
        }
        let block = Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h)).shrink(0.5);
        let nestable = child.is_dir
            && depth < gate.max_depth
            && block.area() > gate.min_area
            && block.width() > gate.min_side
            && block.height() > gate.min_side + DIR_LABEL_H;
        out.push(BenchBlock {
            rect: block,
            is_dir: child.is_dir,
            depth,
            nestable,
        });
        if nestable {
            let inset = theme::DIR_FRAME_BORDER_WIDTH;
            let inner = Rect::from_min_max(
                block.left_top() + Vec2::new(inset, DIR_LABEL_H + inset),
                block.right_bottom() - Vec2::new(inset, inset),
            );
            collect_bench_blocks(child, inner, depth + 1, gate, out);
        }
    }
}

/// Today's flat rendering for every block: rect fill + hairline stroke.
fn build_baseline_shapes(blocks: &[BenchBlock], clip: Rect) -> Vec<egui::epaint::ClippedShape> {
    let mut out = Vec::new();
    for b in blocks {
        let color = theme::depth_shift(theme::base_block_color("f.dll", b.is_dir), b.depth);
        out.push(egui::epaint::ClippedShape {
            clip_rect: clip,
            shape: egui::Shape::rect_filled(b.rect, 2.0, color),
        });
        out.push(egui::epaint::ClippedShape {
            clip_rect: clip,
            shape: egui::Shape::rect_stroke(
                b.rect,
                2.0,
                Stroke::new(1.0, theme::BLOCK_BORDER),
                StrokeKind::Inside,
            ),
        });
    }
    out
}

/// The soft-elevation rendering, mirroring the planned `draw_children`: cards
/// get shadow + gradient, trays get a recessed body + header, sub-threshold
/// blocks fall back to flat.
fn build_elevated_shapes(blocks: &[BenchBlock], clip: Rect) -> Vec<egui::epaint::ClippedShape> {
    let mut out = Vec::new();
    let mut push = |shape: egui::Shape| {
        out.push(egui::epaint::ClippedShape {
            clip_rect: clip,
            shape,
        })
    };
    for b in blocks {
        let base = theme::depth_shift(theme::base_block_color("f.dll", b.is_dir), b.depth);
        let card = b.rect.width() >= MIN_CARD_SIDE && b.rect.height() >= MIN_CARD_SIDE;
        if !card {
            push(egui::Shape::rect_filled(b.rect, 0.0, base));
            push(egui::Shape::rect_stroke(
                b.rect,
                0.0,
                Stroke::new(1.0, theme::BLOCK_BORDER),
                StrokeKind::Inside,
            ));
        } else if b.is_dir && b.nestable {
            let border = theme::dir_frame_border_color("dir", b.depth);
            let fill = theme::dir_frame_fill_color(border);
            push(egui::Shape::rect_filled(
                b.rect,
                theme::TRAY_CORNER_RADIUS,
                fill,
            ));
            push(egui::Shape::rect_stroke(
                b.rect,
                theme::TRAY_CORNER_RADIUS,
                Stroke::new(theme::DIR_FRAME_BORDER_WIDTH, border),
                StrokeKind::Inside,
            ));
            let header = Rect::from_min_max(
                b.rect.left_top(),
                Pos2::new(b.rect.right(), b.rect.top() + DIR_LABEL_H),
            );
            push(egui::Shape::rect_filled(
                header,
                theme::TRAY_CORNER_RADIUS,
                theme::tray_header_color("dir", b.depth),
            ));
        } else {
            push(
                theme::card_shadow()
                    .as_shape(b.rect, theme::CARD_CORNER_RADIUS)
                    .into(),
            );
            let (top, bottom) = theme::gradient_stops(base);
            push(egui::Shape::mesh(gradient_mesh(
                b.rect,
                theme::CARD_CORNER_RADIUS,
                top,
                bottom,
            )));
            push(egui::Shape::rect_stroke(
                b.rect,
                theme::CARD_CORNER_RADIUS,
                Stroke::new(1.0, theme::BLOCK_BORDER),
                StrokeKind::Inside,
            ));
        }
    }
    out
}

/// A synthetic tree shaped like the dense motivating screenshot, for the perf
/// spike. Deterministic (no RNG): sizes vary by index. Spike-only.
fn synth_dense_tree() -> Entry {
    fn file(name: String, size: u64) -> Entry {
        Entry {
            name,
            path: PathBuf::from("bench"),
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }
    fn dir(name: impl Into<String>, children: Vec<Entry>) -> Entry {
        let size = children.iter().map(|c| c.size).sum();
        Entry {
            name: name.into(),
            path: PathBuf::from("bench"),
            size,
            is_dir: true,
            children,
        }
    }

    // A big system dir dominated by hundreds of small DLLs (the dense mosaic).
    let system32 = dir(
        "System32",
        (0..240)
            .map(|i| file(format!("mod{i}.dll"), 40_000 + (i as u64 % 32) * 90_000))
            .chain((0..60).map(|i| file(format!("drv{i}.sys"), 20_000 + (i as u64 % 16) * 30_000)))
            .collect(),
    );
    // A few large installers.
    let installers = dir(
        "Installers",
        (0..14)
            .map(|i| {
                file(
                    format!("setup{i}.exe"),
                    200_000_000 + (i as u64) * 90_000_000,
                )
            })
            .collect(),
    );
    // A dense ~30-file mosaic of similar mid-size files.
    let downloads = dir(
        "Downloads",
        (0..30)
            .map(|i| {
                file(
                    format!("clip{i}.mp4"),
                    6_000_000 + (i as u64 % 5) * 1_000_000,
                )
            })
            .chain(
                (0..8).map(|i| file(format!("iso{i}.iso"), 700_000_000 + (i as u64) * 30_000_000)),
            )
            .collect(),
    );
    // Nested app dirs (depth) with mixed small files.
    let program_files = dir(
        "Program Files",
        (0..6)
            .map(|a| {
                dir(
                    format!("App{a}"),
                    (0..3)
                        .map(|s| {
                            dir(
                                format!("sub{s}"),
                                (0..24)
                                    .map(|i| {
                                        file(
                                            format!("res{i}.bin"),
                                            80_000 + (i as u64 % 10) * 120_000,
                                        )
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect(),
    );

    let mut loose: Vec<Entry> = (0..8)
        .map(|i| {
            file(
                format!("archive{i}.zip"),
                1_200_000_000 + (i as u64) * 200_000_000,
            )
        })
        .collect();
    loose.extend([
        dir("Windows", vec![system32]),
        installers,
        downloads,
        program_files,
    ]);
    dir("C:\\", loose)
}

/// A single directory of ~400 near-equal mid-size files: squarify tiles them
/// into a grid of ~49px blocks, all above the card threshold. The worst case
/// for elevation cost (almost no flat fallback). Spike-only.
fn synth_all_cards_tree() -> Entry {
    let children: Vec<Entry> = (0..400)
        .map(|i| Entry {
            name: format!("file{i}.dat"),
            path: PathBuf::from("bench"),
            size: 1_000_000 + (i as u64 % 7) * 40_000,
            is_dir: false,
            children: Vec::new(),
        })
        .collect();
    let size = children.iter().map(|c| c.size).sum();
    Entry {
        name: "Mosaic".to_string(),
        path: PathBuf::from("bench"),
        size,
        is_dir: true,
        children,
    }
}

#[cfg(test)]
mod abstraction_tests {
    use super::*;

    #[test]
    fn detail_end_matches_todays_constants_exactly() {
        let gate = resolve_nest_gate(0.0);
        assert_eq!(gate.max_depth, MAX_DEPTH);
        assert_eq!(gate.min_side, MIN_NEST_SIDE);
        assert_eq!(gate.min_area, MIN_NEST_AREA);
    }

    #[test]
    fn abstract_end_drops_depth_to_the_floor_and_scales_size_up() {
        let gate = resolve_nest_gate(1.0);
        assert_eq!(
            gate.max_depth, 1,
            "full abstract must cap depth at its floor of 1"
        );
        assert_eq!(gate.min_side, MIN_NEST_SIDE * (1.0 + ABSTRACTION_SIDE_GAIN));
        assert_eq!(
            gate.min_area,
            MIN_NEST_AREA * (1.0 + ABSTRACTION_SIDE_GAIN) * (1.0 + ABSTRACTION_SIDE_GAIN)
        );
    }

    #[test]
    fn depth_and_size_thresholds_move_monotonically_toward_abstract() {
        let steps = [0.0, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 1.0];
        let gates: Vec<NestGate> = steps.iter().map(|&a| resolve_nest_gate(a)).collect();
        for pair in gates.windows(2) {
            assert!(
                pair[1].max_depth <= pair[0].max_depth,
                "depth cap must never rise as abstraction increases"
            );
            assert!(
                pair[1].min_side >= pair[0].min_side,
                "size threshold must never fall as abstraction increases"
            );
        }
    }

    #[test]
    fn out_of_range_abstraction_is_clamped() {
        assert_eq!(resolve_nest_gate(-1.0), resolve_nest_gate(0.0));
        assert_eq!(resolve_nest_gate(2.0), resolve_nest_gate(1.0));
    }

    /// Builds `chains` top-level directories, each a single-child chain
    /// `chainN/lvl0/lvl1/.../lvl{chain_len-1}/leaf.bin`, every leaf the same
    /// size. Single-child directories always get ~the full parent rect from
    /// `squarify` (nothing to split against), so the block stays large enough
    /// to clear the pixel-size gate for many levels regardless of viewport —
    /// isolating the depth cap as the only thing that can stop nesting, which
    /// is exactly what the tests below need to exercise.
    fn build_chain_tree(chains: usize, chain_len: usize) -> Node {
        let mut root = Node::new("root".to_string(), PathBuf::from("root"), 0, true);
        for c in 0..chains {
            let mut rel = PathBuf::new();
            rel.push(format!("chain{c}"));
            for lvl in 0..chain_len {
                rel.push(format!("lvl{lvl}"));
            }
            rel.push("leaf.bin");
            root.insert(&rel, 10_000_000, false);
        }
        root
    }

    /// Core block-count check for the abstraction mechanism (tasks.md 4.1):
    /// the same nested tree renders strictly fewer visible blocks, and
    /// strictly fewer directories expand into their children, under the
    /// abstract posture than under detail.
    #[test]
    fn abstract_posture_renders_fewer_blocks_than_detail_on_the_same_tree() {
        let root = build_chain_tree(2, 6);
        let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1280.0, 760.0));

        let mut detail_blocks = Vec::new();
        collect_bench_blocks(
            &root,
            viewport.shrink(BLOCK_PAD),
            0,
            resolve_nest_gate(0.0),
            &mut detail_blocks,
        );

        let mut abstract_blocks = Vec::new();
        collect_bench_blocks(
            &root,
            viewport.shrink(BLOCK_PAD),
            0,
            resolve_nest_gate(1.0),
            &mut abstract_blocks,
        );

        let nestable_count = |blocks: &[BenchBlock]| blocks.iter().filter(|b| b.nestable).count();

        assert!(
            abstract_blocks.len() < detail_blocks.len(),
            "abstract ({}) should render fewer total blocks than detail ({})",
            abstract_blocks.len(),
            detail_blocks.len()
        );
        assert!(
            nestable_count(&abstract_blocks) < nestable_count(&detail_blocks),
            "abstract should expand fewer directories into their children than detail"
        );
        // Detail recurses through Program Files/App/sub down to individual
        // res*.bin files, so it must reach the depth-4 file level; abstract's
        // depth-1 cap must not.
        assert!(detail_blocks.iter().any(|b| b.depth >= 3));
        assert!(abstract_blocks.iter().all(|b| b.depth <= 1));
    }

    #[test]
    fn abstract_posture_still_shows_at_least_the_top_level_blocks() {
        let root = build_chain_tree(2, 6);
        let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1280.0, 760.0));

        let mut abstract_blocks = Vec::new();
        collect_bench_blocks(
            &root,
            viewport.shrink(BLOCK_PAD),
            0,
            resolve_nest_gate(1.0),
            &mut abstract_blocks,
        );

        // The root's direct children (both chains) must still all be present
        // as blocks — abstraction hides *interior* structure, never the top
        // level itself.
        let top_level = abstract_blocks.iter().filter(|b| b.depth == 0).count();
        assert_eq!(top_level, root.children.len());
    }
}

#[cfg(test)]
mod layout_cache_tests {
    use super::*;

    fn node(name: &str, path: &str) -> Node {
        let mut node = Node::new(name.to_owned(), PathBuf::from(path), 0, true);
        node.insert(Path::new("first.bin"), 10, false);
        node.insert(Path::new("second.bin"), 5, false);
        node
    }

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(800.0, 500.0))
    }

    #[test]
    fn reuses_unchanged_layouts_and_prunes_unseen_entries() {
        let root = node("root", "root");
        let sibling = node("sibling", "sibling");
        let focus = Vec::new();
        let gate = resolve_nest_gate(0.0);
        let rect = viewport();
        let mut cache = TreemapLayoutCache::default();

        cache.begin_frame(&focus, rect, gate);
        let first = cache.layout_for(&root, rect, 0, gate);
        cache.layout_for(&sibling, rect, 0, gate);
        cache.finish_frame();
        assert_eq!(cache.stats(), (0, 2, 2));

        cache.begin_frame(&focus, rect, gate);
        let reused = cache.layout_for(&root, rect, 0, gate);
        cache.finish_frame();

        assert!(Rc::ptr_eq(&first, &reused));
        assert_eq!(cache.stats(), (1, 2, 1));
    }

    #[test]
    fn insertion_size_change_and_removal_invalidate_layout() {
        let mut root = node("root", "root");
        let focus = Vec::new();
        let gate = resolve_nest_gate(0.0);
        let rect = viewport();
        let mut cache = TreemapLayoutCache::default();

        for expected_misses in 1..=4 {
            cache.begin_frame(&focus, rect, gate);
            cache.layout_for(&root, rect, 0, gate);
            cache.finish_frame();
            assert_eq!(cache.stats(), (0, expected_misses, 1));

            match expected_misses {
                1 => root.insert(Path::new("new.bin"), 20, false),
                2 => root.insert(Path::new("first.bin"), 3, false),
                3 => assert!(root.remove(&["new.bin".to_owned()])),
                _ => {}
            }
        }
    }

    #[test]
    fn unrelated_branch_reuses_its_layout_after_a_sibling_mutates() {
        let mut changed = node("changed", "changed");
        let stable = node("stable", "stable");
        let focus = Vec::new();
        let gate = resolve_nest_gate(0.0);
        let rect = viewport();
        let mut cache = TreemapLayoutCache::default();

        cache.begin_frame(&focus, rect, gate);
        cache.layout_for(&changed, rect, 0, gate);
        let stable_first = cache.layout_for(&stable, rect, 0, gate);
        cache.finish_frame();

        changed.insert(Path::new("new.bin"), 20, false);
        cache.begin_frame(&focus, rect, gate);
        cache.layout_for(&changed, rect, 0, gate);
        let stable_reused = cache.layout_for(&stable, rect, 0, gate);
        cache.finish_frame();

        assert!(Rc::ptr_eq(&stable_first, &stable_reused));
        assert_eq!(cache.stats(), (1, 3, 2));
    }

    #[test]
    fn focus_viewport_and_abstraction_changes_invalidate_context() {
        let root = node("root", "root");
        let rect = viewport();
        let other_rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(801.0, 500.0));
        let detail = resolve_nest_gate(0.0);
        let abstracted = resolve_nest_gate(1.0);
        let mut cache = TreemapLayoutCache::default();

        let contexts = [
            (Vec::new(), rect, detail),
            (vec!["focused".to_owned()], rect, detail),
            (vec!["focused".to_owned()], other_rect, detail),
            (vec!["focused".to_owned()], other_rect, abstracted),
        ];
        for (focus, viewport, gate) in contexts {
            cache.begin_frame(&focus, viewport, gate);
            cache.layout_for(&root, viewport, 0, gate);
            cache.finish_frame();
        }

        assert_eq!(cache.stats(), (0, 4, 1));
    }
}

#[cfg(test)]
mod insights_cache_tests {
    use super::*;

    fn tree() -> Node {
        let mut root = Node::new("root".to_owned(), PathBuf::from("root"), 0, true);
        root.insert(Path::new("focused/old.bin"), 10, false);
        root.insert(Path::new("sibling/old.bin"), 20, false);
        root
    }

    fn prepared_app() -> BytewhifferApp {
        let mut app = BytewhifferApp::new();
        app.replace_root(Some(tree()));
        app
    }

    #[test]
    fn pointer_only_frames_reuse_insights() {
        let mut app = prepared_app();

        app.refresh_insights();
        app.refresh_insights();

        assert_eq!(app.insights_refreshes, 1);
    }

    #[test]
    fn sibling_mutation_reuses_focused_insights() {
        let mut app = prepared_app();
        app.focus = vec!["focused".to_owned()];
        app.refresh_insights();

        app.root
            .as_mut()
            .unwrap()
            .insert(Path::new("sibling/new.bin"), 30, false);
        app.tree_rev = app.tree_rev.wrapping_add(1);
        app.refresh_insights();

        assert_eq!(app.insights_refreshes, 1);
    }

    #[test]
    fn mutation_under_focus_invalidates_insights() {
        let mut app = prepared_app();
        app.focus = vec!["focused".to_owned()];
        app.refresh_insights();

        app.root
            .as_mut()
            .unwrap()
            .insert(Path::new("focused/new.bin"), 30, false);
        app.tree_rev = app.tree_rev.wrapping_add(1);
        app.refresh_insights();

        assert_eq!(app.insights_refreshes, 2);
    }

    #[test]
    fn authoritative_root_replacement_invalidates_matching_revisions() {
        let mut app = prepared_app();
        app.refresh_insights();
        let old_revision = app.root.as_ref().unwrap().structural_rev();

        let mut replacement = Node::new(
            "replacement".to_owned(),
            PathBuf::from("replacement"),
            0,
            true,
        );
        replacement.insert(Path::new("first.bin"), 10, false);
        replacement.insert(Path::new("second.bin"), 20, false);
        app.replace_root(Some(replacement));
        assert_eq!(app.root.as_ref().unwrap().structural_rev(), old_revision);
        app.refresh_insights();

        assert_eq!(app.insights_refreshes, 2);
    }
}

#[cfg(test)]
mod scan_lifecycle_ui_tests {
    use super::*;
    use crate::scanner::ScanOutcome;

    struct ImmediateEngine;

    impl ScanEngine for ImmediateEngine {
        fn name(&self) -> &'static str {
            "ui-test"
        }

        fn is_available(&self, _target: &Path) -> Availability {
            Availability::Available
        }

        fn scan(&self, target: &Path, _ctx: &crate::scanner::ScanContext) -> ScanOutcome {
            ScanOutcome::Success(Entry {
                name: "root".to_owned(),
                path: target.to_path_buf(),
                size: 0,
                is_dir: true,
                children: Vec::new(),
            })
        }
    }

    #[test]
    fn typed_target_overrides_historical_scan_path() {
        let mut app = BytewhifferApp::new();
        app.last_scanned_path = Some(PathBuf::from("historical"));
        app.requested_target = Some(PathBuf::from("old-request"));
        app.path_input = "typed-folder".to_owned();

        assert_eq!(
            app.resolve_requested_target(),
            Some(PathBuf::from("typed-folder"))
        );
    }

    #[test]
    fn rescan_candidate_uses_requested_target_when_input_is_empty() {
        let mut app = BytewhifferApp::new();
        app.last_scanned_path = Some(PathBuf::from("historical"));
        app.requested_target = Some(PathBuf::from("requested"));
        app.path_input.clear();

        assert_eq!(
            app.resolve_requested_target(),
            Some(PathBuf::from("requested"))
        );
    }

    /// After `start_scan` (picker/CLI target), `path_input` mirrors the
    /// target's display text untouched — a later Rescan/Turbo action must
    /// return the exact stored `PathBuf` rather than reparsing that display
    /// text, or a picker/CLI path would be silently re-lossified on every
    /// follow-up action.
    #[test]
    fn rescan_after_start_scan_reuses_the_exact_stored_target() {
        let mut app = BytewhifferApp::new();
        app.start_scan(PathBuf::from("picked-folder"));

        assert_eq!(
            app.resolve_requested_target(),
            Some(PathBuf::from("picked-folder"))
        );
    }

    /// Once the user edits the path field after a scan, the field no longer
    /// mirrors the stored target's display text, so the typed text (not the
    /// stale stored target) must win.
    #[test]
    fn editing_the_field_after_a_scan_overrides_the_stored_target() {
        let mut app = BytewhifferApp::new();
        app.start_scan(PathBuf::from("picked-folder"));
        app.path_input = "edited-folder".to_owned();

        assert_eq!(
            app.resolve_requested_target(),
            Some(PathBuf::from("edited-folder"))
        );
    }

    /// The regression this guards against only bites on paths that aren't
    /// exactly representable in valid Unicode: a lone UTF-16 surrogate,
    /// which `Path::display()` replaces with U+FFFD. Reparsing that display
    /// text would silently change the target; the stored `PathBuf` must be
    /// reused unchanged instead.
    #[test]
    #[cfg(windows)]
    fn rescan_after_start_scan_preserves_a_non_unicode_path() {
        use std::os::windows::ffi::OsStringExt;

        // A lone high surrogate (0xD800) is not valid UTF-16 on its own and
        // has no lossless UTF-8 representation.
        let wide: Vec<u16> = "C:\\".encode_utf16().chain([0xD800, 'x' as u16]).collect();
        let lossy_path = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        assert_ne!(lossy_path, PathBuf::from(lossy_path.display().to_string()));

        let mut app = BytewhifferApp::new();
        app.start_scan(lossy_path.clone());

        assert_eq!(app.resolve_requested_target(), Some(lossy_path));
    }

    #[test]
    fn delete_is_gated_during_scan() {
        let mut app = BytewhifferApp::new();
        app.scan_controller
            .start(PathBuf::from("scan"), Box::new(ImmediateEngine));
        assert!(!app.delete_available());
        app.scan_controller.cancel_current();

        let mut completed = false;
        for _ in 0..10_000 {
            if app.scan_controller.poll_completion().is_some() {
                completed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            !app.scan_controller.is_active(),
            "cancelled test scan should be fully retired before assembly gating is checked"
        );

        assert!(completed, "cancelled scan did not publish completion");
        assert!(app.delete_available());
    }
}

#[cfg(test)]
mod delete_action_tests {
    use super::*;

    fn file(name: &str, size: u64) -> Entry {
        Entry {
            name: name.to_owned(),
            path: PathBuf::from(name),
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }

    fn dir(name: &str, children: Vec<Entry>) -> Entry {
        let size = children
            .iter()
            .fold(0u64, |total, child| total.saturating_add(child.size));
        Entry {
            name: name.to_owned(),
            path: PathBuf::from(name),
            size,
            is_dir: true,
            children,
        }
    }

    fn target(trail: &[&str], is_dir: bool) -> ActionTarget {
        let trail = trail.iter().map(|name| (*name).to_owned()).collect();
        make_action_target(trail, PathBuf::from("scan").join("target"), is_dir)
    }

    #[test]
    fn remove_nested_leaf_propagates_size_and_repairs_index() {
        let entry = dir(
            "root",
            vec![
                dir("alpha", vec![file("a.bin", 7), file("b.bin", 11)]),
                file("sibling.bin", 13),
            ],
        );
        let mut root = Node::from_entry(&entry);

        assert!(root.remove(&["alpha".into(), "a.bin".into()]));
        assert_eq!(root.size, 24);
        assert_eq!(root.find(&["alpha".into()]).unwrap().size, 11);
        let alpha = root.find(&["alpha".into()]).unwrap();
        assert!(alpha.find(&["a.bin".into()]).is_none());
        assert_eq!(alpha.child_index.get("b.bin"), Some(&0));
    }

    #[test]
    fn remove_top_level_child_rebuilds_sibling_indexes() {
        let entry = dir("root", vec![file("first.bin", 5), file("second.bin", 9)]);
        let mut root = Node::from_entry(&entry);

        assert!(root.remove(&["first.bin".into()]));
        assert_eq!(root.size, 9);
        assert_eq!(root.child_index.get("second.bin"), Some(&0));
        assert!(root.find(&["first.bin".into()]).is_none());
    }

    #[test]
    fn missing_removal_does_not_change_tree() {
        let entry = dir("root", vec![dir("alpha", vec![file("a.bin", 7)])]);
        let mut root = Node::from_entry(&entry);
        let before_size = root.size;
        let before_children: Vec<String> = root
            .children
            .iter()
            .map(|child| child.name.clone())
            .collect();

        assert!(!root.remove(&["alpha".into(), "missing.bin".into()]));
        assert_eq!(root.size, before_size);
        assert_eq!(
            root.children
                .iter()
                .map(|child| child.name.clone())
                .collect::<Vec<_>>(),
            before_children
        );
        assert_eq!(root.find(&["alpha".into()]).unwrap().size, 7);
    }

    #[test]
    fn removal_uses_saturating_size_policy_for_large_values() {
        let entry = Entry {
            name: "root".to_owned(),
            path: PathBuf::from("root"),
            size: u64::MAX,
            is_dir: true,
            children: vec![Entry {
                name: "huge.bin".to_owned(),
                path: PathBuf::from("huge.bin"),
                size: u64::MAX,
                is_dir: false,
                children: Vec::new(),
            }],
        };
        let mut root = Node::from_entry(&entry);

        assert!(root.remove(&["huge.bin".into()]));
        assert_eq!(root.size, 0);
    }

    #[test]
    fn cancelled_delete_keeps_pending_target_and_tree_untouched() {
        let entry = dir("root", vec![file("keep.bin", 12)]);
        let mut app = BytewhifferApp::new();
        app.root = Some(Node::from_entry(&entry));
        app.pending_delete = Some(target(&["keep.bin"], false));

        app.cancel_pending_delete();

        assert!(app.pending_delete.is_none());
        assert_eq!(app.root.as_ref().unwrap().size, 12);
        assert!(app
            .root
            .as_ref()
            .unwrap()
            .find(&["keep.bin".into()])
            .is_some());
        assert_eq!(app.tree_rev, 0);
        assert!(app.error.is_none());
    }

    struct BlockingDeleteEngine;

    impl ScanEngine for BlockingDeleteEngine {
        fn name(&self) -> &'static str {
            "delete-test"
        }

        fn is_available(&self, _target: &Path) -> Availability {
            Availability::Available
        }

        fn scan(
            &self,
            _target: &Path,
            ctx: &crate::scanner::ScanContext,
        ) -> crate::scanner::ScanOutcome {
            while !ctx.is_cancelled() {
                std::thread::yield_now();
            }
            crate::scanner::ScanOutcome::Cancelled
        }
    }

    #[test]
    fn stale_pending_delete_is_cancelled_when_scan_starts() {
        let mut app = BytewhifferApp::new();
        app.pending_delete = Some(target(&["keep.bin"], false));
        app.scan_controller
            .start(PathBuf::from("scan"), Box::new(BlockingDeleteEngine));

        assert!(!app.delete_available());
        assert!(app.clear_pending_delete_if_unavailable());
        assert!(app.pending_delete.is_none());
    }

    #[test]
    fn failed_delete_preserves_tree_and_surfaces_error() {
        let entry = dir("root", vec![file("keep.bin", 12)]);
        let mut app = BytewhifferApp::new();
        app.root = Some(Node::from_entry(&entry));
        let target = target(&["keep.bin"], false);

        app.apply_delete_result(&target, Err("access denied".to_owned()));

        assert_eq!(app.root.as_ref().unwrap().size, 12);
        assert!(app
            .root
            .as_ref()
            .unwrap()
            .find(&["keep.bin".into()])
            .is_some());
        assert_eq!(app.tree_rev, 0);
        let expected_error = format!(
            "Could not send {} to the recycle bin: access denied",
            target.path.display()
        );
        assert_eq!(app.error.as_deref(), Some(expected_error.as_str()));
    }

    #[test]
    fn successful_delete_updates_tree_revision_and_repairs_focus() {
        let entry = dir(
            "root",
            vec![
                dir("gone", vec![file("inside.bin", 20)]),
                file("stay.bin", 3),
            ],
        );
        let mut app = BytewhifferApp::new();
        app.root = Some(Node::from_entry(&entry));
        app.focus = vec!["gone".to_owned(), "inside.bin".to_owned()];
        let target = target(&["gone"], true);

        app.apply_delete_result(&target, Ok(()));

        let root = app.root.as_ref().unwrap();
        assert_eq!(root.size, 3);
        assert!(root.find(&["gone".into()]).is_none());
        assert_eq!(app.focus, Vec::<String>::new());
        assert_eq!(app.tree_rev, 1);
        assert!(app.error.is_none());
    }

    #[test]
    fn treemap_and_insights_adapters_preserve_exact_target_shape() {
        let base = vec!["focused".to_owned()];
        let path = PathBuf::from("scan/folder/file.bin");
        let hit = HitRect {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::splat(10.0)),
            trail: vec!["folder".to_owned(), "file.bin".to_owned()],
            fs_path: path.clone(),
            is_dir: false,
            size: 42,
            collapsed: false,
        };
        let candidate = insights::CleanupCandidate {
            name: "file.bin".to_owned(),
            trail: vec!["folder".to_owned(), "file.bin".to_owned()],
            path: path.clone(),
            is_dir: false,
            size: 42,
            classification: insights::CleanupClassification {
                category: insights::CleanupCategory::Installer,
                reason: "test candidate",
                confidence: insights::CleanupConfidence::ContextDependent,
            },
        };

        let treemap_target = action_target_from_treemap_hit(&base, &hit);
        let insights_target = action_target_from_cleanup_candidate(&base, &candidate);
        let expected_trail = vec![
            "focused".to_owned(),
            "folder".to_owned(),
            "file.bin".to_owned(),
        ];

        for target in [&treemap_target, &insights_target] {
            assert_eq!(target.trail, expected_trail);
            assert_eq!(target.path, path);
            assert!(!target.is_dir);
            assert_eq!(target.display_name, "file.bin");
        }
        assert_eq!(treemap_target, insights_target);
    }
}

/// Opens the system file manager with `path` selected. On Windows this is
/// Explorer's `/select,` verb; elsewhere (dev environment) fall back to
/// opening the containing directory.
fn reveal_in_file_manager(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let parent = path.parent().unwrap_or(path);
        open::that_detached(parent).map_err(std::io::Error::other)
    }
}
