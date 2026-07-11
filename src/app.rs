//! eframe::App implementation: UI state, panel layout, background-scan
//! orchestration, and navigation state (focus path / breadcrumb).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

use eframe::egui::{self, Align2, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::scanner::{
    walker::WalkerEngine, Availability, Entry, ScanContext, ScanEngine, ScanError, ScanEvent,
};
use crate::theme;
use crate::treemap;
use crate::util::format_size;

/// Stop nesting once a block is this small; below it nothing inside would
/// be readable or clickable anyway.
const MIN_NEST_AREA: f32 = 1200.0;
const MIN_NEST_SIDE: f32 = 24.0;
/// Hard depth cap as a backstop against pathological trees.
const MAX_DEPTH: usize = 10;
/// Vertical space reserved for a directory's name strip when nesting into it.
const DIR_LABEL_H: f32 = 16.0;
const BLOCK_PAD: f32 = 2.0;

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
    error: Option<String>,
    debug_shot: Option<DebugShot>,
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

        let handle = std::thread::spawn(move || engine.scan(&target, &thread_ctx));
        self.scan = Some(ActiveScan {
            ctx: ui_ctx,
            events: rx,
            handle: Some(handle),
        });
    }

    fn drain_scan(&mut self) {
        let Some(scan) = &mut self.scan else { return };

        if let Some(root) = &mut self.root {
            let base = root.path.clone();
            for event in scan.events.try_iter() {
                let ScanEvent::Discovered { path, size, is_dir } = event;
                if let Ok(rel) = path.strip_prefix(&base) {
                    root.insert(rel, size, is_dir);
                }
            }
        }

        // The trait contract guarantees engines mark progress complete
        // before returning, so this is the "no longer in flight" signal;
        // the join right after it can only block momentarily.
        let finished = scan.ctx.progress.is_complete();
        if finished {
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
            self.scan = None;
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("📁 Pick folder…").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.path_input = folder.to_string_lossy().into_owned();
                    self.start_scan(folder);
                }
            }

            let edit = egui::TextEdit::singleline(&mut self.path_input)
                .hint_text("…or type a path")
                .desired_width(320.0);
            let path_response = ui.add(edit);
            let submitted =
                path_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Scan").clicked() || submitted) && !self.path_input.trim().is_empty() {
                self.start_scan(PathBuf::from(self.path_input.trim()));
            }

            if let Some(scan) = &self.scan {
                if ui.button("Cancel").clicked() {
                    scan.ctx.cancel.store(true, Ordering::Relaxed);
                }
                ui.spinner();
                let files = scan.ctx.progress.files_scanned.load(Ordering::Relaxed);
                let bytes = scan.ctx.progress.bytes_scanned.load(Ordering::Relaxed);
                ui.colored_label(
                    theme::TEXT_SUBTLE,
                    format!("{files} files · {} scanned", format_size(bytes)),
                );
            } else if let Some(root) = &self.root {
                ui.colored_label(
                    theme::TEXT_SUBTLE,
                    format!("{} total", format_size(root.size)),
                );
            }
        });
    }

    fn breadcrumb(&mut self, ui: &mut egui::Ui) {
        let Some(root) = &self.root else { return };
        let mut new_focus: Option<Vec<String>> = None;

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;

            let back = ui
                .add_enabled(!self.focus.is_empty(), egui::Button::new("⬅"))
                .on_hover_text("Back to parent");
            if back.clicked() {
                let mut f = self.focus.clone();
                f.pop();
                new_focus = Some(f);
            }

            // Root crumb, then one crumb per focused level. The *current*
            // level is the one place (besides hover/selection) that wears
            // the accent color.
            let at_root = self.focus.is_empty();
            let root_label = egui::RichText::new(&root.name).color(if at_root {
                theme::ACCENT
            } else {
                theme::TEXT_SUBTLE
            });
            if ui.link(root_label).clicked() {
                new_focus = Some(Vec::new());
            }

            for (i, name) in self.focus.iter().enumerate() {
                ui.colored_label(theme::TEXT_SUBTLE, "›");
                let is_current = i == self.focus.len() - 1;
                let label = egui::RichText::new(name).color(if is_current {
                    theme::ACCENT
                } else {
                    theme::TEXT_SUBTLE
                });
                if ui.link(label).clicked() {
                    new_focus = Some(self.focus[..=i].to_vec());
                }
            }
        });

        if let Some(f) = new_focus {
            self.focus = f;
            self.hovered_path = None;
        }
    }

    fn treemap_panel(&mut self, ui: &mut egui::Ui) {
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
        );

        // Deepest block under the pointer wins: children are pushed after
        // their parents, so the last containing rect is the innermost.
        let hover_pos = response.hover_pos();
        let hovered = hover_pos.and_then(|pos| hits.iter().rev().find(|h| h.rect.contains(pos)));

        self.hovered_path = hovered.map(|h| h.fs_path.clone());
        if let Some(hit) = hovered {
            painter.rect_stroke(
                hit.rect,
                2.0,
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
                ui.label(egui::RichText::new(hit.trail.join("/")).strong());
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
            self.breadcrumb(ui);
            ui.add_space(4.0);
        });

        egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(theme::BG))
            .show(ui, |ui| {
                self.treemap_panel(ui);
            });

        self.error_window(&ctx);
    }
}

/// Recursively draws `node`'s children into `rect`, collecting hit-test
/// rects along the way. Children are laid out largest-first by the
/// squarified algorithm; directories big enough to matter nest their own
/// contents inside, one lightness step up per level.
fn draw_children(
    painter: &egui::Painter,
    node: &Node,
    rect: Rect,
    depth: usize,
    trail: &mut Vec<String>,
    hits: &mut Vec<HitRect>,
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
        if r.w < 2.0 || r.h < 2.0 {
            continue;
        }
        let block = Rect::from_min_size(Pos2::new(r.x, r.y), Vec2::new(r.w, r.h)).shrink(0.5);

        let color = theme::depth_shift(theme::base_block_color(&child.name, child.is_dir), depth);
        painter.rect_filled(block, 2.0, color);
        painter.rect_stroke(
            block,
            2.0,
            Stroke::new(1.0, theme::BLOCK_BORDER),
            StrokeKind::Inside,
        );

        trail.push(child.name.clone());
        hits.push(HitRect {
            rect: block,
            trail: trail.clone(),
            fs_path: child.path.clone(),
            is_dir: child.is_dir,
            size: child.size,
        });

        let label_fits = block.width() > 48.0 && block.height() > DIR_LABEL_H + 2.0;
        if label_fits {
            let label_painter = painter.with_clip_rect(block);
            label_painter.text(
                block.left_top() + Vec2::new(4.0, 2.0),
                Align2::LEFT_TOP,
                &child.name,
                FontId::proportional(11.0),
                theme::TEXT,
            );
        }

        let nestable = child.is_dir
            && depth < MAX_DEPTH
            && block.area() > MIN_NEST_AREA
            && block.width() > MIN_NEST_SIDE
            && block.height() > MIN_NEST_SIDE + DIR_LABEL_H;
        if nestable {
            let inner = Rect::from_min_max(
                block.left_top() + Vec2::new(BLOCK_PAD, if label_fits { DIR_LABEL_H } else { BLOCK_PAD }),
                block.right_bottom() - Vec2::new(BLOCK_PAD, BLOCK_PAD),
            );
            draw_children(painter, child, inner, depth + 1, trail, hits);
        }

        trail.pop();
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
