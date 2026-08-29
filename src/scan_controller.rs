//! Generation-aware, GUI-free orchestration for background scans.
//!
//! The controller owns the current worker, its live-event receiver, and the
//! completion hand-off. Starting a new generation retires the old worker
//! asynchronously; the UI only polls messages and never joins a superseded
//! handle itself.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::display_tree::DisplayNode;
use crate::scanner::{
    live_event_channel, ScanContext, ScanEngine, ScanError, ScanEvent, ScanId, ScanOutcome,
    ScanProgress,
};

#[derive(Debug)]
pub(crate) enum PreparedOutcome {
    Success(DisplayNode),
    Cancelled,
    Failed(ScanError),
    Panicked,
}

struct WorkerCompletion {
    id: ScanId,
    outcome: PreparedOutcome,
}

struct ActiveGeneration {
    id: ScanId,
    engine_name: &'static str,
    ctx: ScanContext,
    events: Receiver<ScanEvent>,
    completion: Receiver<WorkerCompletion>,
    handle: Option<JoinHandle<()>>,
}

/// A completed generation result tagged with the generation that produced it.
pub(crate) struct ScanCompletion {
    pub(crate) id: ScanId,
    pub(crate) engine_name: &'static str,
    pub(crate) progress: Arc<ScanProgress>,
    pub(crate) outcome: PreparedOutcome,
}

/// Owns and joins retired worker handles away from the UI thread.
///
/// Its own thread handle is retained (not discarded) so that dropping a
/// `WorkerReaper` can synchronously close the channel and join the thread,
/// guaranteeing every handle queued up to that point is actually joined
/// before shutdown proceeds — a bare `Sender` drop only lets the thread
/// *start* draining, it doesn't wait for it to finish.
struct WorkerReaper {
    sender: Option<Sender<JoinHandle<()>>>,
    joined: Arc<AtomicUsize>,
    handle: Option<JoinHandle<()>>,
}

impl WorkerReaper {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<JoinHandle<()>>();
        let joined = Arc::new(AtomicUsize::new(0));
        let reaper_joined = joined.clone();
        let handle = thread::Builder::new()
            .name("bytewhiffer-scan-reaper".to_owned())
            .spawn(move || {
                while let Ok(handle) = receiver.recv() {
                    let _ = handle.join();
                    reaper_joined.fetch_add(1, Ordering::Release);
                }
            })
            .expect("scan reaper thread must start");
        Self {
            sender: Some(sender),
            joined,
            handle: Some(handle),
        }
    }

    fn retire(&self, handle: JoinHandle<()>) {
        let Some(sender) = self.sender.as_ref() else {
            // The reaper thread has already been shut down. Preserve the
            // invariant that every handle is joined even in this edge case.
            let _ = handle.join();
            self.joined.fetch_add(1, Ordering::Release);
            return;
        };
        if let Err(error) = sender.send(handle) {
            let _ = error.0.join();
            self.joined.fetch_add(1, Ordering::Release);
        }
    }

    #[cfg(test)]
    fn joined_count(&self) -> usize {
        self.joined.load(Ordering::Acquire)
    }
}

impl Drop for WorkerReaper {
    fn drop(&mut self) {
        // Close the channel first so the reaper thread's `recv()` loop drains
        // every handle already queued and then returns, then join it
        // synchronously so shutdown cannot proceed until that drain is done.
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Owns exactly one authoritative scan generation at a time.
pub struct ScanController {
    next_id: ScanId,
    current: Option<ActiveGeneration>,
    reaper: WorkerReaper,
}

impl ScanController {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            current: None,
            reaper: WorkerReaper::new(),
        }
    }

    /// Starts `engine` as the sole current generation, retiring the prior
    /// worker first. The returned ID is also copied onto every event and
    /// completion produced by this generation.
    pub fn start(&mut self, target: PathBuf, engine: Box<dyn ScanEngine>) -> ScanId {
        self.retire_current();
        let id = self.next_id();
        let (event_sender, events) = live_event_channel();
        let worker_ctx = ScanContext::new()
            .with_scan_id(id)
            .with_events(event_sender);
        let ui_ctx = worker_ctx.ui_handle();
        let progress = worker_ctx.progress.clone();
        let (completion_sender, completion) = mpsc::channel();
        let engine_name = engine.name();
        let worker_target = target.clone();
        let worker_handle = thread::Builder::new()
            .name(format!("bytewhiffer-scan-{id}"))
            .spawn(move || {
                let outcome = match catch_unwind(AssertUnwindSafe(|| {
                    let outcome = engine.scan(&worker_target, &worker_ctx);
                    let outcome = if worker_ctx.cancel.load(Ordering::Acquire) {
                        match outcome {
                            // Cancellation wins a race where a fake or future
                            // engine returns success immediately after the UI
                            // retires the generation.
                            ScanOutcome::Success(_) => ScanOutcome::Cancelled,
                            other => other,
                        }
                    } else {
                        outcome
                    };

                    match outcome {
                        ScanOutcome::Success(entry) => {
                            // Engines mark their traversal complete before
                            // returning. Preparation is still part of this
                            // generation, so reopen the progress window until
                            // the display tree has been fully built.
                            worker_ctx.progress.mark_incomplete();
                            DisplayNode::from_owned_entry_with_progress(
                                entry,
                                &worker_ctx.progress,
                                &worker_ctx.cancel,
                            )
                            .map(PreparedOutcome::Success)
                            .unwrap_or(PreparedOutcome::Cancelled)
                        }
                        ScanOutcome::Cancelled => PreparedOutcome::Cancelled,
                        ScanOutcome::Failed(error) => PreparedOutcome::Failed(error),
                        ScanOutcome::Panicked => PreparedOutcome::Panicked,
                    }
                })) {
                    Ok(outcome) => outcome,
                    Err(_) => PreparedOutcome::Panicked,
                };
                progress.mark_complete();
                let _ = completion_sender.send(WorkerCompletion { id, outcome });
            })
            .expect("scan worker thread must start");

        self.current = Some(ActiveGeneration {
            id,
            engine_name,
            ctx: ui_ctx,
            events,
            completion,
            handle: Some(worker_handle),
        });
        id
    }

    /// Cancels the current worker. Its completion is still reaped and can be
    /// polled as `PreparedOutcome::Cancelled`; no partial display tree is
    /// synthesized.
    pub fn cancel_current(&self) {
        if let Some(active) = &self.current {
            active.ctx.cancel.store(true, Ordering::Release);
        }
    }

    /// Returns and removes at most one frame's worth of current-generation
    /// live events. Events tagged for any other generation are discarded.
    pub fn take_events(&mut self, budget: Duration) -> Vec<ScanEvent> {
        let Some(active) = &mut self.current else {
            return Vec::new();
        };
        let started = Instant::now();
        let mut events = Vec::new();
        while let Ok(event) = active.events.try_recv() {
            if event_scan_id(&event) == active.id {
                events.push(event);
            }
            if !events.is_empty() && started.elapsed() >= budget {
                break;
            }
        }
        events
    }

    /// Polls the current generation's completion without joining on the UI
    /// thread. The worker handle is handed to the reaper as soon as its result
    /// is observed.
    pub fn poll_completion(&mut self) -> Option<ScanCompletion> {
        let message = {
            let active = self.current.as_ref()?;
            match active.completion.try_recv() {
                Ok(message) => Some(message),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    if active.handle.as_ref().is_some_and(JoinHandle::is_finished) {
                        Some(WorkerCompletion {
                            id: active.id,
                            outcome: PreparedOutcome::Panicked,
                        })
                    } else {
                        None
                    }
                }
            }
        }?;

        let mut active = self.current.take()?;
        let id_matches = message.id == active.id;
        if let Some(handle) = active.handle.take() {
            self.reaper.retire(handle);
        }
        if !id_matches {
            return None;
        }
        Some(ScanCompletion {
            id: active.id,
            engine_name: active.engine_name,
            progress: active.ctx.progress,
            outcome: message.outcome,
        })
    }

    pub fn current_id(&self) -> Option<ScanId> {
        self.current.as_ref().map(|active| active.id)
    }

    pub fn current_progress(&self) -> Option<Arc<ScanProgress>> {
        self.current
            .as_ref()
            .map(|active| active.ctx.progress.clone())
    }

    pub fn is_active(&self) -> bool {
        self.current.is_some()
    }

    fn next_id(&mut self) -> ScanId {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1);
            if self.next_id == 0 {
                self.next_id = 1;
            }
            if id != 0 && self.current.as_ref().is_none_or(|active| active.id != id) {
                return id;
            }
        }
    }

    fn retire_current(&mut self) {
        let Some(mut active) = self.current.take() else {
            return;
        };
        active.ctx.cancel.store(true, Ordering::Release);
        // Dropping the receiver immediately stops accepting late preview
        // events from the retired generation.
        drop(active.events);
        drop(active.completion);
        if let Some(handle) = active.handle.take() {
            self.reaper.retire(handle);
        }
    }

    #[cfg(test)]
    fn reaped_workers(&self) -> usize {
        self.reaper.joined_count()
    }
}

impl Default for ScanController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ScanController {
    fn drop(&mut self) {
        self.retire_current();
        // `reaper` drops next (declaration order), which closes its channel
        // and synchronously joins its thread — guaranteeing the handle just
        // queued above is actually joined before this call returns.
    }
}

fn event_scan_id(event: &ScanEvent) -> ScanId {
    match event {
        ScanEvent::Discovered { scan_id, .. } => *scan_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Availability, Entry};
    use std::path::Path;
    use std::sync::atomic::AtomicBool;
    use std::thread;

    fn tree(name: &str) -> Entry {
        Entry {
            name: name.to_owned(),
            path: PathBuf::from(name),
            size: 1,
            is_dir: true,
            children: Vec::new(),
        }
    }

    struct ReturnEngine {
        outcome: MutexOutcome,
    }

    struct MutexOutcome(std::sync::Mutex<Option<ScanOutcome>>);

    impl ReturnEngine {
        fn success(name: &str) -> Self {
            let _ = name;
            Self {
                outcome: MutexOutcome(std::sync::Mutex::new(Some(ScanOutcome::Success(tree(
                    "result",
                ))))),
            }
        }
    }

    impl ScanEngine for ReturnEngine {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn is_available(&self, _target: &Path) -> crate::scanner::Availability {
            Availability::Available
        }

        fn scan(&self, _target: &Path, _ctx: &ScanContext) -> ScanOutcome {
            self.outcome
                .0
                .lock()
                .unwrap()
                .take()
                .expect("fake engine runs once")
        }
    }

    struct BlockingEngine {
        started: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
        panic: bool,
    }

    impl ScanEngine for BlockingEngine {
        fn name(&self) -> &'static str {
            "blocking-fake"
        }

        fn is_available(&self, _target: &Path) -> Availability {
            Availability::Available
        }

        fn scan(&self, _target: &Path, ctx: &ScanContext) -> ScanOutcome {
            self.started.store(true, Ordering::Release);
            if self.panic {
                panic!("synthetic worker panic");
            }
            while !self.release.load(Ordering::Acquire) {
                if ctx.is_cancelled() {
                    return ScanOutcome::Cancelled;
                }
                thread::yield_now();
            }
            ScanOutcome::Success(tree("blocking"))
        }
    }

    struct EventBlockingEngine {
        started: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    impl ScanEngine for EventBlockingEngine {
        fn name(&self) -> &'static str {
            "event-blocking-fake"
        }

        fn is_available(&self, _target: &Path) -> Availability {
            Availability::Available
        }

        fn scan(&self, _target: &Path, ctx: &ScanContext) -> ScanOutcome {
            ctx.emit(ScanEvent::Discovered {
                scan_id: 999,
                path: PathBuf::from("early"),
                size: 1,
                is_dir: false,
            });
            self.started.store(true, Ordering::Release);
            while !self.release.load(Ordering::Acquire) {
                thread::yield_now();
            }
            // This event is emitted after supersession in the test. The old
            // receiver must already be disconnected, so it cannot leak into
            // the new generation's state.
            ctx.emit(ScanEvent::Discovered {
                scan_id: 999,
                path: PathBuf::from("late"),
                size: 1,
                is_dir: false,
            });
            ScanOutcome::Success(tree("old"))
        }
    }

    fn wait_for_completion(controller: &mut ScanController) -> ScanCompletion {
        for _ in 0..2_000 {
            if let Some(completion) = controller.poll_completion() {
                return completion;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("fake scan did not complete")
    }

    #[test]
    fn immediate_scan_completes_with_a_generation_id() {
        let mut controller = ScanController::new();
        let id = controller.start(PathBuf::from("a"), Box::new(ReturnEngine::success("a")));
        let completion = wait_for_completion(&mut controller);
        assert_eq!(completion.id, id);
        assert!(matches!(completion.outcome, PreparedOutcome::Success(_)));
    }

    #[test]
    fn supersession_cancels_old_generation_and_only_new_result_is_current() {
        let mut controller = ScanController::new();
        let old_started = Arc::new(AtomicBool::new(false));
        let old_release = Arc::new(AtomicBool::new(false));
        let old_id = controller.start(
            PathBuf::from("a"),
            Box::new(BlockingEngine {
                started: old_started.clone(),
                release: old_release.clone(),
                panic: false,
            }),
        );
        while !old_started.load(Ordering::Acquire) {
            thread::yield_now();
        }

        let new_id = controller.start(PathBuf::from("b"), Box::new(ReturnEngine::success("b")));
        old_release.store(true, Ordering::Release);
        let completion = wait_for_completion(&mut controller);
        assert_eq!(completion.id, new_id);
        assert_ne!(old_id, new_id);
        assert!(matches!(completion.outcome, PreparedOutcome::Success(_)));
        for _ in 0..2_000 {
            if controller.reaped_workers() >= 1 {
                return;
            }
            thread::yield_now();
        }
        panic!("superseded worker was not reaped");
    }

    #[test]
    fn cancellation_wins_a_success_race_and_publishes_no_partial_tree() {
        let mut controller = ScanController::new();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        controller.start(
            PathBuf::from("cancel"),
            Box::new(BlockingEngine {
                started: started.clone(),
                release: release.clone(),
                panic: false,
            }),
        );
        while !started.load(Ordering::Acquire) {
            thread::yield_now();
        }
        controller.cancel_current();
        let completion = wait_for_completion(&mut controller);
        assert!(matches!(completion.outcome, PreparedOutcome::Cancelled));
    }

    #[test]
    fn panic_is_contained_and_a_later_generation_can_complete() {
        let mut controller = ScanController::new();
        controller.start(
            PathBuf::from("panic"),
            Box::new(BlockingEngine {
                started: Arc::new(AtomicBool::new(false)),
                release: Arc::new(AtomicBool::new(false)),
                panic: true,
            }),
        );
        let first = wait_for_completion(&mut controller);
        assert!(matches!(first.outcome, PreparedOutcome::Panicked));

        controller.start(
            PathBuf::from("after"),
            Box::new(ReturnEngine::success("after")),
        );
        let second = wait_for_completion(&mut controller);
        assert!(matches!(second.outcome, PreparedOutcome::Success(_)));
    }

    #[test]
    fn live_events_are_tagged_and_late_events_are_disconnected() {
        let mut controller = ScanController::new();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let old_id = controller.start(
            PathBuf::from("a"),
            Box::new(EventBlockingEngine {
                started: started.clone(),
                release: release.clone(),
            }),
        );
        while !started.load(Ordering::Acquire) {
            thread::yield_now();
        }
        let early = controller.take_events(Duration::from_millis(1));
        assert_eq!(early.len(), 1);
        assert!(matches!(
            early[0],
            ScanEvent::Discovered { scan_id, .. } if scan_id == old_id
        ));

        let new_id = controller.start(PathBuf::from("b"), Box::new(ReturnEngine::success("b")));
        release.store(true, Ordering::Release);
        let completion = wait_for_completion(&mut controller);
        assert_eq!(completion.id, new_id);
        assert!(controller.take_events(Duration::from_millis(1)).is_empty());
    }

    #[test]
    fn ids_skip_zero_and_advance_across_generations() {
        let mut controller = ScanController::new();
        let first = controller.start(PathBuf::from("a"), Box::new(ReturnEngine::success("a")));
        let _ = wait_for_completion(&mut controller);
        let second = controller.start(PathBuf::from("b"), Box::new(ReturnEngine::success("b")));
        assert_ne!(first, 0);
        assert!(second > first);
    }

    #[test]
    fn dropping_the_reaper_synchronously_joins_every_queued_handle() {
        let reaper = WorkerReaper::new();
        let joined = reaper.joined.clone();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let worker_started = started.clone();
        let worker_release = release.clone();
        let handle = thread::Builder::new()
            .spawn(move || {
                worker_started.store(true, Ordering::Release);
                while !worker_release.load(Ordering::Acquire) {
                    thread::yield_now();
                }
            })
            .expect("worker thread must start");
        reaper.retire(handle);
        while !started.load(Ordering::Acquire) {
            thread::yield_now();
        }
        // The worker is still running and unjoined at this point.
        assert_eq!(joined.load(Ordering::Acquire), 0);
        release.store(true, Ordering::Release);

        // Dropping the reaper must not return until its thread has drained
        // the queue and joined the worker above — this is the shutdown
        // invariant a bare `Sender` drop cannot provide.
        drop(reaper);
        assert_eq!(joined.load(Ordering::Acquire), 1);
    }
}
