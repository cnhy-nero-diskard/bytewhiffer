//! The [`Entry`] tree type, the [`ScanEngine`] trait every scanning backend
//! implements, and the shared progress/cancellation/event types engines use
//! to report on an in-flight scan. This module has no GUI dependencies so it
//! can be exercised with plain `cargo test`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;

pub mod mft;
pub mod walker;

/// Process-local identity for one scan generation.
pub type ScanId = u64;

/// Maximum number of provisional discovery events retained for the UI.
///
/// Discovery is deliberately best-effort: scanners must never wait for the UI
/// to make room. The authoritative `Entry` tree is returned separately.
pub const LIVE_EVENT_CAPACITY: usize = 1024;

/// Creates the bounded channel used for provisional live discovery.
pub fn live_event_channel() -> (SyncSender<ScanEvent>, Receiver<ScanEvent>) {
    std::sync::mpsc::sync_channel(LIVE_EVENT_CAPACITY)
}

/// A single file or directory discovered during a scan.
///
/// Directories store the *total* size of everything beneath them in `size`,
/// which is what makes the treemap meaningful. `children` is empty for
/// files, and is sorted largest-first for directories once a scan finishes
/// (see [`Entry::sort_children_recursive`]).
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Entry>,
}

impl Entry {
    /// Sorts this entry's children largest-first, recursively. Doing this
    /// once after a scan (rather than re-sorting every frame) keeps the UI
    /// code simple: it can assume children already arrive in the order the
    /// treemap should lay them out.
    pub fn sort_children_recursive(&mut self) {
        self.children
            .sort_unstable_by_key(|b| std::cmp::Reverse(b.size));
        for child in &mut self.children {
            child.sort_children_recursive();
        }
    }

    /// Number of direct children.
    #[allow(dead_code)] // exercised by tests; kept as tree-inspection API
    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// Whether a given engine can scan a given target. Anything other than
/// [`Availability::Available`] tells the orchestration layer to fall back to
/// another engine rather than start a scan that cannot succeed.
// Only `Available` is constructed while the walker is the sole engine; the
// other variants are the contract the v2 MFT engine reports through.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// The engine can scan this target right now.
    Available,
    /// The engine could scan this target, but only with elevated privileges
    /// (e.g. raw NTFS volume access needs admin rights).
    RequiresElevation,
    /// The target's filesystem is one this engine cannot read (e.g. an MFT
    /// reader pointed at a FAT32 or network volume).
    UnsupportedFilesystem,
    /// The engine does not apply to this kind of target at all.
    NotApplicable,
}

/// Why an engine produced no result at all. This is deliberately distinct
/// from individual unreadable entries *within* a scan, which are skipped
/// silently and simply absent from the resulting tree: a `ScanError` means
/// the caller should fall back to another engine or surface a real failure,
/// not treat the outcome as "scanned, found nothing."
#[derive(Debug)]
pub enum ScanError {
    /// The engine cannot run against this target (mirrors a non-`Available`
    /// capability check). Unconstructed until an engine that can actually
    /// be unavailable (the v2 MFT reader) exists.
    #[allow(dead_code)]
    Unavailable(Availability),
    /// The scan root itself could not be read.
    RootUnreadable(std::io::Error),
}

/// The complete outcome of one scan generation.
///
/// Cancellation is normal control flow and is intentionally not represented
/// as an empty or partial `Entry`. A caller can therefore avoid installing a
/// cancelled tree or showing a failure dialog.
#[derive(Debug)]
pub enum ScanOutcome {
    /// The engine produced the authoritative tree.
    Success(Entry),
    /// Cooperative cancellation stopped this generation.
    Cancelled,
    /// The engine could not produce a result.
    Failed(ScanError),
    /// The worker contained a panic raised by the engine.
    #[allow(dead_code)]
    Panicked,
}

/// Shared, lock-free progress state a caller can poll from another thread to
/// show "N files / X GB scanned so far" while a scan is in flight. Counters
/// only ever increase during a scan; `complete` flips once after the scanner
/// and background display-tree preparation finish, so pollers have a final
/// "no longer in flight" state to observe.
#[derive(Default)]
pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub dirs_scanned: AtomicU64,
    pub bytes_scanned: AtomicU64,
    conversion_nodes_total: AtomicU64,
    conversion_nodes_finished: AtomicU64,
    conversion_started: AtomicBool,
    conversion_complete: AtomicBool,
    complete: AtomicBool,
}

impl ScanProgress {
    pub(crate) fn start_conversion(&self, total: u64) {
        self.conversion_nodes_total.store(total, Ordering::Relaxed);
        self.conversion_nodes_finished.store(0, Ordering::Relaxed);
        self.conversion_started.store(true, Ordering::Release);
        self.conversion_complete.store(false, Ordering::Release);
    }

    pub(crate) fn conversion_node_finished(&self) {
        self.conversion_nodes_finished
            .fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn finish_conversion(&self) {
        self.conversion_complete.store(true, Ordering::Release);
    }

    pub(crate) fn conversion_started(&self) -> bool {
        self.conversion_started.load(Ordering::Acquire)
    }

    pub(crate) fn conversion_complete(&self) -> bool {
        self.conversion_complete.load(Ordering::Acquire)
    }

    pub(crate) fn conversion_progress(&self) -> f32 {
        if self.conversion_complete() {
            return 1.0;
        }
        let total = self.conversion_nodes_total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let finished = self
            .conversion_nodes_finished
            .load(Ordering::Relaxed)
            .min(total);
        finished as f32 / total as f32
    }

    #[cfg(test)]
    pub(crate) fn conversion_counts(&self) -> (u64, u64) {
        (
            self.conversion_nodes_total.load(Ordering::Relaxed),
            self.conversion_nodes_finished.load(Ordering::Relaxed),
        )
    }

    pub(crate) fn mark_incomplete(&self) {
        self.complete.store(false, Ordering::Release);
    }

    pub fn mark_complete(&self) {
        self.complete.store(true, Ordering::Release);
    }

    #[allow(dead_code)]
    pub fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Relaxed)
    }
}

/// A live-discovery notification an engine may emit while scanning, letting
/// the UI grow its treemap before the final tree is available.
#[derive(Debug, Clone)]
pub enum ScanEvent {
    Discovered {
        scan_id: ScanId,
        path: PathBuf,
        size: u64,
        is_dir: bool,
    },
}

impl ScanEvent {
    fn with_scan_id(self, scan_id: ScanId) -> Self {
        match self {
            Self::Discovered {
                path, size, is_dir, ..
            } => Self::Discovered {
                scan_id,
                path,
                size,
                is_dir,
            },
        }
    }
}

/// Everything an engine needs to communicate with the rest of the app while
/// a scan runs: cooperative cancellation, pollable progress, and an
/// *optional, best-effort* event sink for live discovery.
///
/// The event sink is best-effort by contract: the walker emits an event per
/// discovered entry as it goes, but an engine that structurally cannot
/// stream (a future MFT reader does one linear pass over the raw `$MFT`,
/// then reconstructs the tree bottom-up in memory) may emit late, coarsely,
/// or not at all. Callers get smooth live fill-in when the engine can offer
/// it, and must not depend on it; the authoritative result is always the
/// final `Entry` tree returned by [`ScanEngine::scan`].
pub struct ScanContext {
    /// Generation identity copied onto every emitted event.
    pub scan_id: ScanId,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<ScanProgress>,
    pub events: Option<SyncSender<ScanEvent>>,
}

impl ScanContext {
    pub fn new() -> Self {
        Self {
            scan_id: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(ScanProgress::default()),
            events: None,
        }
    }

    pub fn with_scan_id(mut self, scan_id: ScanId) -> Self {
        self.scan_id = scan_id;
        self
    }

    pub fn with_events(mut self, sender: SyncSender<ScanEvent>) -> Self {
        self.events = Some(sender);
        self
    }

    /// Returns the UI-side view of this context without retaining an event
    /// sender. Dropping the event receiver can therefore disconnect the
    /// worker's live preview channel when a generation is superseded.
    pub fn ui_handle(&self) -> Self {
        Self {
            scan_id: self.scan_id,
            cancel: self.cancel.clone(),
            progress: self.progress.clone(),
            events: None,
        }
    }

    /// Sends a discovery event if a sink is attached, ignoring a
    /// disconnected receiver (the UI hanging up is not the scan's problem).
    pub(crate) fn emit(&self, event: ScanEvent) {
        if let Some(sender) = &self.events {
            // Live discovery is provisional. A full or disconnected queue is
            // intentionally ignored so scanner traversal never waits for UI
            // capacity and never treats a closed preview as scan failure.
            let _ = sender.try_send(event.with_scan_id(self.scan_id));
        }
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

impl Default for ScanContext {
    fn default() -> Self {
        Self::new()
    }
}

/// A scanning backend. The parallel directory walker implements this today;
/// a v2 NTFS `$MFT` reader will implement it later, and the UI orchestration
/// layer drives whichever engine it holds only through this interface.
pub trait ScanEngine: Send + Sync {
    /// Short human-readable engine name, for status display.
    fn name(&self) -> &'static str;

    /// Whether this engine can scan `target`. Callers should check this
    /// before [`ScanEngine::scan`] and fall back to another engine on
    /// anything other than [`Availability::Available`].
    fn is_available(&self, target: &Path) -> Availability;

    /// Scans `target`, returning a typed success, cancellation, or failure
    /// outcome. Individual unreadable entries are skipped and simply absent
    /// from a successful tree. Implementations must honor `ctx.cancel`, keep
    /// `ctx.progress` monotonically increasing, and call `mark_complete`
    /// before returning.
    fn scan(&self, target: &Path, ctx: &ScanContext) -> ScanOutcome;
}
