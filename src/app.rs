//! eframe::App implementation: UI state, panel layout, background-scan
//! orchestration, and navigation state (focus path / breadcrumb).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::insights;
use crate::scanner::{
    walker::WalkerEngine, Availability, Entry, ScanContext, ScanEngine, ScanError, ScanEvent,
};
use crate::theme;
use crate::treemap;
use crate::util::{elide_middle, format_duration, format_size};

/// Stop nesting once a block is this small; below it nothing inside would
/// be readable or clickable anyway.
const MIN_NEST_AREA: f32 = 1200.0;
const MIN_NEST_SIDE: f32 = 24.0;
/// Extra multiplier the render-posture slider adds to the nesting gate at its
/// abstract end: at full abstract a block must be `1.0 + ABSTRACTION_NEST_GAIN`
/// times larger per side to still nest, folding more directories into single
/// collapsed blocks. See `BytewhifferApp::nest_scale`.
const ABSTRACTION_NEST_GAIN: f32 = 2.0;
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

/// UI-side mirror of the scan tree. Built incrementally from `ScanEvent`s
/// while a scan runs (so the map fills in live), then swapped wholesale for
/// the engine's authoritative tree when the scan completes. The name→index
/// map makes per-event path insertion cheap even for huge directories.
struct Node {
    name: String,
    path: PathBuf,
    size: u64,
    is_dir: bool,
    children: Vec<Node>,
    child_index: HashMap<String, usize>,
}

impl Node {
    fn new(name: String, path: PathBuf, size: u64, is_dir: bool) -> Self {
        Self {
            name,
            path,
            size,
            is_dir,
            children: Vec::new(),
            child_index: HashMap::new(),
        }
    }

    /// Inserts a discovered entry by its path relative to this node,
    /// creating intermediate directories as needed and accumulating file
    /// sizes into every ancestor on the way down.
    fn insert(&mut self, rel: &Path, size: u64, is_dir: bool) {
        let mut components: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if components.is_empty() {
            return;
        }
        let leaf = components.pop().unwrap();

        self.size += size;
        let mut node = self;
        let mut path = node.path.clone();
        for comp in components {
            path.push(&comp);
            let idx = match node.child_index.get(&comp) {
                Some(&i) => i,
                None => {
                    let i = node.children.len();
                    node.children
                        .push(Node::new(comp.clone(), path.clone(), 0, true));
                    node.child_index.insert(comp.clone(), i);
                    i
                }
            };
            node = &mut node.children[idx];
            node.size += size;
        }

        path.push(&leaf);
        match node.child_index.get(&leaf) {
            Some(&i) => node.children[i].size += size,
            None => {
                let i = node.children.len();
                node.children.push(Node::new(leaf.clone(), path, size, is_dir));
                node.child_index.insert(leaf, i);
            }
        }
    }

    /// Converts the engine's final tree into the UI shape.
    fn from_entry(entry: &Entry) -> Self {
        let mut node = Node::new(
            entry.name.clone(),
            entry.path.clone(),
            entry.size,
            entry.is_dir,
        );
        for (i, child) in entry.children.iter().enumerate() {
            node.child_index.insert(child.name.clone(), i);
            node.children.push(Node::from_entry(child));
        }
        node
    }

    fn find(&self, names: &[String]) -> Option<&Node> {
        let mut node = self;
        for name in names {
            node = &node.children[*node.child_index.get(name)?];
        }
        Some(node)
    }

    /// Total entries in this node's subtree, excluding the node itself — every
    /// descendant file and directory, at any depth. A stable, cheap density
    /// proxy for the render-tier decision (see `BytewhifferApp::refresh_density`),
    /// walked once per (focus, tree_rev) change rather than every frame.
    fn descendant_count(&self) -> usize {
        self.children
            .iter()
            .map(|c| 1 + c.descendant_count())
            .sum()
    }

    /// Removes the node at `names`, subtracting its size from every
    /// ancestor. Returns false if the path no longer exists.
    fn remove(&mut self, names: &[String]) -> bool {
        let Some((leaf, ancestors)) = names.split_last() else {
            return false;
        };
        let mut node = self;
        let mut chain: Vec<*mut Node> = vec![node as *mut Node];
        for name in ancestors {
            let Some(&i) = node.child_index.get(name) else {
                return false;
            };
            node = &mut node.children[i];
            chain.push(node as *mut Node);
        }
        let Some(&i) = node.child_index.get(leaf) else {
            return false;
        };
        let removed_size = node.children[i].size;
        node.children.remove(i);
        node.child_index.clear();
        for (j, child) in node.children.iter().enumerate() {
            node.child_index.insert(child.name.clone(), j);
        }
        // SAFETY: the raw pointers were taken while walking down a single
        // &mut borrow chain and are only used here, one at a time, after
        // the child borrow above has ended.
        for ptr in chain {
            unsafe {
                (*ptr).size = (*ptr).size.saturating_sub(removed_size);
            }
        }
        true
    }
}

/// A scan running on a background thread, plus the channels to observe it.
struct ActiveScan {
    ctx: ScanContext,
    events: Receiver<ScanEvent>,
    handle: Option<JoinHandle<Result<Entry, ScanError>>>,
}

/// One rendered treemap block that can be hovered/clicked, with the trail
/// of names leading to it from the focus node.
struct HitRect {
    rect: Rect,
    trail: Vec<String>,
    fs_path: PathBuf,
    is_dir: bool,
    size: u64,
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

/// The Insights drawer's computed analytics for one (focus, tree revision).
/// Cached so the whole-subtree aggregations run once per change — not every
/// frame (see the change's design doc) — and cloned cheaply for rendering.
#[derive(Clone, Default)]
struct InsightsData {
    ext_totals: Vec<(String, u64)>,
    leaderboard: Vec<insights::LeaderboardEntry>,
    blizzard: Vec<insights::BlizzardEntry>,
    junk: Vec<insights::JunkEntry>,
}

#[derive(Default)]
pub struct BytewhifferApp {
    path_input: String,
    root: Option<Node>,
    scan: Option<ActiveScan>,
    /// Names from the root node down to the focused directory.
    focus: Vec<String>,
    /// Block the open context menu refers to (trail from root, fs path).
    context_target: Option<(Vec<String>, PathBuf, bool)>,
    hovered_path: Option<PathBuf>,
    hovered_size: Option<u64>,
    error: Option<String>,
    debug_shot: Option<DebugShot>,
    /// Root path of the most recently started scan, kept after the scan
    /// completes (or fails) so Rescan can re-run it without retyping.
    last_scanned_path: Option<PathBuf>,
    /// Name of the engine that produced, or is producing, the current scan.
    engine_name: Option<&'static str>,
    scan_started_at: Option<Instant>,
    /// Last (time, bytes) sample used to derive `scan_rate_bps`, refreshed
    /// roughly once a second so the rate doesn't jitter between repaints.
    rate_sample: Option<(Instant, u64)>,
    scan_rate_bps: f64,
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
    /// completion, deletion) so the drawer can tell its cache is stale.
    tree_rev: u64,
    /// Cached drawer analytics plus the (focus, tree_rev) they describe;
    /// recomputed only when that key changes.
    insights_cache: Option<InsightsData>,
    insights_key: Option<(Vec<String>, u64)>,
    /// Cached "is the focused subtree dense enough for the cheap render tier?"
    /// decision, plus the (focus, tree_rev) it describes — recomputed only when
    /// that key changes, exactly like `insights_cache`. Keeps the descendant
    /// count off the per-frame path so pointer/hover tracking stays responsive.
    render_dense: bool,
    density_key: Option<(Vec<String>, u64)>,
    /// Render posture: 0.0 = detail (today's exact nesting gates), rising
    /// toward 1.0 = abstract (fewer, larger blocks). Drives a multiplier on
    /// `MIN_NEST_AREA`/`MIN_NEST_SIDE` in `draw_children` so more directories
    /// stay collapsed. Manual only — the user drags the toolbar slider; there
    /// is no density-based auto-engage. Defaults to 0.0 via `derive(Default)`,
    /// which is the detail end, preserving prior behavior. See
    /// `BytewhifferApp::nest_scale`.
    abstraction: f32,
}

impl BytewhifferApp {
    pub fn with_debug_shot(shot: DebugShot) -> Self {
        Self {
            debug_shot: Some(shot),
            ..Self::default()
        }
    }
}

impl BytewhifferApp {
    fn start_scan(&mut self, target: PathBuf) {
        let engine = WalkerEngine;
        match engine.is_available(&target) {
            Availability::Available => {}
            other => {
                // Only one engine exists today, so a non-available report
                // is surfaced rather than falling back; the orchestration
                // shape is what the v2 MFT engine will slot into.
                self.error = Some(format!(
                    "The {} engine cannot scan this target: {:?}",
                    engine.name(),
                    other
                ));
                return;
            }
        }

        let engine_name = engine.name();
        let (tx, rx) = mpsc::channel();
        let thread_ctx = ScanContext::new().with_events(tx);
        // The UI keeps its own handles to the same cancel/progress state,
        // but not to the event sender — otherwise the channel would never
        // disconnect when the scan thread finishes.
        let ui_ctx = ScanContext {
            cancel: thread_ctx.cancel.clone(),
            progress: thread_ctx.progress.clone(),
            events: None,
        };

        let root_name = target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| target.to_string_lossy().into_owned());
        self.root = Some(Node::new(root_name, target.clone(), 0, true));
        self.focus.clear();
        self.hovered_path = None;
        self.hovered_size = None;
        self.last_scanned_path = Some(target.clone());
        self.engine_name = Some(engine_name);
        self.scan_started_at = Some(Instant::now());
        self.rate_sample = None;
        self.scan_rate_bps = 0.0;
        self.top_level_sizes.clear();
        self.biggest_top_level = None;
        self.tree_rev = self.tree_rev.wrapping_add(1);

        let handle = std::thread::spawn(move || engine.scan(&target, &thread_ctx));
        self.scan = Some(ActiveScan {
            ctx: ui_ctx,
            events: rx,
            handle: Some(handle),
        });
    }

    fn drain_scan(&mut self) {
        let Some(scan) = &mut self.scan else { return };

        let mut discovered_any = false;
        if let Some(root) = &mut self.root {
            let base = root.path.clone();
            for event in scan.events.try_iter() {
                discovered_any = true;
                let ScanEvent::Discovered { path, size, is_dir } = event;
                if let Ok(rel) = path.strip_prefix(&base) {
                    if let Some(first) = rel.components().next() {
                        let top_name = first.as_os_str().to_string_lossy().into_owned();
                        let entry = self.top_level_sizes.entry(top_name.clone()).or_insert(0);
                        *entry += size;
                        let total = *entry;
                        let is_new_max = self
                            .biggest_top_level
                            .as_ref()
                            .map_or(true, |(_, max)| total > *max);
                        if is_new_max {
                            self.biggest_top_level = Some((top_name, total));
                        }
                    }
                    root.insert(rel, size, is_dir);
                }
            }
        }
        // A live scan grows the tree; let the drawer recompute against it.
        if discovered_any {
            self.tree_rev = self.tree_rev.wrapping_add(1);
        }

        // Refresh the scan-rate sample roughly once a second so the number
        // doesn't jitter between the ~100ms repaints a streaming scan drives.
        let now = Instant::now();
        let bytes_now = scan.ctx.progress.bytes_scanned.load(Ordering::Relaxed);
        match self.rate_sample {
            None => self.rate_sample = Some((now, bytes_now)),
            Some((t, b)) => {
                let dt = now.duration_since(t).as_secs_f64();
                if dt >= 1.0 {
                    self.scan_rate_bps = bytes_now.saturating_sub(b) as f64 / dt;
                    self.rate_sample = Some((now, bytes_now));
                }
            }
        }

        // The trait contract guarantees engines mark progress complete
        // before returning, so this is the "no longer in flight" signal;
        // the join right after it can only block momentarily.
        let finished = scan.ctx.progress.is_complete();
        if finished {
            self.last_summary = Some(ScanSummary {
                files: scan.ctx.progress.files_scanned.load(Ordering::Relaxed),
                dirs: scan.ctx.progress.dirs_scanned.load(Ordering::Relaxed),
                bytes: scan.ctx.progress.bytes_scanned.load(Ordering::Relaxed),
                elapsed: self
                    .scan_started_at
                    .map(|t| t.elapsed())
                    .unwrap_or_default(),
            });
            if let Some(handle) = scan.handle.take() {
                match handle.join() {
                    Ok(Ok(entry)) => {
                        self.root = Some(Node::from_entry(&entry));
                        if let Some(root) = &self.root {
                            if root.find(&self.focus).is_none() {
                                self.focus.clear();
                            }
                        }
                    }
                    Ok(Err(err)) => {
                        self.error = Some(match err {
                            ScanError::Unavailable(a) => {
                                format!("Scan engine unavailable for this target: {a:?}")
                            }
                            ScanError::RootUnreadable(e) => {
                                format!("Cannot read that folder: {e}")
                            }
                        });
                        self.root = None;
                    }
                    Err(_) => {
                        self.error = Some("The scan thread panicked.".to_owned());
                        self.root = None;
                    }
                }
            }
            // The authoritative tree replaced the live one; recompute.
            self.tree_rev = self.tree_rev.wrapping_add(1);
            self.scan = None;
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
                self.start_scan(PathBuf::from(self.path_input.trim()));
            }

            if chrome_button(ui, "Rescan", self.last_scanned_path.is_some()).clicked() {
                if let Some(path) = self.last_scanned_path.clone() {
                    self.start_scan(path);
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

            if let Some(scan) = &self.scan {
                if chrome_button(ui, "Cancel", true).clicked() {
                    scan.ctx.cancel.store(true, Ordering::Relaxed);
                }
                ui.spinner();
            }
        });
    }

    /// In-flight scan HUD: an indeterminate/pulsing progress bar plus every
    /// *live* number the scan can report. Shown only while `self.scan` is
    /// `Some`; owns these figures exclusively so the bottom status bar can
    /// stay quiet about them until the scan completes.
    fn scan_hud(&mut self, ui: &mut egui::Ui) {
        let Some(scan) = &self.scan else { return };
        let files = scan.ctx.progress.files_scanned.load(Ordering::Relaxed);
        let dirs = scan.ctx.progress.dirs_scanned.load(Ordering::Relaxed);
        let bytes = scan.ctx.progress.bytes_scanned.load(Ordering::Relaxed);
        let elapsed = self
            .scan_started_at
            .map(|t| t.elapsed())
            .unwrap_or_default();
        let rate = self.scan_rate_bps;
        let biggest = self.biggest_top_level.clone();

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;

            // Motion only — no fill level tied to completion, since the
            // parallel walker has no way to know a scan's total size ahead
            // of time. `0.5` is an arbitrary constant, not a fraction of
            // anything real.
            ui.add(
                egui::ProgressBar::new(0.5)
                    .animate(true)
                    .desired_width(110.0)
                    .desired_height(6.0)
                    .fill(theme::ACCENT),
            );

            ui.colored_label(
                theme::TEXT_SUBTLE,
                format!("{files} files · {dirs} dirs · {}", format_size(bytes)),
            );
            ui.colored_label(theme::TEXT_SUBTLE, format!("{}/s", format_size(rate as u64)));
            ui.colored_label(theme::TEXT_SUBTLE, format_duration(elapsed));
            if let Some((name, size)) = biggest {
                ui.colored_label(
                    theme::TEXT_SUBTLE,
                    format!("Largest so far: {name} ({})", format_size(size)),
                );
            }
        });
    }

    /// Persistent bottom status bar: a hover readout on the left (mirrors
    /// the block tooltip but never disappears), and on the right a scan
    /// summary that survives past scan completion plus the engine name.
    /// Goes quiet about live counts while a scan is running, since the HUD
    /// above already owns those.
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
                if self.scan.is_some() {
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
        let nest_scale = self.nest_scale();

        let avail = ui.available_rect_before_wrap();
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
            let msg = if self.scan.is_some() {
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
        draw_children(
            &painter,
            focus_node,
            avail.shrink(BLOCK_PAD),
            0,
            &mut Vec::new(),
            &mut hits,
            dense,
            nest_scale,
        );

        // Deepest block under the pointer wins: children are pushed after
        // their parents, so the last containing rect is the innermost.
        let hover_pos = response.hover_pos();
        let hovered = hover_pos.and_then(|pos| hits.iter().rev().find(|h| h.rect.contains(pos)));

        self.hovered_path = hovered.map(|h| h.fs_path.clone());
        self.hovered_size = hovered.map(|h| h.size);
        if let Some(hit) = hovered {
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
                let mut trail = self.focus.clone();
                trail.extend(hit.trail.iter().cloned());
                self.context_target = Some((trail, hit.fs_path.clone(), hit.is_dir));
            }
        }

        response.context_menu(|ui| self.context_menu_contents(ui));
    }

    fn context_menu_contents(&mut self, ui: &mut egui::Ui) {
        let Some((trail, fs_path, _is_dir)) = self.context_target.clone() else {
            ui.close();
            return;
        };

        ui.label(
            egui::RichText::new(trail.last().map(String::as_str).unwrap_or("?"))
                .color(theme::TEXT_SUBTLE),
        );
        ui.separator();

        if ui.button("Open").clicked() {
            if let Err(err) = open::that_detached(&fs_path) {
                self.error = Some(format!("Could not open {}: {err}", fs_path.display()));
            }
            ui.close();
        }

        if ui.button("Reveal in Explorer").clicked() {
            if let Err(err) = reveal_in_file_manager(&fs_path) {
                self.error = Some(format!("Could not reveal {}: {err}", fs_path.display()));
            }
            ui.close();
        }

        ui.separator();

        if ui.button("🗑 Delete").clicked() {
            match trash::delete(&fs_path) {
                Ok(()) => {
                    if let Some(root) = &mut self.root {
                        root.remove(&trail);
                    }
                    self.tree_rev = self.tree_rev.wrapping_add(1);
                    // If the deleted directory was inside the focused path,
                    // fall back to its parent.
                    if self.focus.starts_with(&trail) {
                        self.focus.truncate(trail.len().saturating_sub(1));
                    }
                }
                Err(err) => {
                    self.error = Some(format!("Could not delete {}: {err}", fs_path.display()));
                }
            }
            ui.close();
        }
    }

    /// Recomputes whether the focused subtree is dense enough to warrant the
    /// cheap (flat-rounded) render tier, if the focus or tree changed since it
    /// was last computed; a no-op otherwise. Mirrors `refresh_insights`: the
    /// whole-subtree descendant count runs once per change, never per frame, so
    /// a frame driven purely by pointer movement never pays for it.
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

    /// Linear multiplier the render posture applies to the nesting gate's
    /// pixel thresholds (`MIN_NEST_SIDE`, and its square for the 2D
    /// `MIN_NEST_AREA`). At the detail end (`abstraction == 0.0`) this is
    /// exactly 1.0, so the gate matches today's constants; at the abstract
    /// end (`abstraction == 1.0`) it reaches `1.0 + ABSTRACTION_NEST_GAIN`,
    /// raising the bar a block must clear to nest so more directories render
    /// as one collapsed block.
    fn nest_scale(&self) -> f32 {
        1.0 + self.abstraction.clamp(0.0, 1.0) * ABSTRACTION_NEST_GAIN
    }

    /// Recomputes the drawer's analytics if the focus or the tree has
    /// changed since they were last computed; a no-op otherwise. Keeps the
    /// whole-subtree walks off the per-frame render path.
    fn refresh_insights(&mut self) {
        let key = (self.focus.clone(), self.tree_rev);
        if self.insights_cache.is_some() && self.insights_key.as_ref() == Some(&key) {
            return;
        }
        let data = {
            let Some(root) = &self.root else {
                self.insights_cache = None;
                self.insights_key = Some(key);
                return;
            };
            // Describe whatever the treemap is currently showing — the same
            // node `treemap_panel` resolves via `root.find(&self.focus)`.
            let focus_node = root.find(&self.focus).unwrap_or(root);
            let view = to_insight(focus_node);
            InsightsData {
                ext_totals: view.extension_totals(),
                leaderboard: view.leaderboard(LEADERBOARD_N),
                blizzard: view.blizzard_flags(),
                junk: view.junk_suggestions(),
            }
        };
        self.insights_cache = Some(data);
        self.insights_key = Some(key);
    }

    /// Renders the Insights drawer: an extension legend + size breakdown, a
    /// biggest-items leaderboard, a small-file-blizzard flag list, and
    /// known-junk suggestions — all describing the focused subtree. Clicking
    /// a leaderboard/blizzard entry navigates the treemap; right-clicking a
    /// junk entry opens the same Delete/Open/Reveal menu as a treemap block.
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
                        ui.horizontal(|ui| {
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
                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let resp = ui
                            .selectable_label(
                                false,
                                format!("{icon} {}  ·  {}", entry.name, format_size(entry.size)),
                            )
                            .on_hover_text(entry.path.display().to_string());
                        if resp.clicked() {
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

                // --- Known-junk suggestions. Advisory only: right-clicking
                // opens the same context menu a treemap block does; nothing
                // is deleted merely by being listed here. ---
                insights_header(ui, "Junk suggestions");
                if data.junk.is_empty() {
                    insights_empty(ui, "No known-junk matches.");
                } else {
                    ui.colored_label(theme::TEXT_SUBTLE, "Right-click for Open / Reveal / Delete.");
                    for entry in &data.junk {
                        let icon = if entry.is_dir { "📁" } else { "📄" };
                        let resp = ui.selectable_label(
                            false,
                            format!(
                                "{icon} {}  ·  {} · {}",
                                entry.name,
                                entry.category,
                                format_size(entry.size)
                            ),
                        );
                        if resp.secondary_clicked() {
                            let mut trail = base.clone();
                            trail.extend(entry.trail.iter().cloned());
                            self.context_target =
                                Some((trail, entry.path.clone(), entry.is_dir));
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
            if let Some(scan) = &self.scan {
                let files = scan.ctx.progress.files_scanned.load(Ordering::Relaxed);
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

        if self.scan.is_none() && self.root.is_some() {
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                ));
            }
        }
    }
}

impl eframe::App for BytewhifferApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.debug_shot_tick(&ctx);
        self.drain_scan();
        if self.scan.is_some() {
            // Keep repainting while the scan streams events so the map
            // visibly fills in without waiting for input.
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        egui::Panel::top(egui::Id::new("toolbar")).show(ui, |ui| {
            ui.add_space(4.0);
            self.toolbar(ui);
            ui.add_space(2.0);
            if self.scan.is_some() {
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
    }
}

/// Borrows a live-UI `Node` subtree into the egui-free insight view the
/// `insights` module aggregates over. Cheap: a shallow walk that borrows
/// names/paths rather than copying them.
fn to_insight(node: &Node) -> insights::InsightNode<'_> {
    insights::InsightNode {
        name: &node.name,
        path: &node.path,
        size: node.size,
        is_dir: node.is_dir,
        children: node.children.iter().map(to_insight).collect(),
    }
}

/// The absolute focus trail for a drawer entry: the current focus `base`
/// plus the entry's relative `trail`. A file can't be focused, so it resolves
/// to its parent directory (the view that shows the file), matching how
/// click-to-drill only ever focuses directories.
fn focus_for(base: &[String], trail: &[String], is_dir: bool) -> Vec<String> {
    let mut f = base.to_vec();
    let take = if is_dir { trail.len() } else { trail.len().saturating_sub(1) };
    f.extend(trail[..take].iter().cloned());
    f
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
fn draw_children(
    painter: &egui::Painter,
    node: &Node,
    rect: Rect,
    depth: usize,
    trail: &mut Vec<String>,
    hits: &mut Vec<HitRect>,
    dense: bool,
    nest_scale: f32,
) {
    if node.children.is_empty() || rect.width() < 1.0 || rect.height() < 1.0 {
        return;
    }

    // The live tree arrives unsorted; sort per visible node per frame.
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
        // The render posture scales the pixel-size gate: `nest_scale` is 1.0
        // at the detail end (matching today's constants) and rises toward the
        // abstract end so a block must be proportionally larger to still nest.
        // Area is 2D, so it takes the square of the linear side multiplier.
        let would_nest = depth < MAX_DEPTH
            && block.area() > MIN_NEST_AREA * nest_scale * nest_scale
            && block.width() > MIN_NEST_SIDE * nest_scale
            && block.height() > MIN_NEST_SIDE * nest_scale + DIR_LABEL_H;
        let tray = child.is_dir && card_eligible && would_nest;

        if tray {
            // Consecutive single-child directories (e.g. a Steam library's
            // `SteamLibrary/steamapps/common`) collapse into one combined
            // header instead of stacking a full-width bar per level; the
            // frame is drawn around the first directory that actually
            // branches, using its name for the frame's identity color.
            let (chain, effective) = collapse_chain(child);
            let label = chain.join(" / ");
            draw_tray_shell(painter, block, &label, &effective.name, depth);

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
            draw_children(painter, effective, inner, depth + 1, trail, hits, dense, nest_scale);

            for _ in 0..chain_len {
                trail.pop();
            }
        } else {
            let base = theme::depth_shift(theme::base_block_color(&child.name, child.is_dir), depth);
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
            });

            // Corner label when there's room. Threshold is lower than a full
            // label's natural width on purpose: clipped text ("app-releas...")
            // still identifies the block, which beats an anonymous color patch.
            let label_fits = block.width() > 30.0 && block.height() > DIR_LABEL_H + 2.0;
            if label_fits {
                let label_painter = painter.with_clip_rect(block);
                label_painter.text(
                    block.left_top() + Vec2::new(6.0, 3.0),
                    Align2::LEFT_TOP,
                    &child.name,
                    FontId::proportional(11.0),
                    theme::TEXT,
                );
            }

            trail.pop();
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
fn draw_tray_shell(painter: &egui::Painter, block: Rect, label: &str, color_name: &str, depth: usize) {
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
    painter.rect_filled(
        header,
        theme::TRAY_CORNER_RADIUS,
        theme::tray_header_color(color_name, depth),
    );
    let label_painter = painter.with_clip_rect(header);
    label_painter.text(
        header.left_top() + Vec2::new(6.0, 2.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(11.0),
        theme::TEXT,
    );
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
fn paint_elevated(painter: &egui::Painter, rect: Rect, base: egui::Color32, shadow: egui::epaint::Shadow) {
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
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let hot = enabled && response.hovered();
        let held = enabled && response.is_pointer_button_down_on();
        let (base, text_color) = if !enabled {
            (theme::CHROME_BASE.gamma_multiply(0.5), theme::TEXT_SUBTLE)
        } else if held {
            (theme::ACCENT.lerp_to_gamma(egui::Color32::BLACK, 0.2), theme::BG)
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
        let text_color = if accent { theme::BG } else { theme::TEXT_SUBTLE };
        paint_surface(ui.painter(), rect, base);
        let tg = ui
            .painter()
            .layout_no_wrap(text.to_owned(), font, text_color);
        let pos = rect.center() - tg.size() / 2.0;
        ui.painter().galley(pos, tg, text_color);
    }
    response
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
    bench_scene("all-cards (400 equal mid-size files)", synth_all_cards_tree());
}

/// Lays out one scene, then tessellates the flat baseline and the elevation
/// treatment many times, reporting triangle counts and per-frame CPU time.
fn bench_scene(label: &str, tree: Entry) {
    use egui::epaint::{ClippedShape, Primitive, TessellationOptions, Tessellator};
    use std::time::Instant;

    let root = Node::from_entry(&tree);
    let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1280.0, 760.0));
    let mut blocks: Vec<BenchBlock> = Vec::new();
    collect_bench_blocks(&root, viewport.shrink(BLOCK_PAD), 0, &mut blocks);

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
/// `Painter`. Spike-only.
fn collect_bench_blocks(node: &Node, rect: Rect, depth: usize, out: &mut Vec<BenchBlock>) {
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
            && depth < MAX_DEPTH
            && block.area() > MIN_NEST_AREA
            && block.width() > MIN_NEST_SIDE
            && block.height() > MIN_NEST_SIDE + DIR_LABEL_H;
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
            collect_bench_blocks(child, inner, depth + 1, out);
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
            push(egui::Shape::rect_filled(b.rect, theme::TRAY_CORNER_RADIUS, fill));
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
            push(theme::card_shadow().as_shape(b.rect, theme::CARD_CORNER_RADIUS).into());
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
            .map(|i| file(format!("setup{i}.exe"), 200_000_000 + (i as u64) * 90_000_000))
            .collect(),
    );
    // A dense ~30-file mosaic of similar mid-size files.
    let downloads = dir(
        "Downloads",
        (0..30)
            .map(|i| file(format!("clip{i}.mp4"), 6_000_000 + (i as u64 % 5) * 1_000_000))
            .chain((0..8).map(|i| file(format!("iso{i}.iso"), 700_000_000 + (i as u64) * 30_000_000)))
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
        .map(|i| file(format!("archive{i}.zip"), 1_200_000_000 + (i as u64) * 200_000_000))
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
